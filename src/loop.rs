use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::RunConfig;
use crate::context::{self, ContextError, MAX_TREE_DEPTH};
use crate::editor::{create_edits, EditError};
use crate::llm::{LlmProvider, Usage};
use crate::planner::{create_plan, Plan, PlanError};
use crate::reviewer::{review, ReviewDecision};
use crate::runner::{apply_edits, run_pipeline, ApplyError, RunResult};
use crate::schema::EditBatch;
use sha2::{Digest, Sha256};

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
        let Ok(reader) = fs::read_dir(dir) else {
            return;
        };
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
                let Ok(rel) = path.strip_prefix(root) else {
                    continue;
                };
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
    #[serde(default)]
    pub usage_this_call: Option<Usage>,
    #[serde(default)]
    pub usage_cumulative: Usage,
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
pub(crate) struct PlanLog {
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
    /// Accumulated token usage across all LLM calls in this run.
    #[serde(default)]
    pub usage: Usage,
    /// Total token requests made in this run.
    #[serde(default)]
    pub llm_requests: u64,
    /// Optional token cap (input + output combined). 0 = no cap.
    #[serde(default)]
    pub max_tokens: u64,
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
            usage: Usage::default(),
            llm_requests: 0,
            max_tokens: config.max_tokens,
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
                let tmp_path = tod_dir.join("state.json.tmp");
                let final_path = tod_dir.join("state.json");
                if let Err(e) = fs::write(&tmp_path, json) {
                    eprintln!("warning: failed to write checkpoint: {e}");
                    return;
                }
                if let Err(e) = fs::rename(&tmp_path, &final_path) {
                    eprintln!("warning: failed to finalize checkpoint: {e}");
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
        usage_this_call: Option<Usage>,
    ) {
        let dir = config.project_root.join(&self.log_dir);
        if fs::create_dir_all(&dir).is_err() {
            return;
        }

        let (stage, ok, output, truncated) = match run_result {
            RunResult::Success => ("success".to_string(), true, String::new(), false),
            RunResult::Failure {
                stage,
                output,
                truncated,
            } => (stage.clone(), false, output.clone(), *truncated),
        };

        let log = AttemptLog {
            run_id: self.run_id.clone(),
            step_index: self.step_index,
            attempt: self.step_state.attempt,
            timestamp_utc: chrono::Utc::now().to_rfc3339(),
            run_mode: format!("{:?}", config.mode).to_lowercase(),
            edit_batch: batch.clone(),
            runner_output: RunnerLog {
                stage,
                ok,
                output,
                truncated,
            },
            review_decision: decision.to_string(),
            usage_this_call,
            usage_cumulative: self.usage.clone(),
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
    TokenCapExceeded {
        used: u64,
        cap: u64,
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
            Self::TokenCapExceeded { used, cap } => {
                write!(f, "token budget exceeded: used {used} tokens, cap was {cap}")
            }
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

impl From<ContextError> for LoopError {
    fn from(value: ContextError) -> Self {
        match value {
            ContextError::Io { path, cause } => Self::Io { path, cause },
            ContextError::InvalidPath {
                step_index,
                path,
                reason,
            } => Self::InvalidPlanPath {
                step_index,
                path,
                reason,
            },
        }
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
    let project_context = context::build_planner_context(&config.project_root)?;
    let (plan, plan_usage) = create_plan(provider, goal, &project_context)?;

    let mut state = RunState::new(goal.to_string(), plan, config);
    if let Some(usage) = &plan_usage {
        state.usage.accumulate(usage);
        state.llm_requests += 1;
    }
    if state.max_tokens > 0 && state.usage.total() > state.max_tokens {
        state.checkpoint(config);
        return Err(LoopError::TokenCapExceeded {
            used: state.usage.total(),
            cap: state.max_tokens,
        });
    }

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
            let mut file_context =
                context::build_step_context(&config.project_root, &step.files, state.step_index)?;
            if let Some(ctx) = &state.step_state.retry_context {
                file_context.push('\n');
                file_context.push_str(&context::build_retry_context(ctx));
            }

            // --- Generate edits ---
            let (batch, call_usage) =
                create_edits(provider, &step, &file_context, &config.project_root).map_err(
                    |source| LoopError::Edit {
                        step_index: state.step_index,
                        iteration: state.step_state.attempt,
                        source,
                    },
                )?;
            if let Some(usage) = &call_usage {
                state.usage.accumulate(usage);
                state.llm_requests += 1;
            }
            if state.max_tokens > 0 && state.usage.total() > state.max_tokens {
                state.checkpoint(config);
                return Err(LoopError::TokenCapExceeded {
                    used: state.usage.total(),
                    cap: state.max_tokens,
                });
            }

            // --- Apply + run (or skip in dry-run) ---
            let run_result = if config.dry_run {
                RunResult::Success
            } else {
                apply_edits(&batch, &config.project_root).map_err(|source| LoopError::Apply {
                    step_index: state.step_index,
                    iteration: state.step_state.attempt,
                    source,
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
                    state.write_attempt_log(
                        config,
                        &batch,
                        &run_result,
                        "proceed",
                        call_usage.clone(),
                    );
                    state.checkpoint(config);
                    break;
                }
                ReviewDecision::Retry { error_context } => {
                    state.write_attempt_log(
                        config,
                        &batch,
                        &run_result,
                        "retry",
                        call_usage.clone(),
                    );
                    state.step_state.retry_context = Some(error_context);
                    state.checkpoint(config);
                }
                ReviewDecision::Abort { reason } => {
                    state.write_attempt_log(
                        config,
                        &batch,
                        &run_result,
                        "abort",
                        call_usage.clone(),
                    );
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
    let mut state: RunState = serde_json::from_str(&json).map_err(|e| LoopError::Io {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempSandbox;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use crate::config::{RunConfig, RunMode};
    use crate::llm::{LlmError, LlmProvider, LlmResponse, Usage};
    use crate::planner::PlanStep;

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
        fn complete(&self, _system: &str, _user: &str) -> Result<LlmResponse, LlmError> {
            let mut lock = self.responses.lock().unwrap();
            let text = lock
                .pop_front()
                .ok_or_else(|| LlmError::RequestFailed("no fake response queued".to_string()))?;
            Ok(LlmResponse { text, usage: None })
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
            max_tokens: 0,
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
            max_tokens: 0,
        };

        let err = run(&provider, "goal", &config).unwrap_err();
        assert!(matches!(err, LoopError::TotalIterationCap { .. }));
    }

    struct UsageProvider {
        responses: Mutex<VecDeque<(String, Option<Usage>)>>,
    }

    impl UsageProvider {
        fn from(responses: Vec<(String, Option<Usage>)>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
        }
    }

    impl LlmProvider for UsageProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<LlmResponse, LlmError> {
            let mut lock = self.responses.lock().unwrap();
            let (text, usage) = lock
                .pop_front()
                .ok_or_else(|| LlmError::RequestFailed("no fake response queued".to_string()))?;
            Ok(LlmResponse { text, usage })
        }
    }

    #[test]
    fn token_cap_aborts_run() {
        let sandbox = TempSandbox::with_main_rs();
        let provider = UsageProvider::from(vec![(
            r#"{"steps":[{"description":"s","files":["src/main.rs"]}]}"#.to_string(),
            Some(Usage {
                input_tokens: 1,
                output_tokens: 1,
            }),
        )]);
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            mode: RunMode::Default,
            max_iterations_per_step: 2,
            max_total_iterations: 3,
            dry_run: true,
            max_runner_output_bytes: 4096,
            max_tokens: 1,
        };

        let err = run(&provider, "goal", &config).unwrap_err();
        assert!(matches!(
            err,
            LoopError::TokenCapExceeded { used: 2, cap: 1 }
        ));
    }

    #[test]
    fn usage_survives_checkpoint() {
        let sandbox = TempSandbox::with_main_rs();
        let provider = UsageProvider::from(vec![
            (
                r#"{"steps":[{"description":"s","files":["src/main.rs"]}]}"#.to_string(),
                Some(Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                }),
            ),
            (
                r#"{"edits":[{"action":"replace_range","path":"src/main.rs","start_line":1,"end_line":1,"content":"fn main() {}"}]}"#.to_string(),
                Some(Usage {
                    input_tokens: 3,
                    output_tokens: 2,
                }),
            ),
        ]);
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            mode: RunMode::Default,
            max_iterations_per_step: 2,
            max_total_iterations: 3,
            dry_run: true,
            max_runner_output_bytes: 4096,
            max_tokens: 0,
        };

        let _ = run(&provider, "goal", &config).unwrap();
        let state_json = fs::read_to_string(sandbox.join(".tod/state.json")).unwrap();
        let state: RunState = serde_json::from_str(&state_json).unwrap();
        assert!(state.usage.input_tokens > 0);
        assert!(state.usage.output_tokens > 0);
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
    // =======================================================================
    // Phase 6 tests — checkpoint, logging, fingerprint, resume
    // =======================================================================

    // -- Checkpoint -------------------------------------------------------

    #[test]
    fn checkpoint_writes_state_json() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![PlanStep {
                description: "test step".into(),
                files: vec!["src/main.rs".into()],
            }],
        };
        let state = RunState::new("test goal".into(), plan, &config);
        state.checkpoint(&config);

        let state_path = sandbox.join(".tod/state.json");
        assert!(
            state_path.exists(),
            ".tod/state.json must exist after checkpoint"
        );

        let json = fs::read_to_string(&state_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["goal"], "test goal");
        assert_eq!(parsed["step_index"], 0);
        assert_eq!(parsed["steps_completed"], 0);
    }

    #[test]
    fn checkpoint_overwrites_existing() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![PlanStep {
                description: "step".into(),
                files: vec!["src/main.rs".into()],
            }],
        };
        let mut state = RunState::new("goal".into(), plan, &config);
        state.checkpoint(&config);

        // Mutate and checkpoint again
        state.steps_completed = 1;
        state.step_index = 1;
        state.checkpoint(&config);

        let json = fs::read_to_string(sandbox.join(".tod/state.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["steps_completed"], 1);
        assert_eq!(parsed["step_index"], 1);
    }

    #[test]
    fn checkpoint_is_atomic() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![PlanStep {
                description: "step".into(),
                files: vec!["src/main.rs".into()],
            }],
        };
        let state = RunState::new("goal".into(), plan, &config);
        state.checkpoint(&config);

        let final_path = sandbox.join(".tod/state.json");
        let tmp_path = sandbox.join(".tod/state.json.tmp");
        assert!(
            final_path.exists(),
            "state.json must exist after checkpoint"
        );
        assert!(
            !tmp_path.exists(),
            "state.json.tmp must not remain after checkpoint"
        );

        let json = fs::read_to_string(final_path).unwrap();
        let parsed: RunState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.goal, "goal");
    }

    // -- Plan log ---------------------------------------------------------

    #[test]
    fn plan_log_written() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![PlanStep {
                description: "do thing".into(),
                files: vec!["src/main.rs".into()],
            }],
        };
        let state = RunState::new("write tests".into(), plan, &config);
        state.write_plan_log(&config);

        let plan_path = sandbox.join(&state.log_dir).join("plan.json");
        assert!(
            plan_path.exists(),
            "plan.json must exist after write_plan_log"
        );

        let json = fs::read_to_string(&plan_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["goal"], "write tests");
        assert_eq!(parsed["run_id"], state.run_id);
        assert!(parsed["plan"]["steps"].is_array());
    }

    // -- Attempt log ------------------------------------------------------

    #[test]
    fn attempt_log_written() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![PlanStep {
                description: "step".into(),
                files: vec!["src/main.rs".into()],
            }],
        };
        let mut state = RunState::new("goal".into(), plan, &config);
        state.step_state.attempt = 1;

        let batch = EditBatch { edits: vec![] };
        let result = RunResult::Success;
        state.write_attempt_log(&config, &batch, &result, "proceed", None);

        let log_path = sandbox.join(&state.log_dir).join("step_0_attempt_1.json");
        assert!(log_path.exists(), "attempt log file must exist");

        let json = fs::read_to_string(&log_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["step_index"], 0);
        assert_eq!(parsed["attempt"], 1);
        assert_eq!(parsed["review_decision"], "proceed");
        assert_eq!(parsed["runner_output"]["ok"], true);
    }

    // -- Fingerprint ------------------------------------------------------

    #[test]
    fn fingerprint_deterministic() {
        let sandbox = TempSandbox::with_main_rs();
        let fp1 = compute_fingerprint(&sandbox);
        let fp2 = compute_fingerprint(&sandbox);
        assert_eq!(fp1.hash, fp2.hash);
        assert_eq!(fp1.file_count, fp2.file_count);
        assert_eq!(fp1.total_bytes, fp2.total_bytes);
    }

    #[test]
    fn fingerprint_detects_new_file() {
        let sandbox = TempSandbox::with_main_rs();
        let before = compute_fingerprint(&sandbox);

        fs::write(sandbox.join("new_file.txt"), "hello").unwrap();
        let after = compute_fingerprint(&sandbox);

        assert_ne!(before.hash, after.hash);
        assert_eq!(after.file_count, before.file_count + 1);
    }

    #[test]
    fn fingerprint_detects_size_change() {
        let sandbox = TempSandbox::with_main_rs();
        let before = compute_fingerprint(&sandbox);

        // Change file content (different size)
        fs::write(
            sandbox.join("src/main.rs"),
            "fn main() { println!(\"changed\"); }\n",
        )
        .unwrap();
        let after = compute_fingerprint(&sandbox);

        assert_ne!(before.hash, after.hash);
        assert_eq!(before.file_count, after.file_count);
    }

    #[test]
    fn fingerprint_excludes_tod_dir() {
        let sandbox = TempSandbox::with_main_rs();
        let before = compute_fingerprint(&sandbox);

        // Add files inside .tod/ — should not affect fingerprint
        fs::create_dir_all(sandbox.join(".tod/logs")).unwrap();
        fs::write(sandbox.join(".tod/state.json"), "{}").unwrap();
        fs::write(sandbox.join(".tod/logs/plan.json"), "{}").unwrap();
        let after = compute_fingerprint(&sandbox);

        assert_eq!(before.hash, after.hash);
        assert_eq!(before.file_count, after.file_count);
    }

    // -- RunState round-trip ----------------------------------------------

    #[test]
    fn run_state_round_trips_with_new_fields() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![PlanStep {
                description: "step".into(),
                files: vec!["src/main.rs".into()],
            }],
        };
        let original = RunState::new("round trip goal".into(), plan, &config);

        let json = serde_json::to_string_pretty(&original).unwrap();
        let restored: RunState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.run_id, original.run_id);
        assert_eq!(restored.log_dir, original.log_dir);
        assert_eq!(restored.last_log_path, original.last_log_path);
        assert_eq!(restored.fingerprint.hash, original.fingerprint.hash);
        assert_eq!(
            restored.fingerprint.file_count,
            original.fingerprint.file_count
        );
        assert_eq!(
            restored.fingerprint.total_bytes,
            original.fingerprint.total_bytes
        );
        assert_eq!(restored.goal, original.goal);
        assert_eq!(restored.step_index, original.step_index);
    }

    // -- Resume -----------------------------------------------------------

    #[test]
    fn resume_loads_state_and_continues() {
        let sandbox = TempSandbox::with_main_rs();

        // Provider: first call returns a plan, second returns edits for step 0,
        // third returns edits for step 1 (resume will use this one)
        let provider = QueueProvider::from(vec![
            // Plan (used by initial run)
            r#"{"steps":[{"description":"step 0","files":["src/main.rs"]},{"description":"step 1","files":["src/main.rs"]}]}"#,
            // Edits for step 0
            r#"{"edits":[{"action":"write_file","path":"src/main.rs","content":"fn main() { println!(\"step0\"); }\n"}]}"#,
            // Edits for step 1 (will be used by resume)
            r#"{"edits":[{"action":"write_file","path":"src/main.rs","content":"fn main() { println!(\"step1\"); }\n"}]}"#,
        ]);

        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            mode: RunMode::Default,
            max_iterations_per_step: 3,
            max_total_iterations: 10,
            dry_run: true,
            max_runner_output_bytes: 4096,
            max_tokens: 0,
        };

        // Run the initial loop — dry_run so it completes both steps
        let report = run(&provider, "two steps", &config).unwrap();
        assert_eq!(report.steps_completed, 2);

        // state.json should exist
        let state_path = sandbox.join(".tod/state.json");
        assert!(state_path.exists());
    }

    #[test]
    fn resume_no_checkpoint_fails() {
        let sandbox = TempSandbox::new();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let provider = QueueProvider::from(vec![]);
        let err = resume(&provider, &config, false).unwrap_err();
        assert!(matches!(err, LoopError::NoCheckpoint));
    }

    #[test]
    fn resume_fingerprint_mismatch_without_force() {
        let sandbox = TempSandbox::with_main_rs();
        let config = RunConfig {
            project_root: sandbox.to_path_buf(),
            ..RunConfig::default()
        };
        let plan = Plan {
            steps: vec![
                PlanStep {
                    description: "s0".into(),
                    files: vec!["src/main.rs".into()],
                },
                PlanStep {
                    description: "s1".into(),
                    files: vec!["src/main.rs".into()],
                },
            ],
        };

        // Write a checkpoint as if a run was in progress at step 1
        let state = RunState::new("goal".into(), plan, &config);
        state.checkpoint(&config);

        // Mutate the workspace to change the fingerprint
        fs::write(sandbox.join("new_file.txt"), "drift").unwrap();

        let provider = QueueProvider::from(vec![]);
        let err = resume(&provider, &config, false).unwrap_err();
        assert!(matches!(err, LoopError::FingerprintMismatch { .. }));
    }
}
