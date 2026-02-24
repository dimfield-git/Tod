# PHASE7.md — Observability Implementation

**Read AGENTS.md first.** All operating principles, coding standards, and safety rules apply.

---

## Goal

Create `src/stats.rs` — a read-only analysis module that derives metrics from the structured JSON logs in `.tod/logs/`. Replace `loop::status()` with stats-driven output. Add cross-run summary capability.

---

## New file: `src/stats.rs`

### Types

```rust
pub enum RunOutcome {
    Success,
    Aborted,
    CapReached,
}

pub struct RunSummary {
    pub run_id: String,
    pub goal: String,
    pub outcome: RunOutcome,
    pub steps_completed: usize,
    pub steps_aborted: usize,
    pub total_attempts: usize,
    pub retries_per_step: Vec<usize>,              // indexed by step_index
    pub failure_stages: Vec<(String, usize)>,      // (stage, count), sorted desc
}

pub struct MultiRunSummary {
    pub runs_total: usize,
    pub runs_succeeded: usize,
    pub runs_aborted: usize,
    pub avg_attempts: f64,
    pub most_common_failure_stage: Option<String>,
}

pub enum StatsError {
    NoData,
    Io { path: String, cause: String },
    InvalidLog { path: String, reason: String },
}
```

Implement `Display` for `StatsError`. Derive `Debug` on all types. `RunOutcome` also needs `Display` (prints `"success"`, `"aborted"`, `"cap_reached"`).

### Functions

**`summarize_run(run_log_dir: &Path) -> Result<RunSummary, StatsError>`**

- Read all `step_*_attempt_*.json` files in the directory.
- Parse each as `AttemptLog` (the struct already exists in `loop.rs` — make it `pub` if needed).
- Read `plan.json` from the same directory to get the goal.
- Compute:
  - `steps_completed`: count distinct `step_index` values where `review_decision == "proceed"`
  - `steps_aborted`: count distinct `step_index` values where the last attempt has `review_decision == "abort"`
  - `total_attempts`: count of attempt log files
  - `retries_per_step`: group by `step_index`, count per group
  - `failure_stages`: group failed attempts (`runner_output.ok == false`) by `runner_output.stage`, sort by count descending
- Determine outcome:
  - All plan steps reached `"proceed"` → `Success`
  - Any step ended with `"abort"` → `Aborted`
  - If total iterations hit the cap (check last attempt context) → `CapReached`

**`summarize_current(project_root: &Path) -> Result<RunSummary, StatsError>`**

- Read `.tod/state.json` to get `run_id` and `log_dir`.
- If missing, return `StatsError::NoData`.
- Call `summarize_run()` on the resolved log directory.

**`summarize_runs(tod_dir: &Path, limit: usize) -> Result<MultiRunSummary, StatsError>`**

- List subdirectories of `tod_dir/logs/`.
- Sort lexicographically descending (newest first). Run IDs are `YYYYMMDD_HHMMSS` — lexicographic == chronological.
- Take the first `limit` entries.
- Call `summarize_run()` on each.
- Aggregate:
  - `runs_succeeded` / `runs_aborted`: count outcomes
  - `avg_attempts`: total attempts across all runs / run count
  - `most_common_failure_stage`: merge all failure stage counts, pick highest

---

## Formatting (stdout output)

**`tod status` output:**

```
Run:        20260224_143022
Goal:       add error handling to parser
Outcome:    success
Progress:   3/3 steps completed, 0 aborted
Attempts:   5 total (step 0: 2, step 1: 1, step 2: 2)
Failures:   test (2), build (1)
Logs:       .tod/logs/20260224_143022/
```

Implement this as a `format_run_summary(summary: &RunSummary) -> String` function in `stats.rs`.

**`tod stats --last N` output:**

```
Last 5 runs:
  Succeeded: 3  Aborted: 2
  Avg attempts: 4.2
  Most common failure: test (7 occurrences)
```

Implement as `format_multi_run_summary(summary: &MultiRunSummary, limit: usize) -> String`.

---

## CLI changes

### In `cli.rs`

Add a `Stats` variant to the `Command` enum:

```rust
/// Analyze run history.
Stats {
    /// Number of recent runs to summarize.
    #[arg(long, default_value = "5")]
    last: usize,
},
```

Keep the existing `Status` variant but have it dispatch to `stats::summarize_current()`.

### In `main.rs`

- `Command::Status` → call `stats::summarize_current()`, print with `format_run_summary()`.
- `Command::Stats { last }` → call `stats::summarize_runs()`, print with `format_multi_run_summary()`.
- Both must handle `StatsError::NoData` gracefully (print a message, exit non-zero).

---

## Migration: remove `loop::status()`

- Delete the `pub fn status()` function from `loop.rs`.
- Delete the `status_displays_summary` and `status_no_checkpoint_fails` tests from `loop.rs` (these will be replaced by equivalent tests in `stats.rs`).
- `LoopError::NoCheckpoint` is still used by `resume()` — do **not** remove it.
- Remove the old status import from `main.rs`.
- Add `mod stats;` to `main.rs`.

---

## Log struct visibility

`AttemptLog`, `RunnerLog`, and `PlanLog` in `loop.rs` need to be readable by `stats.rs`:

- `AttemptLog` and `RunnerLog` should already be `pub`. Verify.
- `PlanLog` is currently private. Make it `pub(crate)` — stats needs to read the goal from `plan.json`.

Do **not** move these structs. They are defined in `loop.rs` and `stats.rs` imports them.

---

## Tests

All tests go in `#[cfg(test)] mod tests` inside `stats.rs`. Use `TempSandbox::new()` — no `with_main_rs` needed.

Tests create fake `.tod/logs/<run_id>/` directories with hand-written JSON. No LLM provider required.

### Test list

| Test | Setup | Assertion |
|------|-------|-----------|
| `summarize_run_success` | Write `plan.json` + 2 attempt logs (both `"proceed"`) | Outcome is `Success`, steps_completed == 2, steps_aborted == 0 |
| `summarize_run_aborted` | Write `plan.json` + attempt log with `"abort"` | Outcome is `Aborted`, steps_aborted == 1 |
| `summarize_run_failure_stages` | Write attempts with `ok: false` across `"build"` and `"test"` stages | `failure_stages` sorted by count, correct totals |
| `summarize_run_retries_per_step` | Write 3 attempts for step 0, 1 for step 1 | `retries_per_step == [3, 1]` |
| `summarize_current_reads_state` | Write `state.json` pointing to a run, write that run's logs | Returns correct `RunSummary` |
| `summarize_current_no_data` | Empty temp dir, no `.tod/` | Returns `StatsError::NoData` |
| `summarize_runs_sorts_chronologically` | Write 3 run dirs with different timestamps | Returned in newest-first order |
| `summarize_runs_respects_limit` | Write 5 run dirs, call with `limit=2` | Only 2 most recent included in aggregates |
| `summarize_runs_aggregates` | Write mix of success/abort runs | Correct success/abort counts, avg attempts, most common failure |
| `summarize_runs_empty` | Write `.tod/logs/` but no run subdirs | Zero counts, `None` for most common failure |

### Test JSON templates

**`plan.json`:**
```json
{
  "run_id": "20260224_143022",
  "goal": "test goal",
  "timestamp_utc": "2026-02-24T14:30:22Z",
  "run_mode": "default",
  "plan": { "steps": [{ "description": "step 0", "files": ["src/main.rs"] }] }
}
```

**`step_0_attempt_1.json`:**
```json
{
  "run_id": "20260224_143022",
  "step_index": 0,
  "attempt": 1,
  "timestamp_utc": "2026-02-24T14:30:25Z",
  "run_mode": "default",
  "edit_batch": { "edits": [] },
  "runner_output": { "stage": "test", "ok": true, "output": "", "truncated": false },
  "review_decision": "proceed"
}
```

Adjust `ok`, `stage`, and `review_decision` per test scenario.

---

## Implementation order

1. Add types (`RunSummary`, `MultiRunSummary`, `RunOutcome`, `StatsError`) to `stats.rs`
2. Implement `summarize_run()`
3. Implement `summarize_current()`
4. Implement `summarize_runs()`
5. Implement formatting functions
6. Wire CLI: add `Stats` command, redirect `Status` to `stats.rs`
7. Remove `loop::status()` and its tests
8. Adjust log struct visibility in `loop.rs` if needed
9. Write all tests
10. Verify: `cargo test`, `cargo clippy -- -D warnings`

**Do not start step 6 until steps 1–5 are tested and passing.**
