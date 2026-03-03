use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use crate::log_schema::{AttemptLog, FinalLog, PlanLog};

/// Allocated identity for a new run.
#[derive(Debug, Clone)]
pub struct RunIdentity {
    pub run_id: String,
    pub log_dir: String,
}

/// Allocate a unique run identity using timestamp + numeric suffix collision policy.
pub fn allocate_run_identity(project_root: &Path) -> RunIdentity {
    let base_id = chrono::Utc::now().format("%Y%m%d_%H%M%S%.6f").to_string();
    allocate_run_identity_from_base(project_root, base_id)
}

fn allocate_run_identity_from_base(project_root: &Path, base_id: String) -> RunIdentity {
    let mut run_id = base_id.clone();
    let mut log_dir = format!(".tod/logs/{run_id}");
    let mut suffix = 2usize;
    while project_root.join(&log_dir).exists() {
        run_id = format!("{base_id}_{suffix}");
        log_dir = format!(".tod/logs/{run_id}");
        suffix += 1;
    }
    RunIdentity { run_id, log_dir }
}

/// Best-effort checkpoint writer with atomic tmp+rename semantics.
pub fn write_checkpoint<T: Serialize>(project_root: &Path, state: &T) {
    let tod_dir = project_root.join(".tod");
    if fs::create_dir_all(&tod_dir).is_err() {
        crate::warn!("could not create .tod directory");
        return;
    }

    match serde_json::to_string_pretty(state) {
        Ok(json) => {
            let tmp_path = tod_dir.join("state.json.tmp");
            let final_path = tod_dir.join("state.json");
            if let Err(e) = fs::write(&tmp_path, json) {
                crate::warn!("failed to write checkpoint: {e}");
                return;
            }
            if let Err(e) = fs::rename(&tmp_path, &final_path) {
                crate::warn!("failed to finalize checkpoint: {e}");
            }
        }
        Err(e) => crate::warn!("failed to serialize checkpoint: {e}"),
    }
}

/// Best-effort `plan.json` writer.
pub fn write_plan_log(log_dir: &Path, log: &PlanLog) {
    if fs::create_dir_all(log_dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(log) {
        let _ = fs::write(log_dir.join("plan.json"), json);
    }
}

/// Best-effort `final.json` writer.
pub fn write_final_log(log_dir: &Path, log: &FinalLog) {
    if fs::create_dir_all(log_dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(log) {
        let _ = fs::write(log_dir.join("final.json"), json);
    }
}

/// Best-effort per-attempt log writer.
pub fn write_attempt_log(log_dir: &Path, filename: &str, log: &AttemptLog) {
    if fs::create_dir_all(log_dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string_pretty(log) {
        let _ = fs::write(log_dir.join(filename), json);
    }
}

/// Write a terminal `final.json` for failures that occur before `RunState` exists.
/// Returns `Err` on I/O failure so callers can decide how to surface best-effort warnings.
pub fn write_plan_error_artifact(log_dir: &Path, run_id: &str, message: &str) -> Result<(), io::Error> {
    fs::create_dir_all(log_dir)?;
    let log = FinalLog {
        run_id: run_id.to_string(),
        timestamp_utc: chrono::Utc::now().to_rfc3339(),
        outcome: "plan_error".to_string(),
        step_index: None,
        attempt: None,
        message: Some(message.to_string()),
    };
    let json = serde_json::to_string_pretty(&log).map_err(io::Error::other)?;
    fs::write(log_dir.join("final.json"), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{Plan, PlanStep};
    use crate::test_util::TempSandbox;

    #[derive(Serialize)]
    struct DummyState {
        value: u32,
    }

    #[test]
    fn write_plan_error_artifact_creates_final_json() {
        let sandbox = TempSandbox::new();
        let log_dir = sandbox.join(".tod/logs/test_run");

        let result = write_plan_error_artifact(&log_dir, "test_run", "model refused");
        assert!(result.is_ok());

        let final_path = log_dir.join("final.json");
        assert!(final_path.exists());

        let content: FinalLog = serde_json::from_str(&std::fs::read_to_string(&final_path).unwrap())
            .expect("valid FinalLog");
        assert_eq!(content.run_id, "test_run");
        assert_eq!(content.outcome, "plan_error");
        assert_eq!(content.message.as_deref(), Some("model refused"));
        assert!(content.step_index.is_none());
    }

    #[test]
    fn checkpoint_write_is_atomic_and_cleans_tmp_file() {
        let sandbox = TempSandbox::new();
        write_checkpoint(&sandbox, &DummyState { value: 7 });

        assert!(sandbox.join(".tod/state.json").exists());
        assert!(!sandbox.join(".tod/state.json.tmp").exists());
    }

    #[test]
    fn plan_log_write_is_best_effort_when_dir_creation_fails() {
        let sandbox = TempSandbox::new();
        let blocker = sandbox.join("not_a_directory");
        fs::write(&blocker, "x").unwrap();

        let log = PlanLog {
            run_id: "r1".to_string(),
            goal: "g".to_string(),
            timestamp_utc: "t".to_string(),
            run_mode: "default".to_string(),
            plan: Plan {
                steps: vec![PlanStep {
                    description: "s".to_string(),
                    files: vec!["src/main.rs".to_string()],
                }],
            },
            usage: None,
        };
        write_plan_log(&blocker.join("child"), &log);
        assert!(!blocker.join("child/plan.json").exists());
    }

    #[test]
    fn allocate_run_identity_uses_suffix_collision_policy() {
        let sandbox = TempSandbox::new();
        let base = "20260303_010203.123456".to_string();

        let first = allocate_run_identity_from_base(&sandbox, base.clone());
        assert_eq!(first.run_id, base);
        assert_eq!(first.log_dir, ".tod/logs/20260303_010203.123456");

        fs::create_dir_all(sandbox.join(&first.log_dir)).unwrap();
        let second = allocate_run_identity_from_base(&sandbox, base.clone());
        assert_eq!(second.run_id, "20260303_010203.123456_2");
        assert_eq!(second.log_dir, ".tod/logs/20260303_010203.123456_2");

        fs::create_dir_all(sandbox.join(&second.log_dir)).unwrap();
        let third = allocate_run_identity_from_base(&sandbox, base);
        assert_eq!(third.run_id, "20260303_010203.123456_3");
        assert_eq!(third.log_dir, ".tod/logs/20260303_010203.123456_3");
    }
}
