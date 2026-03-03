use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::json;

use crate::log_schema::{AttemptLog, FinalLog, PlanLog};
use crate::r#loop::RunState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Success,
    Aborted,
    CapReached,
    TokenCap,
    EditError,
    ApplyError,
    PlanError,
}

impl std::fmt::Display for RunOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Aborted => write!(f, "aborted"),
            Self::CapReached => write!(f, "cap_reached"),
            Self::TokenCap => write!(f, "token_cap"),
            Self::EditError => write!(f, "edit_error"),
            Self::ApplyError => write!(f, "apply_error"),
            Self::PlanError => write!(f, "plan_error"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub goal: String,
    pub outcome: RunOutcome,
    pub terminal_message: Option<String>,
    pub steps_completed: usize,
    pub steps_aborted: usize,
    pub total_attempts: usize,
    pub attempts_per_step: Vec<usize>,
    pub failure_stages: Vec<(String, usize)>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub llm_requests_total: u64,
    pub llm_requests_plan: u64,
    pub llm_requests_edit: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiRunSummary {
    pub runs_total: usize,
    pub runs_succeeded: usize,
    pub runs_aborted: usize,
    pub runs_cap_reached: usize,
    pub runs_token_cap: usize,
    pub runs_edit_error: usize,
    pub runs_apply_error: usize,
    pub runs_plan_error: usize,
    pub avg_attempts: f64,
    pub avg_tokens: f64,
    pub most_common_failure_stage: Option<(String, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsError {
    NoData,
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
        message: String,
    },
    InvalidLog {
        path: PathBuf,
        reason: String,
    },
}

impl std::fmt::Display for StatsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoData => write!(f, "no run data available"),
            Self::Io {
                path,
                kind: _kind,
                message,
            } => {
                write!(f, "I/O error for {}: {message}", path.display())
            }
            Self::InvalidLog { path, reason } => {
                write!(f, "invalid log at {}: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for StatsError {}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StatsError> {
    let json = fs::read_to_string(path).map_err(|e| StatsError::Io {
        path: path.to_path_buf(),
        kind: e.kind(),
        message: e.to_string(),
    })?;
    serde_json::from_str(&json).map_err(|e| StatsError::InvalidLog {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

fn parse_attempt_filename(name: &str) -> Option<(usize, usize)> {
    if !name.starts_with("step_") || !name.ends_with(".json") {
        return None;
    }

    let stem = &name[..name.len() - 5];
    let mut parts = stem.split('_');
    let p0 = parts.next()?;
    let p1 = parts.next()?;
    let p2 = parts.next()?;
    let p3 = parts.next()?;

    if parts.next().is_some() || p0 != "step" || p2 != "attempt" {
        return None;
    }

    let step = p1.parse().ok()?;
    let attempt = p3.parse().ok()?;
    Some((step, attempt))
}

fn parse_final_outcome(outcome: &str) -> Option<RunOutcome> {
    match outcome {
        "success" => Some(RunOutcome::Success),
        "aborted" => Some(RunOutcome::Aborted),
        "cap_reached" => Some(RunOutcome::CapReached),
        "token_cap" => Some(RunOutcome::TokenCap),
        "edit_error" => Some(RunOutcome::EditError),
        "apply_error" => Some(RunOutcome::ApplyError),
        "plan_error" => Some(RunOutcome::PlanError),
        _ => None,
    }
}

pub fn summarize_run(run_log_dir: &Path) -> Result<RunSummary, StatsError> {
    if !run_log_dir.exists() {
        return Err(StatsError::NoData);
    }

    let final_path = run_log_dir.join("final.json");
    let final_log = if final_path.exists() {
        Some(read_json::<FinalLog>(&final_path)?)
    } else {
        None
    };
    let plan_path = run_log_dir.join("plan.json");
    if !plan_path.exists() {
        if let Some(log) = final_log {
            if parse_final_outcome(&log.outcome) == Some(RunOutcome::PlanError) {
                return Ok(RunSummary {
                    run_id: log.run_id,
                    goal: "(plan unavailable)".to_string(),
                    outcome: RunOutcome::PlanError,
                    terminal_message: log.message,
                    steps_completed: 0,
                    steps_aborted: 0,
                    total_attempts: 0,
                    attempts_per_step: vec![],
                    failure_stages: vec![],
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    llm_requests_total: 1,
                    llm_requests_plan: 1,
                    llm_requests_edit: 0,
                });
            }
        }
        return Err(StatsError::NoData);
    }
    let plan_log: PlanLog = read_json(&plan_path)?;

    let mut attempt_files = Vec::new();
    let reader = fs::read_dir(run_log_dir).map_err(|e| StatsError::Io {
        path: run_log_dir.to_path_buf(),
        kind: e.kind(),
        message: e.to_string(),
    })?;

    for entry in reader {
        let entry = entry.map_err(|e| StatsError::Io {
            path: run_log_dir.to_path_buf(),
            kind: e.kind(),
            message: e.to_string(),
        })?;

        let file_type = entry.file_type().map_err(|e| StatsError::Io {
            path: entry.path(),
            kind: e.kind(),
            message: e.to_string(),
        })?;
        if !file_type.is_file() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if let Some((step_index, attempt)) = parse_attempt_filename(&name) {
            attempt_files.push((step_index, attempt, entry.path()));
        }
    }

    attempt_files.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));

    let mut attempt_logs = Vec::with_capacity(attempt_files.len());
    for (_, _, path) in attempt_files {
        attempt_logs.push(read_json::<AttemptLog>(&path)?);
    }

    let mut attempts_by_step: BTreeMap<usize, usize> = BTreeMap::new();
    let mut last_decision_per_step: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    let mut completed_steps: HashSet<usize> = HashSet::new();
    let mut failure_counts: HashMap<String, usize> = HashMap::new();

    for log in &attempt_logs {
        *attempts_by_step.entry(log.step_index).or_insert(0) += 1;

        if log.review_decision == "proceed" {
            completed_steps.insert(log.step_index);
        }

        match last_decision_per_step.get(&log.step_index) {
            Some((prev_attempt, _)) if *prev_attempt >= log.attempt => {}
            _ => {
                last_decision_per_step
                    .insert(log.step_index, (log.attempt, log.review_decision.clone()));
            }
        }

        if !log.runner_output.ok {
            *failure_counts
                .entry(log.runner_output.stage.clone())
                .or_insert(0) += 1;
        }
    }

    let steps_completed = completed_steps.len();
    let steps_aborted = last_decision_per_step
        .values()
        .filter(|(_, decision)| decision == "abort")
        .count();
    let total_attempts = attempt_logs.len();

    let plan_steps = plan_log.plan.steps.len();
    let max_step_seen = attempts_by_step.keys().copied().max().unwrap_or(0);
    let attempts_len = if attempts_by_step.is_empty() {
        plan_steps
    } else {
        plan_steps.max(max_step_seen + 1)
    };

    let mut attempts_per_step = vec![0usize; attempts_len];
    for (step_index, count) in attempts_by_step {
        attempts_per_step[step_index] = count;
    }

    let mut failure_stages: Vec<(String, usize)> = failure_counts.into_iter().collect();
    failure_stages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let heuristic_outcome = if steps_completed == plan_steps {
        RunOutcome::Success
    } else if steps_aborted > 0 {
        RunOutcome::Aborted
    } else {
        RunOutcome::CapReached
    };
    let outcome = final_log
        .as_ref()
        .and_then(|log| parse_final_outcome(&log.outcome))
        .unwrap_or(heuristic_outcome);
    let terminal_message = final_log.and_then(|log| log.message);

    let usage_cumulative = attempt_logs
        .last()
        .map(|log| log.usage_cumulative.clone())
        .unwrap_or_default();
    let input_tokens = usage_cumulative.input_tokens;
    let output_tokens = usage_cumulative.output_tokens;
    let total_tokens = usage_cumulative.total();
    let llm_requests_plan: u64 = 1;
    let llm_requests_edit = attempt_logs.len() as u64;
    let llm_requests_total = llm_requests_plan + llm_requests_edit;

    Ok(RunSummary {
        run_id: plan_log.run_id,
        goal: plan_log.goal,
        outcome,
        terminal_message,
        steps_completed,
        steps_aborted,
        total_attempts,
        attempts_per_step,
        failure_stages,
        input_tokens,
        output_tokens,
        total_tokens,
        llm_requests_total,
        llm_requests_plan,
        llm_requests_edit,
    })
}

pub fn summarize_current(project_root: &Path) -> Result<RunSummary, StatsError> {
    let state_path = project_root.join(".tod/state.json");
    if !state_path.exists() {
        return Err(StatsError::NoData);
    }

    let state: RunState = read_json(&state_path)?;
    if state.run_id.trim().is_empty() || state.log_dir.trim().is_empty() {
        return Err(StatsError::InvalidLog {
            path: state_path,
            reason: "missing run_id or log_dir".to_string(),
        });
    }

    summarize_run(&project_root.join(state.log_dir))
}

pub fn summarize_runs(tod_dir: &Path, limit: usize) -> Result<MultiRunSummary, StatsError> {
    let logs_dir = tod_dir.join("logs");
    if !logs_dir.exists() {
        return Err(StatsError::NoData);
    }

    let mut run_dirs: Vec<String> = Vec::new();
    let reader = fs::read_dir(&logs_dir).map_err(|e| StatsError::Io {
        path: logs_dir.clone(),
        kind: e.kind(),
        message: e.to_string(),
    })?;

    for entry in reader {
        let entry = entry.map_err(|e| StatsError::Io {
            path: logs_dir.clone(),
            kind: e.kind(),
            message: e.to_string(),
        })?;
        let file_type = entry.file_type().map_err(|e| StatsError::Io {
            path: entry.path(),
            kind: e.kind(),
            message: e.to_string(),
        })?;
        if file_type.is_dir() {
            run_dirs.push(entry.file_name().to_string_lossy().to_string());
        }
    }

    run_dirs.sort_by(|a, b| b.cmp(a));

    let selected = run_dirs.into_iter().take(limit);

    let mut runs_total = 0usize;
    let mut runs_succeeded = 0usize;
    let mut runs_aborted = 0usize;
    let mut runs_cap_reached = 0usize;
    let mut runs_token_cap = 0usize;
    let mut runs_edit_error = 0usize;
    let mut runs_apply_error = 0usize;
    let mut runs_plan_error = 0usize;
    let mut attempts_total = 0usize;
    let mut tokens_total = 0u64;
    let mut failure_counts: HashMap<String, usize> = HashMap::new();

    for run_id in selected {
        let summary = summarize_run(&logs_dir.join(run_id))?;
        runs_total += 1;
        attempts_total += summary.total_attempts;
        tokens_total += summary.total_tokens;

        match summary.outcome {
            RunOutcome::Success => runs_succeeded += 1,
            RunOutcome::Aborted => runs_aborted += 1,
            RunOutcome::CapReached => runs_cap_reached += 1,
            RunOutcome::TokenCap => runs_token_cap += 1,
            RunOutcome::EditError => runs_edit_error += 1,
            RunOutcome::ApplyError => runs_apply_error += 1,
            RunOutcome::PlanError => runs_plan_error += 1,
        }

        for (stage, count) in summary.failure_stages {
            *failure_counts.entry(stage).or_insert(0) += count;
        }
    }

    let avg_attempts = if runs_total == 0 {
        0.0
    } else {
        attempts_total as f64 / runs_total as f64
    };
    let avg_tokens = if runs_total == 0 {
        0.0
    } else {
        tokens_total as f64 / runs_total as f64
    };

    let most_common_failure_stage = failure_counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)));

    Ok(MultiRunSummary {
        runs_total,
        runs_succeeded,
        runs_aborted,
        runs_cap_reached,
        runs_token_cap,
        runs_edit_error,
        runs_apply_error,
        runs_plan_error,
        avg_attempts,
        avg_tokens,
        most_common_failure_stage,
    })
}

pub fn format_run_summary(summary: &RunSummary) -> String {
    let attempts = summary
        .attempts_per_step
        .iter()
        .enumerate()
        .map(|(step, count)| format!("step {step}: {count}"))
        .collect::<Vec<_>>()
        .join(", ");

    let failures = if summary.failure_stages.is_empty() {
        "none".to_string()
    } else {
        summary
            .failure_stages
            .iter()
            .map(|(stage, count)| format!("{stage} ({count})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let terminal_line = summary
        .terminal_message
        .as_ref()
        .map(|message| format!("\nTerminal:   {message}"))
        .unwrap_or_default();

    let tokens_line = if summary.total_tokens > 0 {
        format!(
            "\nTokens:     {} in / {} out ({} requests: {} plan, {} edit)",
            summary.input_tokens,
            summary.output_tokens,
            summary.llm_requests_total,
            summary.llm_requests_plan,
            summary.llm_requests_edit,
        )
    } else {
        String::new()
    };

    format!(
        "Run:        {}\nGoal:       {}\nOutcome:    {}\nProgress:   {}/{} steps completed, {} aborted\nAttempts:   {} total ({})\nFailures:   {}{}\nLogs:       .tod/logs/{}/",
        summary.run_id,
        summary.goal,
        summary.outcome,
        summary.steps_completed,
        summary.attempts_per_step.len(),
        summary.steps_aborted,
        summary.total_attempts,
        attempts,
        failures,
        terminal_line,
        summary.run_id,
    ) + &tokens_line
}

pub fn format_run_summary_json(summary: &RunSummary) -> String {
    json!({
        "run_id": &summary.run_id,
        "goal": &summary.goal,
        "outcome": summary.outcome.to_string(),
        "terminal_message": &summary.terminal_message,
        "steps_completed": summary.steps_completed,
        "steps_aborted": summary.steps_aborted,
        "total_attempts": summary.total_attempts,
        "attempts_per_step": &summary.attempts_per_step,
        "failure_stages": &summary.failure_stages,
        "input_tokens": summary.input_tokens,
        "output_tokens": summary.output_tokens,
        "total_tokens": summary.total_tokens,
        "llm_requests_total": summary.llm_requests_total,
        "llm_requests_plan": summary.llm_requests_plan,
        "llm_requests_edit": summary.llm_requests_edit
    })
    .to_string()
}

pub fn format_multi_run_summary(summary: &MultiRunSummary) -> String {
    let failure = match &summary.most_common_failure_stage {
        Some((stage, count)) => format!("{stage} ({count} occurrences)"),
        None => "none".to_string(),
    };

    let outcome_line = [
        ("Succeeded", summary.runs_succeeded, true),
        ("Aborted", summary.runs_aborted, false),
        ("Cap reached", summary.runs_cap_reached, false),
        ("Token cap", summary.runs_token_cap, false),
        ("Edit error", summary.runs_edit_error, false),
        ("Apply error", summary.runs_apply_error, false),
        ("Plan error", summary.runs_plan_error, false),
    ]
    .into_iter()
    .filter(|(_, count, force)| *force || *count > 0)
    .map(|(label, count, _)| format!("{label}: {count}"))
    .collect::<Vec<_>>()
    .join("  ");

    format!(
        "Last {} runs:\n  {}\n  Avg attempts: {:.1}\n  Avg tokens: {:.0}\n  Most common failure: {}",
        summary.runs_total,
        outcome_line,
        summary.avg_attempts,
        summary.avg_tokens,
        failure,
    )
}

pub fn format_multi_run_summary_json(summary: &MultiRunSummary) -> String {
    json!({
        "runs_total": summary.runs_total,
        "runs_succeeded": summary.runs_succeeded,
        "runs_aborted": summary.runs_aborted,
        "runs_cap_reached": summary.runs_cap_reached,
        "runs_token_cap": summary.runs_token_cap,
        "runs_edit_error": summary.runs_edit_error,
        "runs_apply_error": summary.runs_apply_error,
        "runs_plan_error": summary.runs_plan_error,
        "avg_attempts": summary.avg_attempts,
        "avg_tokens": summary.avg_tokens,
        "most_common_failure_stage": &summary.most_common_failure_stage
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempSandbox;
    use std::path::Path;

    use serde_json::json;

    use crate::llm::Usage;
    use crate::planner::{Plan, PlanStep};
    use crate::r#loop::{Fingerprint, StepState};

    fn write_plan(run_dir: &Path, run_id: &str, goal: &str, steps: usize) {
        write_plan_with_usage(run_dir, run_id, goal, steps, None);
    }

    fn write_plan_with_usage(
        run_dir: &Path,
        run_id: &str,
        goal: &str,
        steps: usize,
        usage: Option<Usage>,
    ) {
        let plan_steps = (0..steps)
            .map(|i| json!({"description": format!("step {i}"), "files": ["src/main.rs"]}))
            .collect::<Vec<_>>();
        let mut value = json!({
            "run_id": run_id,
            "goal": goal,
            "timestamp_utc": "2026-02-24T14:30:22Z",
            "run_mode": "default",
            "plan": {"steps": plan_steps}
        });
        if let Some(usage) = usage {
            value["usage"] = json!({
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens
            });
        }
        fs::create_dir_all(run_dir).unwrap();
        fs::write(
            run_dir.join("plan.json"),
            serde_json::to_string_pretty(&value).unwrap(),
        )
        .unwrap();
    }

    fn write_attempt(
        run_dir: &Path,
        run_id: &str,
        step_index: usize,
        attempt: usize,
        stage: &str,
        ok: bool,
        review_decision: &str,
    ) {
        write_attempt_with_usage(
            run_dir,
            run_id,
            step_index,
            attempt,
            stage,
            ok,
            review_decision,
            None,
            None,
        );
    }

    fn write_attempt_with_usage(
        run_dir: &Path,
        run_id: &str,
        step_index: usize,
        attempt: usize,
        stage: &str,
        ok: bool,
        review_decision: &str,
        usage_this_call: Option<Usage>,
        usage_cumulative: Option<Usage>,
    ) {
        let mut value = json!({
            "run_id": run_id,
            "step_index": step_index,
            "attempt": attempt,
            "timestamp_utc": "2026-02-24T14:30:25Z",
            "run_mode": "default",
            "edit_batch": {"edits": []},
            "runner_output": {"stage": stage, "ok": ok, "output": "", "truncated": false},
            "review_decision": review_decision
        });
        if let Some(usage) = usage_this_call {
            value["usage_this_call"] = json!({
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens
            });
        }
        if let Some(usage) = usage_cumulative {
            value["usage_cumulative"] = json!({
                "input_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens
            });
        }
        let path = run_dir.join(format!("step_{step_index}_attempt_{attempt}.json"));
        fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    }

    fn write_state(project_root: &Path, run_id: &str, log_dir: &str) {
        let state = RunState {
            goal: "test goal".to_string(),
            plan: Plan {
                steps: vec![PlanStep {
                    description: "step 0".to_string(),
                    files: vec!["src/main.rs".to_string()],
                }],
            },
            step_index: 0,
            step_state: StepState {
                attempt: 0,
                retry_context: None,
            },
            steps_completed: 0,
            total_iterations: 0,
            max_iterations_per_step: 5,
            max_total_iterations: 25,
            run_id: run_id.to_string(),
            log_dir: log_dir.to_string(),
            last_log_path: None,
            fingerprint: Fingerprint {
                fingerprint_version: 2,
                file_count: 0,
                total_bytes: 0,
                hash: "hash".to_string(),
            },
            profile: None,
            usage: Usage::default(),
            llm_requests: 0,
            max_tokens: 0,
        };

        let tod_dir = project_root.join(".tod");
        fs::create_dir_all(&tod_dir).unwrap();
        fs::write(
            tod_dir.join("state.json"),
            serde_json::to_string_pretty(&state).unwrap(),
        )
        .unwrap();
    }

    fn write_final(run_dir: &Path, run_id: &str, outcome: &str, message: Option<&str>) {
        let mut value = json!({
            "run_id": run_id,
            "timestamp_utc": "2026-03-02T00:00:00Z",
            "outcome": outcome
        });
        if let Some(message) = message {
            value["message"] = json!(message);
        }
        fs::write(
            run_dir.join("final.json"),
            serde_json::to_string_pretty(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn summarize_run_success() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_143022";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "test goal", 2);
        write_attempt(&run_dir, run_id, 0, 1, "test", true, "proceed");
        write_attempt(&run_dir, run_id, 1, 1, "test", true, "proceed");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.outcome, RunOutcome::Success);
        assert_eq!(summary.steps_completed, 2);
        assert_eq!(summary.steps_aborted, 0);
    }

    #[test]
    fn summarize_run_aborted() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_143022";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "test goal", 1);
        write_attempt(&run_dir, run_id, 0, 1, "test", false, "abort");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.outcome, RunOutcome::Aborted);
        assert_eq!(summary.steps_aborted, 1);
    }

    #[test]
    fn summarize_run_prefers_final_log_outcome() {
        let sandbox = TempSandbox::new();
        let run_id = "20260302_000000";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "goal", 1);
        write_attempt(&run_dir, run_id, 0, 1, "edit_generation", false, "error");
        write_final(
            &run_dir,
            run_id,
            "edit_error",
            Some("edit parse failed: expected JSON"),
        );

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.outcome, RunOutcome::EditError);
        assert_eq!(
            summary.terminal_message,
            Some("edit parse failed: expected JSON".to_string())
        );
    }

    #[test]
    fn summarize_run_error_decision_not_counted_as_abort() {
        let sandbox = TempSandbox::new();
        let run_id = "20260302_000100";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "goal", 1);
        write_attempt(&run_dir, run_id, 0, 1, "edit_generation", false, "error");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.steps_aborted, 0);
    }

    #[test]
    fn summarize_run_legacy_without_final_log_still_works() {
        let sandbox = TempSandbox::new();
        let run_id = "20260302_000200";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "legacy goal", 1);
        write_attempt(&run_dir, run_id, 0, 1, "test", false, "retry");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.outcome, RunOutcome::CapReached);
        assert!(summary.terminal_message.is_none());
    }

    #[test]
    fn summarize_run_plan_error_without_plan_log() {
        let sandbox = TempSandbox::new();
        let run_id = "20260303_010000";
        let run_dir = sandbox.join(".tod/logs").join(run_id);
        fs::create_dir_all(&run_dir).unwrap();
        write_final(&run_dir, run_id, "plan_error", Some("model refused"));

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.run_id, run_id);
        assert_eq!(summary.goal, "(plan unavailable)");
        assert_eq!(summary.outcome, RunOutcome::PlanError);
        assert_eq!(summary.terminal_message.as_deref(), Some("model refused"));
        assert_eq!(summary.llm_requests_plan, 1);
        assert_eq!(summary.llm_requests_edit, 0);
        assert_eq!(summary.llm_requests_total, 1);
    }

    #[test]
    fn summarize_run_failure_stages() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_143022";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "test goal", 2);
        write_attempt(&run_dir, run_id, 0, 1, "build", false, "retry");
        write_attempt(&run_dir, run_id, 0, 2, "test", false, "retry");
        write_attempt(&run_dir, run_id, 0, 3, "test", false, "abort");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(
            summary.failure_stages,
            vec![("test".to_string(), 2), ("build".to_string(), 1)]
        );
    }

    #[test]
    fn summarize_run_attempts_per_step() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_143022";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "test goal", 2);
        write_attempt(&run_dir, run_id, 0, 1, "test", false, "retry");
        write_attempt(&run_dir, run_id, 0, 2, "test", false, "retry");
        write_attempt(&run_dir, run_id, 0, 3, "test", true, "proceed");
        write_attempt(&run_dir, run_id, 1, 1, "test", true, "proceed");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.attempts_per_step, vec![3, 1]);
    }

    #[test]
    fn summarize_run_counts_plan_request() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_150000";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan_with_usage(
            &run_dir,
            run_id,
            "goal",
            1,
            Some(Usage {
                input_tokens: 10,
                output_tokens: 5,
            }),
        );
        write_attempt_with_usage(
            &run_dir,
            run_id,
            0,
            1,
            "test",
            true,
            "proceed",
            Some(Usage {
                input_tokens: 3,
                output_tokens: 2,
            }),
            Some(Usage {
                input_tokens: 13,
                output_tokens: 7,
            }),
        );

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.llm_requests_plan, 1);
        assert_eq!(summary.llm_requests_edit, 1);
        assert_eq!(summary.llm_requests_total, 2);
    }

    #[test]
    fn summarize_run_legacy_plan_no_usage() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_150500";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "goal", 1);
        write_attempt_with_usage(
            &run_dir,
            run_id,
            0,
            1,
            "test",
            true,
            "proceed",
            Some(Usage {
                input_tokens: 3,
                output_tokens: 2,
            }),
            Some(Usage {
                input_tokens: 3,
                output_tokens: 2,
            }),
        );

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.llm_requests_plan, 1);
        assert_eq!(summary.llm_requests_edit, 1);
        assert_eq!(summary.llm_requests_total, 2);
    }

    #[test]
    fn summarize_run_request_count_independent_of_usage_fields() {
        let sandbox = TempSandbox::new();
        let run_id = "count_test";
        let run_dir = sandbox.join(format!(".tod/logs/{run_id}"));

        write_plan(&run_dir, run_id, "test goal", 1);
        write_attempt(&run_dir, run_id, 0, 1, "test", false, "retry");
        write_attempt(&run_dir, run_id, 0, 2, "test", false, "retry");
        write_final(&run_dir, run_id, "aborted", Some("test abort"));

        let summary = summarize_run(&run_dir).expect("should summarize");
        assert_eq!(summary.llm_requests_plan, 1);
        assert_eq!(summary.llm_requests_edit, 2);
        assert_eq!(summary.llm_requests_total, 3);
    }

    #[test]
    fn summarize_current_reads_state() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_143022";
        let log_dir = format!(".tod/logs/{run_id}");
        let run_dir = sandbox.join(&log_dir);

        write_plan(&run_dir, run_id, "state goal", 1);
        write_attempt(&run_dir, run_id, 0, 1, "test", true, "proceed");
        write_state(&sandbox, run_id, &log_dir);

        let summary = summarize_current(&sandbox).unwrap();
        assert_eq!(summary.run_id, run_id);
        assert_eq!(summary.goal, "state goal");
    }

    #[test]
    fn summarize_current_no_data() {
        let sandbox = TempSandbox::new();
        let err = summarize_current(&sandbox).unwrap_err();
        assert!(matches!(err, StatsError::NoData));
    }

    #[test]
    fn summarize_runs_sorts_chronologically() {
        let sandbox = TempSandbox::new();
        let logs_dir = sandbox.join(".tod/logs");

        let old = "20260224_120000";
        let mid = "20260224_130000";
        let new = "20260224_140000";

        let old_dir = logs_dir.join(old);
        let mid_dir = logs_dir.join(mid);
        let new_dir = logs_dir.join(new);

        write_plan(&old_dir, old, "old", 1);
        write_attempt(&old_dir, old, 0, 1, "test", false, "abort");

        write_plan(&mid_dir, mid, "mid", 1);
        write_attempt(&mid_dir, mid, 0, 1, "test", false, "abort");

        write_plan(&new_dir, new, "new", 1);
        write_attempt(&new_dir, new, 0, 1, "test", true, "proceed");

        let summary = summarize_runs(&sandbox.join(".tod"), 2).unwrap();
        assert_eq!(summary.runs_total, 2);
        assert_eq!(summary.runs_succeeded, 1);
        assert_eq!(summary.runs_aborted, 1);
    }

    #[test]
    fn run_id_sorting_remains_stable_for_stats() {
        let sandbox = TempSandbox::new();
        let logs_dir = sandbox.join(".tod/logs");

        let old = "20260303_120000.100000";
        let same_tick_newer = "20260303_120000.100000_2";
        let newest = "20260303_120000.100001";

        let old_dir = logs_dir.join(old);
        let same_tick_newer_dir = logs_dir.join(same_tick_newer);
        let newest_dir = logs_dir.join(newest);

        write_plan(&old_dir, old, "old", 1);
        write_attempt(&old_dir, old, 0, 1, "test", true, "proceed");

        write_plan(
            &same_tick_newer_dir,
            same_tick_newer,
            "same_tick_newer",
            1,
        );
        write_attempt(
            &same_tick_newer_dir,
            same_tick_newer,
            0,
            1,
            "test",
            false,
            "abort",
        );

        write_plan(&newest_dir, newest, "newest", 1);
        write_attempt(&newest_dir, newest, 0, 1, "test", false, "abort");

        let summary = summarize_runs(&sandbox.join(".tod"), 2).unwrap();
        assert_eq!(summary.runs_total, 2);
        assert_eq!(summary.runs_succeeded, 0);
        assert_eq!(summary.runs_aborted, 2);
    }

    #[test]
    fn summarize_runs_respects_limit() {
        let sandbox = TempSandbox::new();
        let logs_dir = sandbox.join(".tod/logs");

        for idx in 0..5 {
            let run_id = format!("20260224_14000{idx}");
            let run_dir = logs_dir.join(&run_id);
            write_plan(&run_dir, &run_id, "goal", 1);
            write_attempt(&run_dir, &run_id, 0, 1, "test", true, "proceed");
        }

        let summary = summarize_runs(&sandbox.join(".tod"), 2).unwrap();
        assert_eq!(summary.runs_total, 2);
        assert_eq!(summary.runs_succeeded, 2);
    }

    #[test]
    fn summarize_runs_aggregates() {
        let sandbox = TempSandbox::new();
        let logs_dir = sandbox.join(".tod/logs");

        let success_id = "20260224_140000";
        let abort_id = "20260224_141000";
        let cap_id = "20260224_142000";

        let success_dir = logs_dir.join(success_id);
        write_plan(&success_dir, success_id, "success", 1);
        write_attempt(&success_dir, success_id, 0, 1, "test", true, "proceed");

        let abort_dir = logs_dir.join(abort_id);
        write_plan(&abort_dir, abort_id, "abort", 1);
        write_attempt(&abort_dir, abort_id, 0, 1, "build", false, "abort");
        write_attempt(&abort_dir, abort_id, 0, 2, "build", false, "abort");

        let cap_dir = logs_dir.join(cap_id);
        write_plan(&cap_dir, cap_id, "cap", 2);
        write_attempt(&cap_dir, cap_id, 0, 1, "test", false, "retry");

        let summary = summarize_runs(&sandbox.join(".tod"), 3).unwrap();
        assert_eq!(summary.runs_total, 3);
        assert_eq!(summary.runs_succeeded, 1);
        assert_eq!(summary.runs_aborted, 1);
        assert_eq!(summary.runs_cap_reached, 1);
        assert_eq!(summary.runs_token_cap, 0);
        assert_eq!(summary.runs_edit_error, 0);
        assert_eq!(summary.runs_apply_error, 0);
        assert_eq!(summary.runs_plan_error, 0);
        assert!((summary.avg_attempts - (4.0 / 3.0)).abs() < f64::EPSILON);
        assert_eq!(
            summary.most_common_failure_stage,
            Some(("build".to_string(), 2))
        );
    }

    #[test]
    fn summarize_runs_counts_terminal_outcomes() {
        let sandbox = TempSandbox::new();
        let logs_dir = sandbox.join(".tod/logs");
        let runs = [
            ("20260303_000001", "success"),
            ("20260303_000002", "aborted"),
            ("20260303_000003", "cap_reached"),
            ("20260303_000004", "token_cap"),
            ("20260303_000005", "edit_error"),
            ("20260303_000006", "apply_error"),
            ("20260303_000007", "plan_error"),
        ];

        for (run_id, outcome) in runs {
            let run_dir = logs_dir.join(run_id);
            write_plan(&run_dir, run_id, "goal", 1);
            write_final(&run_dir, run_id, outcome, None);
        }

        let summary = summarize_runs(&sandbox.join(".tod"), 10).unwrap();
        assert_eq!(summary.runs_total, 7);
        assert_eq!(summary.runs_succeeded, 1);
        assert_eq!(summary.runs_aborted, 1);
        assert_eq!(summary.runs_cap_reached, 1);
        assert_eq!(summary.runs_token_cap, 1);
        assert_eq!(summary.runs_edit_error, 1);
        assert_eq!(summary.runs_apply_error, 1);
        assert_eq!(summary.runs_plan_error, 1);
    }

    #[test]
    fn summarize_runs_includes_plan_error_without_plan_log() {
        let sandbox = TempSandbox::new();
        let logs_dir = sandbox.join(".tod/logs");
        let run_id = "20260303_020000";
        let run_dir = logs_dir.join(run_id);
        fs::create_dir_all(&run_dir).unwrap();
        write_final(&run_dir, run_id, "plan_error", Some("planner failed"));

        let summary = summarize_runs(&sandbox.join(".tod"), 5).unwrap();
        assert_eq!(summary.runs_total, 1);
        assert_eq!(summary.runs_plan_error, 1);
        assert_eq!(summary.runs_succeeded, 0);
    }

    #[test]
    fn summarize_runs_empty() {
        let sandbox = TempSandbox::new();
        fs::create_dir_all(sandbox.join(".tod/logs")).unwrap();

        let summary = summarize_runs(&sandbox.join(".tod"), 5).unwrap();
        assert_eq!(summary.runs_total, 0);
        assert_eq!(summary.runs_succeeded, 0);
        assert_eq!(summary.runs_aborted, 0);
        assert_eq!(summary.runs_cap_reached, 0);
        assert_eq!(summary.runs_token_cap, 0);
        assert_eq!(summary.runs_edit_error, 0);
        assert_eq!(summary.runs_apply_error, 0);
        assert_eq!(summary.runs_plan_error, 0);
        assert_eq!(summary.avg_attempts, 0.0);
        assert_eq!(summary.avg_tokens, 0.0);
        assert_eq!(summary.most_common_failure_stage, None);
    }

    #[test]
    fn format_run_summary_shows_tokens() {
        let summary = RunSummary {
            run_id: "r1".to_string(),
            goal: "g".to_string(),
            outcome: RunOutcome::Success,
            terminal_message: None,
            steps_completed: 1,
            steps_aborted: 0,
            total_attempts: 1,
            attempts_per_step: vec![1],
            failure_stages: vec![],
            input_tokens: 1234,
            output_tokens: 567,
            total_tokens: 1801,
            llm_requests_total: 3,
            llm_requests_plan: 1,
            llm_requests_edit: 2,
        };
        let rendered = format_run_summary(&summary);
        assert!(rendered.contains("Tokens:"));
        assert!(rendered.contains("3 requests: 1 plan, 2 edit"));
    }

    #[test]
    fn format_run_summary_hides_zero_tokens() {
        let summary = RunSummary {
            run_id: "r1".to_string(),
            goal: "g".to_string(),
            outcome: RunOutcome::Success,
            terminal_message: None,
            steps_completed: 1,
            steps_aborted: 0,
            total_attempts: 1,
            attempts_per_step: vec![1],
            failure_stages: vec![],
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            llm_requests_total: 0,
            llm_requests_plan: 0,
            llm_requests_edit: 0,
        };
        let rendered = format_run_summary(&summary);
        assert!(!rendered.contains("Tokens:"));
    }

    #[test]
    fn format_run_summary_does_not_show_legacy_annotation() {
        let summary = RunSummary {
            run_id: "r1".to_string(),
            goal: "g".to_string(),
            outcome: RunOutcome::Success,
            terminal_message: None,
            steps_completed: 1,
            steps_aborted: 0,
            total_attempts: 1,
            attempts_per_step: vec![1],
            failure_stages: vec![],
            input_tokens: 700,
            output_tokens: 300,
            total_tokens: 1000,
            llm_requests_total: 1,
            llm_requests_plan: 0,
            llm_requests_edit: 1,
        };
        let rendered = format_run_summary(&summary);
        assert!(!rendered.contains("legacy"));
    }

    #[test]
    fn format_run_summary_json_round_trips() {
        let summary = RunSummary {
            run_id: "r1".to_string(),
            goal: "goal".to_string(),
            outcome: RunOutcome::CapReached,
            terminal_message: Some("hit cap".to_string()),
            steps_completed: 1,
            steps_aborted: 0,
            total_attempts: 3,
            attempts_per_step: vec![3],
            failure_stages: vec![("test".to_string(), 2)],
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            llm_requests_total: 4,
            llm_requests_plan: 1,
            llm_requests_edit: 3,
        };

        let rendered = format_run_summary_json(&summary);
        let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed["run_id"], "r1");
        assert_eq!(parsed["outcome"], "cap_reached");
        assert_eq!(parsed["total_tokens"], 15);
    }

    #[test]
    fn format_multi_run_summary_json_has_expected_keys() {
        let summary = MultiRunSummary {
            runs_total: 5,
            runs_succeeded: 2,
            runs_aborted: 1,
            runs_cap_reached: 1,
            runs_token_cap: 0,
            runs_edit_error: 1,
            runs_apply_error: 0,
            runs_plan_error: 0,
            avg_attempts: 2.4,
            avg_tokens: 1200.0,
            most_common_failure_stage: Some(("test".to_string(), 3)),
        };

        let rendered = format_multi_run_summary_json(&summary);
        let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed["runs_total"], 5);
        assert_eq!(parsed["runs_edit_error"], 1);
        assert!(parsed.get("most_common_failure_stage").is_some());
    }
}
