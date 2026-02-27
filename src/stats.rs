use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::r#loop::{AttemptLog, PlanLog, RunState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Success,
    Aborted,
    CapReached,
}

impl std::fmt::Display for RunOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Aborted => write!(f, "aborted"),
            Self::CapReached => write!(f, "cap_reached"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub goal: String,
    pub outcome: RunOutcome,
    pub steps_completed: usize,
    pub steps_aborted: usize,
    pub total_attempts: usize,
    pub retries_per_step: Vec<usize>,
    pub failure_stages: Vec<(String, usize)>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub llm_requests: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiRunSummary {
    pub runs_total: usize,
    pub runs_succeeded: usize,
    pub runs_aborted: usize,
    pub runs_cap_reached: usize,
    pub avg_attempts: f64,
    pub avg_tokens: f64,
    pub most_common_failure_stage: Option<(String, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsError {
    NoData,
    Io { path: String, cause: String },
    InvalidLog { path: String, reason: String },
}

impl std::fmt::Display for StatsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoData => write!(f, "no run data available"),
            Self::Io { path, cause } => write!(f, "I/O error for {path}: {cause}"),
            Self::InvalidLog { path, reason } => {
                write!(f, "invalid log at {path}: {reason}")
            }
        }
    }
}

impl std::error::Error for StatsError {}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StatsError> {
    let json = fs::read_to_string(path).map_err(|e| StatsError::Io {
        path: path.display().to_string(),
        cause: e.to_string(),
    })?;
    serde_json::from_str(&json).map_err(|e| StatsError::InvalidLog {
        path: path.display().to_string(),
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

pub fn summarize_run(run_log_dir: &Path) -> Result<RunSummary, StatsError> {
    if !run_log_dir.exists() {
        return Err(StatsError::NoData);
    }

    let plan_path = run_log_dir.join("plan.json");
    if !plan_path.exists() {
        return Err(StatsError::NoData);
    }
    let plan_log: PlanLog = read_json(&plan_path)?;

    let mut attempt_files = Vec::new();
    let reader = fs::read_dir(run_log_dir).map_err(|e| StatsError::Io {
        path: run_log_dir.display().to_string(),
        cause: e.to_string(),
    })?;

    for entry in reader {
        let entry = entry.map_err(|e| StatsError::Io {
            path: run_log_dir.display().to_string(),
            cause: e.to_string(),
        })?;

        let file_type = entry.file_type().map_err(|e| StatsError::Io {
            path: entry.path().display().to_string(),
            cause: e.to_string(),
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

    let mut attempts_per_step: BTreeMap<usize, usize> = BTreeMap::new();
    let mut last_decision_per_step: BTreeMap<usize, (usize, String)> = BTreeMap::new();
    let mut completed_steps: HashSet<usize> = HashSet::new();
    let mut failure_counts: HashMap<String, usize> = HashMap::new();

    for log in &attempt_logs {
        *attempts_per_step.entry(log.step_index).or_insert(0) += 1;

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
    let max_step_seen = attempts_per_step.keys().copied().max().unwrap_or(0);
    let retries_len = if attempts_per_step.is_empty() {
        plan_steps
    } else {
        plan_steps.max(max_step_seen + 1)
    };

    let mut retries_per_step = vec![0usize; retries_len];
    for (step_index, count) in attempts_per_step {
        retries_per_step[step_index] = count;
    }

    let mut failure_stages: Vec<(String, usize)> = failure_counts.into_iter().collect();
    failure_stages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let outcome = if steps_completed == plan_steps {
        RunOutcome::Success
    } else if steps_aborted > 0 {
        RunOutcome::Aborted
    } else {
        RunOutcome::CapReached
    };

    let usage_cumulative = attempt_logs
        .last()
        .map(|log| log.usage_cumulative.clone())
        .unwrap_or_default();
    let input_tokens = usage_cumulative.input_tokens;
    let output_tokens = usage_cumulative.output_tokens;
    let total_tokens = usage_cumulative.total();
    let llm_requests = attempt_logs
        .iter()
        .filter(|log| log.usage_this_call.is_some())
        .count() as u64;

    Ok(RunSummary {
        run_id: plan_log.run_id,
        goal: plan_log.goal,
        outcome,
        steps_completed,
        steps_aborted,
        total_attempts,
        retries_per_step,
        failure_stages,
        input_tokens,
        output_tokens,
        total_tokens,
        llm_requests,
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
            path: state_path.display().to_string(),
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
        path: logs_dir.display().to_string(),
        cause: e.to_string(),
    })?;

    for entry in reader {
        let entry = entry.map_err(|e| StatsError::Io {
            path: logs_dir.display().to_string(),
            cause: e.to_string(),
        })?;
        let file_type = entry.file_type().map_err(|e| StatsError::Io {
            path: entry.path().display().to_string(),
            cause: e.to_string(),
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
        avg_attempts,
        avg_tokens,
        most_common_failure_stage,
    })
}

pub fn format_run_summary(summary: &RunSummary) -> String {
    let attempts = summary
        .retries_per_step
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

    let tokens_line = if summary.total_tokens > 0 {
        format!(
            "\nTokens:     {} in / {} out ({} requests)",
            summary.input_tokens, summary.output_tokens, summary.llm_requests
        )
    } else {
        String::new()
    };

    format!(
        "Run:        {}\nGoal:       {}\nOutcome:    {}\nProgress:   {}/{} steps completed, {} aborted\nAttempts:   {} total ({})\nFailures:   {}\nLogs:       .tod/logs/{}/",
        summary.run_id,
        summary.goal,
        summary.outcome,
        summary.steps_completed,
        summary.retries_per_step.len(),
        summary.steps_aborted,
        summary.total_attempts,
        attempts,
        failures,
        summary.run_id,
    ) + &tokens_line
}

pub fn format_multi_run_summary(summary: &MultiRunSummary) -> String {
    let failure = match &summary.most_common_failure_stage {
        Some((stage, count)) => format!("{stage} ({count} occurrences)"),
        None => "none".to_string(),
    };

    let cap_str = if summary.runs_cap_reached > 0 {
        format!("  Cap reached: {}", summary.runs_cap_reached)
    } else {
        String::new()
    };

    format!(
        "Last {} runs:\n  Succeeded: {}  Aborted: {}{}\n  Avg attempts: {:.1}\n  Avg tokens: {:.0}\n  Most common failure: {}",
        summary.runs_total,
        summary.runs_succeeded,
        summary.runs_aborted,
        cap_str,
        summary.avg_attempts,
        summary.avg_tokens,
        failure,
    )
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
        let plan_steps = (0..steps)
            .map(|i| json!({"description": format!("step {i}"), "files": ["src/main.rs"]}))
            .collect::<Vec<_>>();
        let value = json!({
            "run_id": run_id,
            "goal": goal,
            "timestamp_utc": "2026-02-24T14:30:22Z",
            "run_mode": "default",
            "plan": {"steps": plan_steps}
        });
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
        let value = json!({
            "run_id": run_id,
            "step_index": step_index,
            "attempt": attempt,
            "timestamp_utc": "2026-02-24T14:30:25Z",
            "run_mode": "default",
            "edit_batch": {"edits": []},
            "runner_output": {"stage": stage, "ok": ok, "output": "", "truncated": false},
            "review_decision": review_decision
        });
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
                file_count: 0,
                total_bytes: 0,
                hash: "hash".to_string(),
            },
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
    fn summarize_run_retries_per_step() {
        let sandbox = TempSandbox::new();
        let run_id = "20260224_143022";
        let run_dir = sandbox.join(".tod/logs").join(run_id);

        write_plan(&run_dir, run_id, "test goal", 2);
        write_attempt(&run_dir, run_id, 0, 1, "test", false, "retry");
        write_attempt(&run_dir, run_id, 0, 2, "test", false, "retry");
        write_attempt(&run_dir, run_id, 0, 3, "test", true, "proceed");
        write_attempt(&run_dir, run_id, 1, 1, "test", true, "proceed");

        let summary = summarize_run(&run_dir).unwrap();
        assert_eq!(summary.retries_per_step, vec![3, 1]);
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
        assert!((summary.avg_attempts - (4.0 / 3.0)).abs() < f64::EPSILON);
        assert_eq!(
            summary.most_common_failure_stage,
            Some(("build".to_string(), 2))
        );
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
            steps_completed: 1,
            steps_aborted: 0,
            total_attempts: 1,
            retries_per_step: vec![1],
            failure_stages: vec![],
            input_tokens: 1234,
            output_tokens: 567,
            total_tokens: 1801,
            llm_requests: 3,
        };
        let rendered = format_run_summary(&summary);
        assert!(rendered.contains("Tokens:"));
    }

    #[test]
    fn format_run_summary_hides_zero_tokens() {
        let summary = RunSummary {
            run_id: "r1".to_string(),
            goal: "g".to_string(),
            outcome: RunOutcome::Success,
            steps_completed: 1,
            steps_aborted: 0,
            total_attempts: 1,
            retries_per_step: vec![1],
            failure_stages: vec![],
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            llm_requests: 0,
        };
        let rendered = format_run_summary(&summary);
        assert!(!rendered.contains("Tokens:"));
    }
}
