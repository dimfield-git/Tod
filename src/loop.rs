use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::RunConfig;
use crate::editor::{create_edits, format_file_context, EditError};
use crate::llm::LlmProvider;
use crate::planner::{create_plan, Plan, PlanError};
use crate::reviewer::{review, ReviewDecision};
use crate::runner::{apply_edits, run_pipeline, ApplyError, RunResult};
use crate::schema::validate_path;

// ---------------------------------------------------------------------------
// State structs
// ---------------------------------------------------------------------------

/// Step-scoped state. Reset cleanly when moving to the next plan step.
///
/// Kept deliberately narrow: only data that is meaningful within a single
/// step's retry cycle. Anything run-scoped lives on `RunState`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    /// Current attempt within this step (1-indexed after the first increment).
    /// Starts at 0; incremented before each iteration's work begins.
    pub attempt: usize,
    /// Truncated runner output from the previous failed attempt.
    /// `None` on the first attempt. Replaced (not appended) on each retry.
    pub retry_context: Option<String>,
}

impl StepState {
    fn new() -> Self {
        Self {
            attempt: 0,
            retry_context: None,
        }
    }
}

/// Run-level state. Owns the plan and tracks progress across all steps.
///
/// Designed to be cheaply serializable (data-only, no handles or references)
/// so it can serve as a checkpoint for logging and future resume support.
///
/// Config-derived caps are copied in so checkpoints are self-contained —
/// a deserialized `RunState` can be understood without the original config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunState {
    /// The goal string that was passed to the planner.
    pub goal: String,
    /// The plan produced by the planner. Immutable once set.
    pub plan: Plan,
    /// Index of the step currently being worked on (0-indexed).
    pub step_index: usize,
    /// Mutable state for the current step's retry cycle.
    pub step_state: StepState,
    /// Number of steps that have reached `Proceed`.
    pub steps_completed: usize,
    /// Total edit→apply→run→review cycles executed across all steps.
    pub total_iterations: usize,
    /// Cap: max fix iterations per plan step (copied from config).
    pub max_iterations_per_step: usize,
    /// Cap: max total iterations across all steps (copied from config).
    pub max_total_iterations: usize,
}

impl RunState {
    fn new(goal: String, plan: Plan, config: &RunConfig) -> Self {
        Self {
            goal,
            plan,
            step_index: 0,
            step_state: StepState::new(),
            steps_completed: 0,
            total_iterations: 0,
            max_iterations_per_step: config.max_iterations_per_step,
            max_total_iterations: config.max_total_iterations,
        }
    }

    /// Produce a `LoopReport` from the current state.
    fn report(&self) -> LoopReport {
        LoopReport {
            steps_completed: self.steps_completed,
            total_iterations: self.total_iterations,
        }
    }

    /// Write state to the checkpoint location.
    ///
    /// Currently a no-op — will be wired to `.agent/run.json` in Phase 6.
    fn checkpoint(&self, _config: &RunConfig) {
        // Phase 6: serde_json::to_writer(file, self)
    }
}

// ---------------------------------------------------------------------------
// Report + Error (public API, unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct LoopReport {
    pub steps_completed: usize,
    pub total_iterations: usize,
}

#[derive(Debug)]
pub enum LoopError {
    Plan(PlanError),
    Edit {
        step_index: usize,
        iteration: usize,
        source: EditError,
    },
    Apply {
        step_index: usize,
        iteration: usize,
        source: ApplyError,
    },
    Io {
        path: String,
        cause: String,
    },
    InvalidPlanPath {
        step_index: usize,
        path: String,
        reason: String,
    },
    Aborted {
        step_index: usize,
        reason: String,
    },
    TotalIterationCap {
        max_total_iterations: usize,
    },
}

impl std::fmt::Display for LoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(e) => write!(f, "plan failed: {e}"),
            Self::Edit {
                step_index,
                iteration,
                source,
            } => write!(
                f,
                "edit creation failed (step {}, iteration {}): {source}",
                step_index + 1,
                iteration
            ),
            Self::Apply {
                step_index,
                iteration,
                source,
            } => write!(
                f,
                "edit application failed (step {}, iteration {}): {source}",
                step_index + 1,
                iteration
            ),
            Self::Io { path, cause } => write!(f, "I/O error for {path}: {cause}"),
            Self::InvalidPlanPath {
                step_index,
                path,
                reason,
            } => write!(
                f,
                "invalid plan path at step {} ({path}): {reason}",
                step_index + 1
            ),
            Self::Aborted { step_index, reason } => {
                write!(f, "run aborted at step {}: {reason}", step_index + 1)
            }
            Self::TotalIterationCap {
                max_total_iterations,
            } => write!(
                f,
                "reached total iteration cap: {max_total_iterations}"
            ),
        }
    }
}

impl std::error::Error for LoopError {}

impl From<PlanError> for LoopError {
    fn from(value: PlanError) -> Self {
        Self::Plan(value)
    }
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Run the full agent loop: plan → (edit → apply → run → review) × N.
///
/// Public API is unchanged — callers pass a provider, goal, and config,
/// and get back a report or error. Internally, all mutable state lives in
/// `RunState` / `StepState` so it can be inspected, serialized, or (later)
/// resumed from a checkpoint.
pub fn run(
    provider: &dyn LlmProvider,
    goal: &str,
    config: &RunConfig,
) -> Result<LoopReport, LoopError> {
    let project_context = build_project_context(&config.project_root)?;
    let plan = create_plan(provider, goal, &project_context)?;

    let mut state = RunState::new(goal.to_string(), plan, config);

    // Checkpoint: plan created, about to start step 0.
    state.checkpoint(config);

    while state.step_index < state.plan.steps.len() {
        let step = state.plan.steps[state.step_index].clone();
        let mut step_succeeded = false;

        while state.step_state.attempt < state.max_iterations_per_step {
            // --- Global cap guard ---
            if state.total_iterations >= state.max_total_iterations {
                return Err(LoopError::TotalIterationCap {
                    max_total_iterations: state.max_total_iterations,
                });
            }

            state.step_state.attempt += 1;
            state.total_iterations += 1;

            // --- Build file context, append retry feedback if present ---
            let mut file_context = build_step_file_context(
                &config.project_root,
                &step.files,
                state.step_index,
            )?;
            if let Some(ctx) = &state.step_state.retry_context {
                file_context.push_str("\n## Previous runner failure\n");
                file_context.push_str(ctx);
            }

            // --- Generate edits ---
            let batch = create_edits(
                provider,
                &step,
                &file_context,
                &config.project_root,
            )
            .map_err(|source| LoopError::Edit {
                step_index: state.step_index,
                iteration: state.step_state.attempt,
                source,
            })?;

            // --- Apply + run (or skip in dry-run) ---
            let run_result = if config.dry_run {
                RunResult::Success
            } else {
                apply_edits(&batch, &config.project_root).map_err(|source| {
                    LoopError::Apply {
                        step_index: state.step_index,
                        iteration: state.step_state.attempt,
                        source,
                    }
                })?;
                run_pipeline(config)
            };

            // --- Review and update step state ---
            match review(
                &run_result,
                state.step_state.attempt,
                state.max_iterations_per_step,
            ) {
                ReviewDecision::Proceed => {
                    step_succeeded = true;
                    // Checkpoint: end of iteration (success).
                    state.checkpoint(config);
                    break;
                }
                ReviewDecision::Retry { error_context } => {
                    state.step_state.retry_context = Some(error_context);
                    // Checkpoint: end of iteration (retry).
                    state.checkpoint(config);
                }
                ReviewDecision::Abort { reason } => {
                    return Err(LoopError::Aborted {
                        step_index: state.step_index,
                        reason,
                    });
                }
            }
        }

        if !step_succeeded {
            return Err(LoopError::Aborted {
                step_index: state.step_index,
                reason: "step did not reach success within per-step cap".to_string(),
            });
        }

        // --- Advance to next step with a clean StepState ---
        state.steps_completed += 1;
        state.step_index += 1;
        state.step_state = StepState::new();

        // Checkpoint: step completed, about to start next (or finish).
        state.checkpoint(config);
    }

    Ok(state.report())
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Maximum directory depth to recurse when building project context.
const MAX_TREE_DEPTH: usize = 12;

/// Maximum files to list in the planner context.
const MAX_LISTED_FILES: usize = 200;

fn build_project_context(project_root: &Path) -> Result<String, LoopError> {
    let mut files = Vec::new();
    collect_paths(project_root, project_root, &mut files, 0)?;
    files.sort();

    let mut out = String::from("Project file tree:\n");
    for file in files.into_iter().take(MAX_LISTED_FILES) {
        out.push_str("- ");
        out.push_str(&file);
        out.push('\n');
    }

    // Cargo.toml is high-signal context for the planner.
    let cargo_path = project_root.join("Cargo.toml");
    if let Ok(contents) = fs::read_to_string(&cargo_path) {
        out.push_str("\n---\nCargo.toml:\n");
        out.push_str(&truncate_context(&contents, 8 * 1024));
    }

    Ok(out)
}

fn collect_paths(
    root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
    depth: usize,
) -> Result<(), LoopError> {
    if depth > MAX_TREE_DEPTH {
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| LoopError::Io {
        path: dir.display().to_string(),
        cause: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| LoopError::Io {
            path: dir.display().to_string(),
            cause: e.to_string(),
        })?;
        let path = entry.path();
        let ty = entry.file_type().map_err(|e| LoopError::Io {
            path: path.display().to_string(),
            cause: e.to_string(),
        })?;

        if ty.is_dir() {
            let name = entry.file_name();
            if name == ".git" || name == "target" {
                continue;
            }
            collect_paths(root, &path, out, depth + 1)?;
        } else if ty.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| LoopError::Io {
                    path: path.display().to_string(),
                    cause: e.to_string(),
                })?
                .to_string_lossy()
                .to_string();
            out.push(rel);
        }
    }

    Ok(())
}

fn build_step_file_context(
    project_root: &Path,
    files: &[String],
    step_index: usize,
) -> Result<String, LoopError> {
    let mut out = String::new();

    for rel in files {
        let full = validate_path(rel, project_root).map_err(|e| LoopError::InvalidPlanPath {
            step_index,
            path: rel.clone(),
            reason: e.to_string(),
        })?;

        match fs::read_to_string(&full) {
            Ok(content) => {
                out.push_str(&format_file_context(rel, &content));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                out.push_str(&format!("=== {rel} ===\n<missing file>\n"));
            }
            Err(e) => {
                return Err(LoopError::Io {
                    path: full.display().to_string(),
                    cause: e.to_string(),
                });
            }
        }
        out.push('\n');
    }

    Ok(out)
}

/// Truncate a string for context inclusion, snapping to a UTF-8 boundary.
fn truncate_context(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let kept = &s[..end];
    format!("{kept}\n\n... [truncated {} bytes] ...", s.len() - end)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::ops::Deref;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use crate::config::{RunConfig, RunMode};
    use crate::llm::{LlmError, LlmProvider};

    // -- RAII temp directory (Drop guard, consistent with runner.rs) -------

    static TEST_ID: AtomicUsize = AtomicUsize::new(0);

    struct TempSandbox(PathBuf);

    impl TempSandbox {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "tod_loop_test_{}_{id}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Create the sandbox with a `src/main.rs` already present.
        fn with_main_rs() -> Self {
            let sb = Self::new();
            fs::create_dir_all(sb.join("src")).unwrap();
            fs::write(sb.join("src/main.rs"), "fn main() {}\n").unwrap();
            sb
        }
    }

    impl Deref for TempSandbox {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // -- Fake providers ---------------------------------------------------

    struct QueueProvider {
        responses: Mutex<VecDeque<String>>,
    }

    impl QueueProvider {
        fn from(responses: Vec<&str>) -> Self {
            let q = responses.into_iter().map(|s| s.to_string()).collect();
            Self {
                responses: Mutex::new(q),
            }
        }
    }

    impl LlmProvider for QueueProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<String, LlmError> {
            let mut lock = self.responses.lock().unwrap();
            lock.pop_front()
                .ok_or_else(|| LlmError::RequestFailed("no fake response queued".to_string()))
        }
    }

    // -- Existing behavior tests ------------------------------------------

    #[test]
    fn dry_run_completes_plan() {
        let sandbox = TempSandbox::with_main_rs();

        let provider = QueueProvider::from(vec![
            r#"{"steps":[{"description":"update main","files":["src/main.rs"]}]}"#,
            r#"{"edits":[{"action":"replace_range","path":"src/main.rs","start_line":1,"end_line":1,"content":"fn main() { println!(\"ok\"); }"}]}"#,
        ]);

        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            mode: RunMode::Default,
            max_iterations_per_step: 2,
            max_total_iterations: 3,
            dry_run: true,
            max_runner_output_bytes: 4096,
        };

        let report = run(&provider, "update", &config).unwrap();
        assert_eq!(report.steps_completed, 1);
        assert_eq!(report.total_iterations, 1);

        // Dry run must not modify disk.
        let content = fs::read_to_string(sandbox.join("src/main.rs")).unwrap();
        assert_eq!(content, "fn main() {}\n");
    }

    #[test]
    fn total_iteration_cap_is_enforced() {
        let sandbox = TempSandbox::with_main_rs();

        let provider = QueueProvider::from(vec![
            r#"{"steps":[{"description":"do 1","files":["src/main.rs"]},{"description":"do 2","files":["src/main.rs"]}]}"#,
            r#"{"edits":[{"action":"write_file","path":"src/main.rs","content":"fn main(){}"}]}"#,
        ]);

        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            mode: RunMode::Default,
            max_iterations_per_step: 5,
            max_total_iterations: 1,
            dry_run: true,
            max_runner_output_bytes: 4096,
        };

        let err = run(&provider, "goal", &config).unwrap_err();
        assert!(matches!(err, LoopError::TotalIterationCap { .. }));
    }

    // -- State struct unit tests ------------------------------------------

    #[test]
    fn step_state_new_is_clean() {
        let ss = StepState::new();
        assert_eq!(ss.attempt, 0);
        assert!(ss.retry_context.is_none());
    }

    #[test]
    fn run_state_new_starts_at_zero() {
        let plan = Plan {
            steps: vec![crate::planner::PlanStep {
                description: "test".into(),
                files: vec!["a.rs".into()],
            }],
        };
        let config = RunConfig::default();
        let rs = RunState::new("test goal".into(), plan, &config);
        assert_eq!(rs.goal, "test goal");
        assert_eq!(rs.step_index, 0);
        assert_eq!(rs.steps_completed, 0);
        assert_eq!(rs.total_iterations, 0);
        assert_eq!(rs.step_state.attempt, 0);
        assert_eq!(rs.max_iterations_per_step, config.max_iterations_per_step);
        assert_eq!(rs.max_total_iterations, config.max_total_iterations);
    }

    #[test]
    fn step_state_reset_on_advance() {
        let plan = Plan {
            steps: vec![crate::planner::PlanStep {
                description: "test".into(),
                files: vec!["a.rs".into()],
            }],
        };
        let config = RunConfig::default();
        let mut rs = RunState::new("goal".into(), plan, &config);

        // Simulate work on step 0.
        rs.step_state.attempt = 3;
        rs.step_state.retry_context = Some("some error".into());
        rs.steps_completed = 1;
        rs.step_index = 1;

        // Reset for next step.
        rs.step_state = StepState::new();

        assert_eq!(rs.step_state.attempt, 0);
        assert!(rs.step_state.retry_context.is_none());
        // Run-level counters must survive reset.
        assert_eq!(rs.steps_completed, 1);
        assert_eq!(rs.step_index, 1);
    }

    #[test]
    fn report_reflects_state() {
        let plan = Plan { steps: vec![] };
        let config = RunConfig::default();
        let mut rs = RunState::new("goal".into(), plan, &config);
        rs.steps_completed = 3;
        rs.total_iterations = 7;

        let report = rs.report();
        assert_eq!(report.steps_completed, 3);
        assert_eq!(report.total_iterations, 7);
    }

    #[test]
    fn run_state_is_serializable() {
        let plan = Plan {
            steps: vec![crate::planner::PlanStep {
                description: "test".into(),
                files: vec!["src/foo.rs".into()],
            }],
        };
        let config = RunConfig::default();
        let state = RunState::new("goal".into(), plan, &config);
        let json = serde_json::to_string(&state);
        assert!(json.is_ok(), "RunState must serialize to JSON");
    }

    #[test]
    fn run_state_round_trips_through_json() {
        let plan = Plan {
            steps: vec![crate::planner::PlanStep {
                description: "do thing".into(),
                files: vec!["src/lib.rs".into()],
            }],
        };
        let config = RunConfig {
            max_iterations_per_step: 7,
            max_total_iterations: 35,
            ..RunConfig::default()
        };
        let original = RunState::new("build a widget".into(), plan, &config);
        let json = serde_json::to_string(&original).unwrap();
        let restored: RunState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.goal, "build a widget");
        assert_eq!(restored.max_iterations_per_step, 7);
        assert_eq!(restored.max_total_iterations, 35);
        assert_eq!(restored.plan.steps.len(), 1);
        assert_eq!(restored.step_index, 0);
        assert_eq!(restored.step_state.attempt, 0);
    }
}
