use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::RunConfig;
use crate::editor::{create_edits, format_file_context, EditError};
use crate::llm::LlmProvider;
use crate::planner::{create_plan, Plan, PlanError};
use crate::reviewer::{review, ReviewDecision};
use crate::runner::{apply_edits, run_pipeline, ApplyError, RunResult};
use crate::schema::{validate_path, EditBatch};
use sha2::{Sha256, Digest};

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

// ---------------------------------------------------------------------------
// Workspace fingerprint
// ---------------------------------------------------------------------------

/// Cheap workspace drift detection.
///
/// Hashes sorted `(relative_path, file_size)` pairs for all tracked files,
/// excluding `target/`, `.git/`, and `.tod/`. No mtime (filesystem-dependent),
/// no content hashing (too slow for v1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub file_count: usize,
    pub total_bytes: u64,
    pub hash: String,
}

fn compute_fingerprint(project_root: &Path) -> Fingerprint {
    let mut entries: Vec<(String, u64)> = Vec::new();

    // Reuse the same walk logic as collect_paths, but gather sizes.
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, u64)>, depth: usize) {
        if depth > MAX_TREE_DEPTH {
            return;
        }
        let Ok(reader) = fs::read_dir(dir) else { return };
        for entry in reader.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                let name = entry.file_name();
                if name == ".git" || name == "target" || name == ".tod" {
                    continue;
                }
                walk(root, &path, out, depth + 1);
            } else if ft.is_file() {
                let Ok(rel) = path.strip_prefix(root) else { continue };
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((rel.to_string_lossy().to_string(), size));
            }
        }
    }

    walk(project_root, project_root, &mut entries, 0);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let file_count = entries.len();
    let total_bytes: u64 = entries.iter().map(|(_, s)| s).sum();

    let mut hasher = Sha256::new();
    for (path, size) in &entries {
        hasher.update(format!("{path}:{size}\n").as_bytes());
    }
    let hash = format!("{:x}", hasher.finalize());

    Fingerprint {
        file_count,
        total_bytes,
        hash,
    }
}

// ---------------------------------------------------------------------------
// Log records
// ---------------------------------------------------------------------------

/// Structured log for a single edit→apply→run→review cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptLog {
    pub run_id: String,
    pub step_index: usize,
    pub attempt: usize,
    pub timestamp_utc: String,
    pub run_mode: String,
    pub edit_batch: EditBatch,
    pub runner_output: RunnerLog,
    pub review_decision: String,
}

/// Structured snapshot of runner output for logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerLog {
    pub stage: String,
    pub ok: bool,
    pub output: String,
    pub truncated: bool,
}

/// Plan log written once after planning completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlanLog {
    pub run_id: String,
    pub goal: String,
    pub timestamp_utc: String,
    pub run_mode: String,
    pub plan: Plan,
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
    /// Unique identifier for this run (YYYYMMDD_HHMMSS UTC).
    pub run_id: String,
    /// Relative path to the log directory for this run.
    pub log_dir: String,
    /// Relative path to the last written attempt log file.
    pub last_log_path: Option<String>,
    /// Workspace fingerprint at last checkpoint.
    pub fingerprint: Fingerprint,
}

impl RunState {
    fn new(goal: String, plan: Plan, config: &RunConfig) -> Self {
        let now = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let log_dir = format!(".tod/logs/{now}");
        let fingerprint = compute_fingerprint(&config.project_root);
        Self {
            goal,
            plan,
            step_index: 0,
            step_state: StepState::new(),
            steps_completed: 0,
            total_iterations: 0,
            max_iterations_per_step: config.max_iterations_per_step,
            max_total_iterations: config.max_total_iterations,
            run_id: now,
            log_dir,
            last_log_path: None,
            fingerprint,
        }
    }

    /// Produce a `LoopReport` from the current state.
    fn report(&self) -> LoopReport {
        LoopReport {
            steps_completed: self.steps_completed,
            total_iterations: self.total_iterations,
        }
    }

    /// Write state to `.tod/state.json`.
    ///
    /// Best-effort — checkpoint failure never aborts a run.
    fn checkpoint(&self, config: &RunConfig) {
        let tod_dir = config.project_root.join(".tod");
        if fs::create_dir_all(&tod_dir).is_err() {
            eprintln!("warning: could not create .tod directory");
            return;
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(tod_dir.join("state.json"), json) {
                    eprintln!("warning: failed to write checkpoint: {e}");
                }
            }
            Err(e) => eprintln!("warning: failed to serialize checkpoint: {e}"),
        }
    }

    /// Write plan.json to the run's log directory. Best-effort.
    fn write_plan_log(&self, config: &RunConfig) {
        let dir = config.project_root.join(&self.log_dir);
        if fs::create_dir_all(&dir).is_err() {
            return;
        }
        let log = PlanLog {
            run_id: self.run_id.clone(),
            goal: self.goal.clone(),
            timestamp_utc: chrono::Utc::now().to_rfc3339(),
            run_mode: format!("{:?}", config.mode).to_lowercase(),
            plan: self.plan.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&log) {
            let _ = fs::write(dir.join("plan.json"), json);
        }
    }

    /// Write a per-attempt log file. Best-effort. Updates `last_log_path`.
    fn write_attempt_log(
        &mut self,
        config: &RunConfig,
        batch: &EditBatch,
        run_result: &RunResult,
        decision: &str,
    ) {
        let dir = config.project_root.join(&self.log_dir);
        if fs::create_dir_all(&dir).is_err() {
            return;
        }

        let (stage, ok, output) = match run_result {
            RunResult::Success => ("success".to_string(), true, String::new()),
            RunResult::Failure { stage, output } => {
                (stage.clone(), false, output.clone())
            }
        };

        let truncated = output.contains("[truncated");

        let log = AttemptLog {
            run_id: self.run_id.clone(),
            step_index: self.step_index,
            attempt: self.step_state.attempt,
            timestamp_utc: chrono::Utc::now().to_rfc3339(),
            run_mode: format!("{:?}", config.mode).to_lowercase(),
            edit_batch: batch.clone(),
            runner_output: RunnerLog { stage, ok, output, truncated },
            review_decision: decision.to_string(),
        };

        let filename = format!(
            "step_{}_attempt_{}.json",
            self.step_index, self.step_state.attempt
        );
        let path = format!("{}/{filename}", self.log_dir);

        if let Ok(json) = serde_json::to_string_pretty(&log) {
            let _ = fs::write(dir.join(&filename), json);
        }

        self.last_log_path = Some(path);
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
    NoCheckpoint,
    FingerprintMismatch {
        expected_hash: String,
        actual_hash: String,
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
            Self::NoCheckpoint => write!(f, "no .tod/state.json found — nothing to resume"),
            Self::FingerprintMismatch {
                expected_hash,
                actual_hash,
            } => write!(
                f,
                "workspace has changed since last checkpoint (expected {expected_hash:.8}, got {actual_hash:.8}) — use --force to override"
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

    // Write plan log and checkpoint: plan created, about to start step 0.
    state.write_plan_log(config);
    state.checkpoint(config);

    run_from_state(provider, config, &mut state)
}

/// Shared step loop used by both `run()` and `resume()`.
fn run_from_state(
    provider: &dyn LlmProvider,
    config: &RunConfig,
    state: &mut RunState,
) -> Result<LoopReport, LoopError> {
    while state.step_index < state.plan.steps.len() {
        let step = state.plan.steps[state.step_index].clone();
        let mut step_succeeded = false;

        while state.step_state.attempt < state.max_iterations_per_step {
            // --- Global cap guard ---
            if state.total_iterations >= state.max_total_iterations {
                state.checkpoint(config);
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
                    state.write_attempt_log(config, &batch, &run_result, "proceed");
                    state.checkpoint(config);
                    break;
                }
                ReviewDecision::Retry { error_context } => {
                    state.write_attempt_log(config, &batch, &run_result, "retry");
                    state.step_state.retry_context = Some(error_context);
                    state.checkpoint(config);
                }
                ReviewDecision::Abort { reason } => {
                    state.write_attempt_log(config, &batch, &run_result, "abort");
                    state.checkpoint(config);
                    return Err(LoopError::Aborted {
                        step_index: state.step_index,
                        reason,
                    });
                }
            }
        }

        if !step_succeeded {
            state.checkpoint(config);
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

/// Resume a previously interrupted run from `.tod/state.json`.
///
/// Loads the checkpoint, verifies the workspace fingerprint, and continues
/// the step loop from where it left off. The plan is not regenerated.
pub fn resume(
    provider: &dyn LlmProvider,
    config: &RunConfig,
    force: bool,
) -> Result<LoopReport, LoopError> {
    let state_path = config.project_root.join(".tod/state.json");
    let json = fs::read_to_string(&state_path).map_err(|_| LoopError::NoCheckpoint)?;
    let mut state: RunState =
        serde_json::from_str(&json).map_err(|e| LoopError::Io {
            path: state_path.display().to_string(),
            cause: format!("failed to parse state.json: {e}"),
        })?;

    // Fingerprint check
    let current = compute_fingerprint(&config.project_root);
    if current.hash != state.fingerprint.hash && !force {
        return Err(LoopError::FingerprintMismatch {
            expected_hash: state.fingerprint.hash.clone(),
            actual_hash: current.hash,
        });
    }
    state.fingerprint = current;

    // Continue the step loop from where we left off
    run_from_state(provider, config, &mut state)
}

/// Display status of the last run. Returns a formatted string.
pub fn status(project_root: &std::path::Path) -> Result<String, LoopError> {
    let state_path = project_root.join(".tod/state.json");
    let json = fs::read_to_string(&state_path).map_err(|_| LoopError::NoCheckpoint)?;
    let state: RunState =
        serde_json::from_str(&json).map_err(|e| LoopError::Io {
            path: state_path.display().to_string(),
            cause: format!("failed to parse state.json: {e}"),
        })?;

    let total_steps = state.plan.steps.len();
    Ok(format!(
        "Run:       {}\n\
         Goal:      {}\n\
         Progress:  step {}/{} (attempt {})\n\
         Completed: {} step(s), {} total iteration(s)\n\
         Logs:      {}",
        state.run_id,
        state.goal,
        state.step_index + 1,
        total_steps,
        state.step_state.attempt,
        state.steps_completed,
        state.total_iterations,
        state.log_dir,
    ))
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
            if name == ".git" || name == "target" || name == ".tod" {
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
