use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tod-contract-{nanos}"));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run_tod(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tod"));
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run tod")
}

fn write_success_run(project_root: &Path, run_id: &str) {
    let log_dir_rel = format!(".tod/logs/{run_id}");
    let run_dir = project_root.join(&log_dir_rel);
    fs::create_dir_all(&run_dir).expect("create run dir");

    let plan = json!({
        "run_id": run_id,
        "goal": "test goal",
        "timestamp_utc": "2026-03-12T00:00:00Z",
        "run_mode": "default",
        "plan": {
            "steps": [
                { "description": "step 0", "files": ["src/main.rs"] }
            ]
        }
    });
    fs::write(
        run_dir.join("plan.json"),
        serde_json::to_string_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");

    let attempt = json!({
        "run_id": run_id,
        "step_index": 0,
        "attempt": 1,
        "timestamp_utc": "2026-03-12T00:00:01Z",
        "run_mode": "default",
        "edit_batch": { "edits": [] },
        "runner_output": { "stage": "test", "ok": true, "output": "", "truncated": false },
        "review_decision": "proceed",
        "usage_cumulative": { "input_tokens": 3, "output_tokens": 2 }
    });
    fs::write(
        run_dir.join("step_0_attempt_1.json"),
        serde_json::to_string_pretty(&attempt).expect("serialize attempt"),
    )
    .expect("write attempt");

    let final_log = json!({
        "run_id": run_id,
        "timestamp_utc": "2026-03-12T00:00:02Z",
        "outcome": "success",
        "llm_requests": 2,
        "input_tokens": 3,
        "output_tokens": 2
    });
    fs::write(
        run_dir.join("final.json"),
        serde_json::to_string_pretty(&final_log).expect("serialize final"),
    )
    .expect("write final");

    let state = json!({
        "goal": "test goal",
        "plan": {
            "steps": [
                { "description": "step 0", "files": ["src/main.rs"] }
            ]
        },
        "step_index": 0,
        "step_state": { "attempt": 0, "retry_context": null },
        "steps_completed": 0,
        "total_iterations": 1,
        "max_iterations_per_step": 5,
        "max_total_iterations": 25,
        "run_id": run_id,
        "log_dir": log_dir_rel,
        "last_log_path": null,
        "fingerprint": {
            "fingerprint_version": 2,
            "file_count": 0,
            "total_bytes": 0,
            "hash": "h"
        }
    });
    fs::create_dir_all(project_root.join(".tod")).expect("create .tod dir");
    fs::write(
        project_root.join(".tod/state.json"),
        serde_json::to_string_pretty(&state).expect("serialize state"),
    )
    .expect("write state");
}

#[test]
fn status_output_contract_human_and_json() {
    let project = TempDir::new();
    write_success_run(project.path(), "run_001");
    let project_str = project.path().to_str().expect("utf8 path");

    let human = run_tod(&["status", "--project", project_str], &[]);
    assert!(human.status.success());
    let human_stdout = String::from_utf8_lossy(&human.stdout);
    let human_stderr = String::from_utf8_lossy(&human.stderr);
    assert!(human_stderr.trim().is_empty());
    assert!(human_stdout.contains("Run:"));
    assert!(human_stdout.contains("Outcome:"));
    assert!(human_stdout.contains("Logs:"));

    let json_out = run_tod(&["status", "--project", project_str, "--json"], &[]);
    assert!(json_out.status.success());
    let json_stdout = String::from_utf8_lossy(&json_out.stdout);
    let json_stderr = String::from_utf8_lossy(&json_out.stderr);
    assert!(json_stderr.trim().is_empty());
    let parsed: Value = serde_json::from_str(&json_stdout).expect("status json");
    assert!(parsed.get("run_id").is_some());
    assert!(parsed.get("outcome").is_some());
    assert!(parsed.get("llm_requests_total").is_some());
}

#[test]
fn stats_output_contract_human_and_json() {
    let project = TempDir::new();
    write_success_run(project.path(), "run_002");
    let project_str = project.path().to_str().expect("utf8 path");

    let human = run_tod(&["stats", "--project", project_str, "--last", "1"], &[]);
    assert!(human.status.success());
    let human_stdout = String::from_utf8_lossy(&human.stdout);
    let human_stderr = String::from_utf8_lossy(&human.stderr);
    assert!(human_stderr.trim().is_empty());
    assert!(human_stdout.contains("Last 1 runs:"));
    assert!(human_stdout.contains("Avg attempts:"));
    assert!(human_stdout.contains("Most common failure:"));

    let json_out = run_tod(&["stats", "--project", project_str, "--last", "1", "--json"], &[]);
    assert!(json_out.status.success());
    let json_stdout = String::from_utf8_lossy(&json_out.stdout);
    let json_stderr = String::from_utf8_lossy(&json_out.stderr);
    assert!(json_stderr.trim().is_empty());
    let parsed: Value = serde_json::from_str(&json_stdout).expect("stats json");
    assert!(parsed.get("runs_total").is_some());
    assert!(parsed.get("avg_attempts").is_some());
    assert!(parsed.get("avg_tokens").is_some());
}

#[test]
fn run_quiet_suppresses_lifecycle_but_keeps_errors() {
    let root = TempDir::new();
    let missing_project = root.path().join("missing-project");
    let missing_project_str = missing_project.to_str().expect("utf8 path");
    let envs = [("ANTHROPIC_API_KEY", "dummy-test-key")];

    let noisy = run_tod(&["run", "--project", missing_project_str, "goal"], &envs);
    assert!(!noisy.status.success());
    let noisy_stdout = String::from_utf8_lossy(&noisy.stdout);
    let noisy_stderr = String::from_utf8_lossy(&noisy.stderr);
    assert!(noisy_stdout.trim().is_empty());
    assert!(noisy_stderr.contains("tod: running in"));
    assert!(noisy_stderr.contains("run failed:"));

    let quiet = run_tod(
        &["run", "--project", missing_project_str, "--quiet", "goal"],
        &envs,
    );
    assert!(!quiet.status.success());
    let quiet_stdout = String::from_utf8_lossy(&quiet.stdout);
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(quiet_stdout.trim().is_empty());
    assert!(!quiet_stderr.contains("tod: running in"));
    assert!(quiet_stderr.contains("run failed:"));
}

#[test]
fn status_no_data_writes_to_stderr_and_keeps_stdout_clean() {
    let project = TempDir::new();
    let project_str = project.path().to_str().expect("utf8 path");

    let output = run_tod(&["status", "--project", project_str], &[]);
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.trim().is_empty());
    assert!(stderr.contains("no run data found"));
}
