# PHASE12.md — Failure Observability & Outcome Fidelity

**Read `AGENTS.md` first.** All operating principles and safety rules apply.

---

## Goal

Make run artifacts trustworthy for every non-success path, not only runner-reviewed failures. After Phase 11, token/request accounting is materially improved, but failure observability is still incomplete for pre-runner exits (`EditError`, `ApplyError`), and stats still infer outcomes heuristically from attempt logs.

Phase 12 should ensure:
1. Any run failure after planning is reconstructible from `.tod/logs/<run_id>/` alone.
2. Reported run outcome reflects true terminal cause, not a heuristic fallback.
3. Backward compatibility with legacy logs remains intact.

---

## Baseline (Current Tree)

Validated on 2026-03-02:
- `cargo test`: **160 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1–11 complete

---

## Deep Assessment (Post-Phase-11)

## What Is Working Well

1. Runtime safety model is strong for normal paths:
- Path checks reject absolute and traversal paths with symlink-aware existing-ancestor checks (`src/schema.rs:156-207`).
- Edit application is transactional with rollback (`src/runner.rs:92-155`).
- Retry transport logic is contained in provider (`src/llm.rs:137-227`).

2. Token usage and planner accounting improved significantly in Phase 11:
- Planner usage is persisted in `plan.json` (`src/loop.rs:141-149`, `src/loop.rs:250-267`).
- Stats now splits request counts (`plan/edit/total`) and annotates legacy logs (`src/stats.rs:235-240`, `src/stats.rs:381-395`).

3. Resuming at exhausted cap is correctly blocked:
- Pre-resume guard uses `>=` (`src/loop.rs:669-674`).

## Material Gaps

1. **Pre-runner failures still bypass structured attempt logging.**
- `create_edits(...)` error returns immediately via `?` (`src/loop.rs:544-551`) with no `write_attempt_log(...)` and no checkpoint for that attempt.
- `apply_edits(...)` error also returns immediately via `?` (`src/loop.rs:568-572`) with no failure attempt log/checkpoint.
- Consequence: operators cannot diagnose these exits from run logs alone, and state progress may lag actual attempts.

2. **Outcome classification in stats is heuristic and can mislabel runs.**
- Outcome currently inferred from `completed_steps`, `steps_aborted`, and presence of `abort` review decisions (`src/stats.rs:220-226`).
- If a run exits via `LoopError::Edit`/`LoopError::Apply` before review logging, `steps_aborted == 0` and the run can be interpreted as `CapReached` even when the real cause is an immediate error.

3. **Stats is tightly coupled to loop internals.**
- `stats.rs` directly imports `AttemptLog`, `PlanLog`, and `RunState` from `loop` (`src/stats.rs:8`).
- This raises change friction: orchestration/log schema evolution requires synchronized refactors in stats.

4. **Residual reliability debt remains outside this phase scope but should be tracked.**
- Fingerprint is intentionally size-only (`src/loop.rs:50-52`, `src/loop.rs:84`, `src/loop.rs:97-100`), so same-size edits can evade drift detection.
- `main` still has one non-test `expect` (`src/main.rs:29-31`), contradicting the no-unwrap/expect invariant in spirit.

---

## Proposed Phase 12 Scope

Theme: **Failure Observability + Outcome Fidelity**

Five tasks, in order. Tasks 1–3 are core. Task 4 is compatibility hardening. Task 5 is doc closure.

---

## Task 1: Persist structured terminal outcome for every run

### What

Add a run-finalization log (suggested file: `.tod/logs/<run_id>/final.json`) written exactly once on terminal exit paths after a `RunState` exists.

Suggested shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalLog {
    run_id: String,
    timestamp_utc: String,
    outcome: String, // success | aborted | cap_reached | token_cap | edit_error | apply_error | plan_error
    step_index: Option<usize>,
    attempt: Option<usize>,
    message: Option<String>,
}
```

### Why

This decouples terminal status truth from attempt-log inference and gives stats a single source of truth for run outcome.

### Touch points

- `src/loop.rs`

### Risk

Medium (new log artifact and call-site fanout).

---

## Task 2: Log/checkpoint pre-runner failures (`EditError`, `ApplyError`)

### What

Before returning `LoopError::Edit` or `LoopError::Apply` from `run_from_state()`, write an explicit attempt record and checkpoint.

Implementation direction:
1. For `EditError`, emit synthetic failure attempt with stage like `edit_generation`, decision `abort`, and error message in output.
2. For `ApplyError`, emit synthetic failure attempt with stage `apply_edits`, decision `abort`, and error message.
3. Immediately checkpoint state after writing the failure attempt.

### Why

Makes logs sufficient for diagnosis even when failure happens before `review(...)`.

### Touch points

- `src/loop.rs`
- optional light updates in `src/stats.rs` if stage labels require formatting tweaks

### Risk

Medium (must avoid breaking existing attempt-log consumers).

---

## Task 3: Make stats outcome explicit (use `final.json` when present)

### What

Update `summarize_run()` to read `final.json` first (if available) and map that to `RunOutcome`; only fall back to heuristic inference for legacy runs without `final.json`.

Also surface terminal reason text in run summary when available.

### Why

Avoids misclassifying runs that fail outside review-controlled paths.

### Touch points

- `src/stats.rs`

### Risk

Medium (backward-compat and formatting changes).

---

## Task 4: Backward compatibility contract for mixed log generations

### What

Ensure all newly introduced fields/files are optional and legacy-safe:
1. `#[serde(default)]` on new fields.
2. Stats path that tolerates missing `final.json` and missing optional fields.
3. Add explicit tests for old plan/attempt formats + no final log.

### Why

Users already have old `.tod` runs; phase rollout must not break `status`/`stats`.

### Touch points

- `src/loop.rs`
- `src/stats.rs`

### Risk

Low-medium.

---

## Task 5: Documentation updates for observability semantics

### What

Document final-log semantics and new outcome fidelity behavior:
1. `AGENTS.md` runtime output map.
2. `README.md` status/stats interpretation notes (brief).

### Why
Prevent operator confusion and preserve parity with runtime behavior.

### Touch points

- `AGENTS.md`
- `README.md`

### Risk

Low.

---

## Tests to Add/Update

Minimum suggested additions:

1. `edit_error_writes_attempt_and_checkpoint`
- Trigger `create_edits` failure.
- Assert: new attempt log exists with failure stage; checkpoint reflects incremented attempt.

2. `apply_error_writes_attempt_and_checkpoint`
- Trigger `apply_edits` failure.
- Assert: failure attempt log + checkpoint.

3. `run_writes_final_log_on_success`
- Assert `final.json` outcome `success`.

4. `run_writes_final_log_on_edit_error`
- Assert explicit `edit_error` outcome and message.

5. `summarize_run_prefers_final_log_outcome`
- Ensure stats maps explicit final outcome correctly.

6. `summarize_run_legacy_without_final_log_still_works`
- Ensure old logs continue to summarize.

---

## Verification Plan

After each task:

```bash
cargo test
cargo clippy -- -D warnings
```

Target after Phase 12 (estimate):
- `cargo test`: **166+ passed, 1 ignored**
- `cargo clippy -- -D warnings`: clean

Suggested targeted checks:

```bash
rg -n "final.json|FinalLog|write_attempt_log|LoopError::Edit|LoopError::Apply" src/loop.rs
rg -n "summarize_run|RunOutcome|final" src/stats.rs
```

---

## Out of Scope (Track Next)

These are important, but should not be bundled into Phase 12:

1. Content-aware fingerprinting (`src/loop.rs:50-52`, `src/loop.rs:97-100`).
2. Decoupling stats schema types from `loop` (`src/stats.rs:8`).
3. Removing the remaining non-test `expect` in `main` (`src/main.rs:29-31`).

These are better candidates for Phase 13 (resume drift hardening + structural cleanup).

---

## Recommended Decision

Proceed with Phase 12 as defined above: **Failure Observability & Outcome Fidelity**.

This closes the highest remaining trust gap in operational diagnostics without requiring broad architecture refactors.
