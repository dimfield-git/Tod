# Phase 12 Implementation Log

Date: March 3, 2026  
Phase: 12 - Failure Observability and Outcome Fidelity  
Repository: Tod (`/home/dim/Agents/Tod`)

## Executive Summary

Phase 12 was implemented to close observability gaps in non-success termination paths and to remove heuristic ambiguity from run outcome reporting. Prior to this phase, runs that failed before reviewer evaluation (`EditError`, `ApplyError`) could exit without complete attempt-level diagnostics, and stats could misclassify terminal outcomes when only partial logs existed.

This implementation introduces explicit terminal outcome logging (`final.json`), structured pre-review failure attempt logs, and stats outcome resolution that prefers terminal truth when available while preserving backward compatibility for legacy logs.

## Baseline and Exit Criteria

Baseline before implementation:
- `cargo test`: 160 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

Post-implementation result:
- `cargo test`: 169 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

## Scope Completed

All five Phase 12 tasks were completed in sequence with verification gates between tasks.

## Task 1 - Persist Structured Terminal Outcome (`final.json`)

### Objective
Ensure every run exit path after planning emits a single terminal artifact at:

`<project_root>/.tod/logs/<run_id>/final.json`

### Implementation
File changed:
- `src/loop.rs`

Added:
- `FinalLog` struct with fields:
  - `run_id`
  - `timestamp_utc`
  - `outcome`
  - `step_index` (optional)
  - `attempt` (optional)
  - `message` (optional)
- `RunState::write_final_log(...)` helper.

Wired terminal logging into exit paths:
- `success`
- `cap_reached`
- `token_cap`
- `aborted`

### Tests Added
- `run_writes_final_log_on_success`
- `run_writes_final_log_on_cap_reached`

### Verification
- `cargo test`: 162 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

## Task 2 - Log and Checkpoint Pre-Runner Failures

### Objective
Make `EditError` and `ApplyError` paths fully reconstructible from logs by writing:
- an explicit attempt log,
- checkpoint update,
- terminal `final.json`.

### Implementation
File changed:
- `src/loop.rs`

Changes:
- Replaced `?` early-return behavior for `create_edits(...)` and `apply_edits(...)` with explicit error handling.
- On edit-generation failure:
  - `runner_output.stage = "edit_generation"`
  - `review_decision = "error"`
  - empty `EditBatch` for logging continuity
  - checkpoint persisted before return
  - `final.json.outcome = "edit_error"`
- On edit-application failure:
  - `runner_output.stage = "edit_application"`
  - `review_decision = "error"`
  - checkpoint persisted before return
  - `final.json.outcome = "apply_error"`

Design behavior preserved:
- Error attempts are not treated as reviewer aborts.
- Usage accounting behavior remains unchanged on failed calls.

### Tests Added
- `edit_error_writes_attempt_and_checkpoint`
- `apply_error_writes_attempt_and_checkpoint`
- `error_attempt_does_not_count_as_abort`

### Verification
- `cargo test`: 165 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

## Task 3 - Stats Outcome Fidelity via `final.json`

### Objective
Use terminal log outcome as source of truth when present; fall back to legacy inference when absent.

### Implementation
File changed:
- `src/stats.rs`

Changes:
- Imported and read `FinalLog` from `final.json` if present.
- Expanded `RunOutcome` variants:
  - `TokenCap`
  - `EditError`
  - `ApplyError`
  - `PlanError`
- Added mapping function for terminal outcome strings.
- Updated `summarize_run()` behavior:
  - derive heuristic outcome (legacy behavior),
  - override with `final.json` outcome when available.
- Added `terminal_message: Option<String>` to `RunSummary`.
- Updated formatted status output to include:
  - `Terminal: <message>` line when available.
- Multi-run aggregate compatibility:
  - `TokenCap` counted under cap-reached bucket,
  - error outcomes excluded from aborted/cap/success counters.

### Tests Added
- `summarize_run_prefers_final_log_outcome`
- `summarize_run_error_decision_not_counted_as_abort`

### Verification
- `cargo test`: 167 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

## Task 4 - Backward Compatibility Hardening

### Objective
Ensure mixed-generation logs remain readable and valid.

### Implementation
Files changed:
- `src/loop.rs`
- `src/stats.rs`

Changes:
- Added serde default for legacy missing `runner_output.stage`:
  - default value: `"review"`
- Maintained optional read path for missing `final.json` in stats.

### Tests Added
- `legacy_attempt_without_stage_deserializes`
- `summarize_run_legacy_without_final_log_still_works`

### Verification
- `cargo test`: 169 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

## Task 5 - Documentation Closure

### Objective
Document new observability semantics and update project phase state.

### Implementation
Files changed:
- `AGENTS.md`
- `README.md`

Documentation updates include:
- runtime output map now includes `final.json`,
- architectural invariant added for explicit terminal outcome logging,
- status/stats interpretation notes for `final.json` preference and legacy fallback,
- phase/baseline metadata updated to reflect Phase 12 completion and new test baseline.

### Verification
- `cargo test`: 169 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

## Final Artifact Semantics

Run artifacts under `.tod/logs/<run_id>/` now provide:
- `plan.json`: planned work + optional planner usage,
- `step_N_attempt_M.json`: per-attempt execution records (including pre-review errors),
- `final.json`: terminal run outcome and terminal reason.

This allows post-hoc reconstruction of failure progression without depending on checkpoint state.

## Risk and Reliability Notes

Phase 12 resolved the highest-impact observability and outcome-fidelity gaps, but the following known items remain outside scope:
- Workspace fingerprint remains size/path-based rather than content-hash-based.
- `stats` still imports log structs from `loop` (tight coupling remains).
- One non-test `expect` in `main.rs` was not addressed in this phase.

## Outcome

Phase 12 delivered complete terminal observability for post-plan runs, explicit differentiation of pre-review infrastructure failures, and deterministic stats outcome reporting for new logs with safe behavior for historical logs.
