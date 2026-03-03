# PHASE14.md — Observability Schema Cohesion & Metrics Fidelity

Read `AGENTS.md` first. All operating principles and safety rules apply.

---

## Goal

Phase 13 made resume behavior deterministic and materially stronger. Phase 14 improves operator trust and maintainability by addressing remaining observability and reporting debt.

Primary outcomes:
- Stats no longer depends on loop-internal log struct ownership.
- Planner-stage failures produce terminal artifacts.
- Multi-run stats report explicit infra-failure outcome buckets.
- LLM request counts reflect logical call semantics, not usage-presence heuristics.

---

## Design Decisions (Locked)

1. **Log schema location**: shared log structs (`RunnerLog`, `AttemptLog`, `PlanLog`, `FinalLog`) live in `src/log_schema.rs`. Pre-run artifact helpers also land there, not in `loop.rs`.
2. **Request counting**: one logical LLM intent = one request. Internal retries in `llm.rs` are invisible to the request count. Retry observability is a separate future concern.
3. **Visibility widening**: `PlanLog` and `FinalLog` widen from `pub(crate)` to `pub` when extracted to `log_schema.rs`.
4. **Task 5 is conditional**: if Tasks 1-2 sufficiently reduce `loop.rs` pressure, Task 5 can be skipped.

---

## Baseline (Current Tree)

Validated on 2026-03-03:
- `cargo test`: **178 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1-13 complete

---

## Task 1: Extract Shared Log Schema

### What
Create `src/log_schema.rs` and move shared run-log structs out of `loop.rs`.

### Structs to move
1. **`RunnerLog`** — with `#[derive]`, doc comment, and all serde attributes.
2. **`default_runner_stage()`** — the free function used by `#[serde(default = "default_runner_stage")]` on `RunnerLog::stage`. Must be in the same module for serde to resolve it.
3. **`AttemptLog`** — with `#[derive]`, doc comment, and all serde attributes.
4. **`PlanLog`** — with `#[derive]` and doc comment. **Widen visibility from `pub(crate)` to `pub`.**
5. **`FinalLog`** — with `#[derive]` and doc comment. **Widen visibility from `pub(crate)` to `pub`.**

### Imports needed in `log_schema.rs`
```rust
use serde::{Deserialize, Serialize};
use crate::llm::Usage;
use crate::planner::Plan;
use crate::schema::EditBatch;
```

### Do NOT move
- `RunState`, `StepState`, `Fingerprint`, `RunProfile` — orchestration state, not log schema.
- `write_plan_log()`, `write_final_log()`, `write_attempt_log()` — `impl RunState` methods that construct log structs; they stay in `loop.rs`.

### Touch points

**`src/main.rs`** — add module declaration after `mod schema;` and before `mod stats;`:
```rust
mod log_schema;
```

**`src/loop.rs`** — remove the four structs and `default_runner_stage()`. Add import:
```rust
use crate::log_schema::{AttemptLog, FinalLog, PlanLog, RunnerLog};
```

**`src/stats.rs`** — change the import line:
```rust
// Before:
use crate::r#loop::{AttemptLog, FinalLog, PlanLog, RunState};

// After:
use crate::log_schema::{AttemptLog, FinalLog, PlanLog};
use crate::r#loop::RunState;
```

**`src/stats.rs` tests** — update any imports that pull log types from `loop`. Test helpers that construct JSON via `serde_json::json!()` do not need changes. Direct struct construction or type references must come from `crate::log_schema`.

### Tests
Add in `src/log_schema.rs` inside a `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_payloads_deserialize_with_defaults() {
        // RunnerLog without "stage" field -> defaults to "review"
        let runner: RunnerLog = serde_json::from_str(
            r#"{"ok": true, "output": "", "truncated": false}"#,
        ).expect("RunnerLog missing stage");
        assert_eq!(runner.stage, "review");

        // FinalLog without optional fields
        let final_log: FinalLog = serde_json::from_str(
            r#"{"run_id": "r1", "timestamp_utc": "t", "outcome": "success"}"#,
        ).expect("FinalLog missing optionals");
        assert!(final_log.step_index.is_none());
        assert!(final_log.attempt.is_none());
        assert!(final_log.message.is_none());

        // AttemptLog without usage_this_call
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
```

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```
Expected: **179 passed** (178 + 1 new), 1 ignored. Clippy clean.

Do not start Task 2 until Task 1 is verified.

---

## Task 2: Planner-Stage Terminal Artifact

### What
When `create_plan` fails before `RunState` exists, write a `final.json` with `outcome: "plan_error"` so the failure is observable. No changes to success-path behavior.

### Helper in `src/log_schema.rs`

Add a public function after the struct definitions:

```rust
/// Write a terminal `final.json` for failures that occur before `RunState` exists.
/// Best-effort: returns `Ok(())` on success, `Err` on I/O failure.
pub fn write_plan_error_artifact(
    log_dir: &std::path::Path,
    run_id: &str,
    message: &str,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(log_dir)?;
    let log = FinalLog {
        run_id: run_id.to_string(),
        timestamp_utc: chrono::Utc::now().to_rfc3339(),
        outcome: "plan_error".to_string(),
        step_index: None,
        attempt: None,
        message: Some(message.to_string()),
    };
    let json = serde_json::to_string_pretty(&log)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(log_dir.join("final.json"), json)
}
```

### Call site in `src/loop.rs`

Locate the plan-failure `Err` path from `create_plan` inside the `run` function, before `RunState` is constructed. At that site:
1. Generate a run ID using the existing `generate_run_id` logic.
2. Construct the log directory path: `.tod/logs/<run_id>/`.
3. Call `crate::log_schema::write_plan_error_artifact(...)` best-effort (log a warning on failure via `crate::warn!`, do not propagate I/O error — the original plan error is still the return value).

Do not restructure the `run` function beyond this minimal insertion.

### Tests

**Unit test in `src/log_schema.rs`** (add to existing `#[cfg(test)] mod tests`):
```rust
#[test]
fn write_plan_error_artifact_creates_final_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log_dir = dir.path().join("logs/test_run");

    let result = write_plan_error_artifact(&log_dir, "test_run", "model refused");
    assert!(result.is_ok());

    let final_path = log_dir.join("final.json");
    assert!(final_path.exists());

    let content: FinalLog =
        serde_json::from_str(&std::fs::read_to_string(&final_path).unwrap())
            .expect("valid FinalLog");
    assert_eq!(content.run_id, "test_run");
    assert_eq!(content.outcome, "plan_error");
    assert_eq!(content.message.as_deref(), Some("model refused"));
    assert!(content.step_index.is_none());
}
```

If `tempfile` is not a dev-dependency, add it (`cargo add tempfile --dev`). If the project uses `TempSandbox` for temp dirs, use that instead.

**Integration test in `src/loop.rs`** (follow existing plan-failure test patterns for mock setup):
```rust
#[test]
fn run_plan_error_writes_terminal_artifact() {
    // Set up a TempSandbox with a .tod/ directory.
    // Configure a mock LLM provider that returns a plan error.
    // Call `run(...)`.
    // Assert that .tod/logs/<run_id>/final.json exists with outcome "plan_error".
}
```

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```
Expected: **181 passed** (179 + 2 new), 1 ignored. Clippy clean.

Do not start Task 3 until Task 2 is verified.

---

## Task 3: Expand Multi-Run Outcome Aggregates

### What
Replace the coarse succeeded/aborted/cap_reached counters in `MultiRunSummary` with per-outcome counts for all seven `RunOutcome` variants. Legacy runs without `final.json` still classify via the existing heuristic fallback.

### Changes in `src/stats.rs`

**Update `MultiRunSummary`:**
```rust
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
```

**Update `summarize_runs`:** populate all fields by matching on each `RunSummary::outcome`.

**Update `format_multi_run_summary`:** show full breakdown, only displaying non-zero counts to keep output clean. Always show `Succeeded` even if 0:
```text
Last 10 runs:
  Succeeded: 7  Aborted: 1  Cap reached: 1  Plan error: 1
  Avg attempts: 3.2
  Avg tokens: 12450
  Most common failure: cargo_check (3 occurrences)
```

Build the outcome line dynamically: iterate over (label, count) pairs, include where count > 0, always include Succeeded.

### Tests

**New:** `summarize_runs_counts_terminal_outcomes` — write test runs with a variety of outcomes (at minimum: one success, one aborted, one plan_error, one cap_reached). Assert each counter.

**Update** any existing tests that assert on exact `format_multi_run_summary` output. Do not weaken assertions.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```
Expected: **182+ passed**, 1 ignored. Clippy clean.

Do not start Task 4 until Task 3 is verified.

---

## Task 4: Request Count Semantics Hardening

### What
Switch request counting from usage-presence heuristics to logical call semantics.

### Change in `src/stats.rs` — `summarize_run`

```rust
// BEFORE (usage-presence heuristic):
let llm_requests_plan: u64 = if plan_log.usage.is_some() { 1 } else { 0 };
let llm_requests_edit = attempt_logs
    .iter()
    .filter(|log| log.usage_this_call.is_some())
    .count() as u64;
let llm_requests_total = llm_requests_plan + llm_requests_edit;

// AFTER (logical call semantics):
let llm_requests_plan: u64 = 1;
let llm_requests_edit = attempt_logs.len() as u64;
let llm_requests_total = llm_requests_plan + llm_requests_edit;
```

### Change in `src/stats.rs` — `format_run_summary`

Remove the legacy suffix. The `llm_requests_plan == 0` branch is now dead code:

```rust
// REMOVE this block entirely:
let legacy_suffix = if summary.llm_requests_plan == 0 {
    " (planner usage unknown - legacy logs)"
} else {
    ""
};
// And remove its interpolation from the format string.
```

### Tests

**New:** `summarize_run_request_count_independent_of_usage_fields`

```rust
#[test]
fn summarize_run_request_count_independent_of_usage_fields() {
    let sb = TempSandbox::new();
    let run_id = "count_test";
    let run_dir = sb.path().join(format!(".tod/logs/{run_id}"));

    // Plan WITHOUT usage field
    write_plan(&run_dir, run_id, "test goal", 1);

    // Two attempt logs WITHOUT usage_this_call
    for attempt in 1..=2 {
        let log = serde_json::json!({
            "run_id": run_id,
            "step_index": 0,
            "attempt": attempt,
            "timestamp_utc": "t",
            "run_mode": "normal",
            "edit_batch": { "edits": [] },
            "runner_output": { "ok": false, "output": "err", "truncated": false },
            "review_decision": "retry"
        });
        let filename = format!("step_0_attempt_{attempt}.json");
        std::fs::write(
            run_dir.join(&filename),
            serde_json::to_string_pretty(&log).unwrap(),
        ).unwrap();
    }

    // final.json
    let final_log = serde_json::json!({
        "run_id": run_id,
        "timestamp_utc": "t",
        "outcome": "aborted",
        "step_index": 0,
        "attempt": 2,
        "message": "test abort"
    });
    std::fs::write(
        run_dir.join("final.json"),
        serde_json::to_string_pretty(&final_log).unwrap(),
    ).unwrap();

    let summary = summarize_run(&run_dir).expect("should summarize");

    // 1 plan + 2 edits = 3, even with no usage fields
    assert_eq!(summary.llm_requests_plan, 1);
    assert_eq!(summary.llm_requests_edit, 2);
    assert_eq!(summary.llm_requests_total, 3);
}
```

Also check existing tests that assert on `llm_requests_*` values — they may need updating since plan requests are now always 1.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```
Expected: **183+ passed**, 1 ignored. Clippy clean.

Do not start Task 5 until Task 4 is verified.

---

## Task 5: Light Loop Surface Reduction (Conditional)

### What
If `loop.rs` still benefits from extraction after Tasks 1-4, perform one behavior-preserving extraction (e.g., logging write helpers). No semantic changes beyond relocation.

### When to skip
If Task 1 (schema extraction) and Task 2 (helper in `log_schema.rs`) have already reduced `loop.rs` to a comfortable size with clear single-responsibility, skip this task.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Expected after Phase 14: all tests pass (target >= baseline), clippy clean.

---

## Implementation Order Summary

| Task | Scope | Reasoning level |
|---|---|---|
| 1. Shared log schema extraction | Coupling reduction | High |
| 2. Planner-stage terminal artifact | Observability closure | High |
| 3. Stats outcome bucket expansion | Reporting fidelity | Medium |
| 4. Request count semantics hardening | Metrics trust | Medium-High |
| 5. Conditional light loop extraction | Maintainability | Medium |

---

## Out of Scope (Phase 15+)

- Patch/diff edit mode.
- Git branch isolation workflow.
- New provider classes (local/offline models).
- Major reviewer policy redesign.
- Retry-level observability (count/latency per LLM call).
