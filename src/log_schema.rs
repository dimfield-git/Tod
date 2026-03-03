use serde::{Deserialize, Serialize};

use crate::llm::Usage;
use crate::planner::Plan;
use crate::schema::EditBatch;

/// Structured log for a single edit->apply->run->review cycle.
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
    #[serde(default = "default_runner_stage")]
    pub stage: String,
    pub ok: bool,
    pub output: String,
    pub truncated: bool,
}

fn default_runner_stage() -> String {
    "review".to_string()
}

/// Plan log written once after planning completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanLog {
    pub run_id: String,
    pub goal: String,
    pub timestamp_utc: String,
    pub run_mode: String,
    pub plan: Plan,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Terminal run outcome log written once on exit paths after planning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalLog {
    pub run_id: String,
    pub timestamp_utc: String,
    pub outcome: String,
    #[serde(default)]
    pub step_index: Option<usize>,
    #[serde(default)]
    pub attempt: Option<usize>,
    #[serde(default)]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_payloads_deserialize_with_defaults() {
        let runner: RunnerLog =
            serde_json::from_str(r#"{"ok": true, "output": "", "truncated": false}"#)
                .expect("RunnerLog missing stage");
        assert_eq!(runner.stage, "review");

        let final_log: FinalLog = serde_json::from_str(
            r#"{"run_id": "r1", "timestamp_utc": "t", "outcome": "success"}"#,
        )
        .expect("FinalLog missing optionals");
        assert!(final_log.step_index.is_none());
        assert!(final_log.attempt.is_none());
        assert!(final_log.message.is_none());

        let attempt_json = serde_json::json!({
            "run_id": "r1",
            "step_index": 0,
            "attempt": 1,
            "timestamp_utc": "t",
            "run_mode": "normal",
            "edit_batch": { "edits": [] },
            "runner_output": { "ok": true, "output": "", "truncated": false },
            "review_decision": "proceed"
        });
        let attempt: AttemptLog =
            serde_json::from_value(attempt_json).expect("AttemptLog missing usage");
        assert!(attempt.usage_this_call.is_none());
        assert_eq!(attempt.usage_cumulative.input_tokens, 0);
    }

}
