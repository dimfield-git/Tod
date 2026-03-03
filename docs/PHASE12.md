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

### What Is Working Well

1. Runtime safety model is strong for normal paths:
   - Path checks reject absolute and traversal paths with symlink-aware existing-ancestor checks (`src/schema.rs:156-207`).
   - Edit application is transactional with rollback (`src/runner.rs:92-155`).
   - Retry transport logic is contained in provider (`src/llm.rs:137-227`).

2. Token usage and planner accounting improved significantly in Phase 11:
   - Planner usage is persisted in `plan.json` (`src/loop.rs:141-149`, `src/loop.rs:250-267`).
   - Stats now splits request counts (`plan/edit/total`) and annotates legacy logs (`src/stats.rs:235-240`, `src/stats.rs:381-395`).

3. Resuming at exhausted cap is correctly blocked:
   - Pre-resume guard uses `>=` (`src/loop.rs:669-674`).

### Material Gaps

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

Add a run-finalization log (file: `.tod/logs/<run_id>/final.json`) written exactly once on terminal exit paths after a `RunState` exists.

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

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `run_writes_final_log_on_success` | `final.json` exists with outcome `"success"` |
| `run_writes_final_log_on_cap_reached` | `final.json` exists with outcome `"cap_reached"` |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **162 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 2 until Task 1 is verified.**

---

## Task 2: Log/checkpoint pre-runner failures (`EditError`, `ApplyError`)

### What

Before returning `LoopError::Edit` or `LoopError::Apply` from `run_from_state()`, write an explicit attempt record and checkpoint.

### Design decision: failure vocabulary

Pre-runner failures are **not** reviewer aborts. They must use distinct stage and decision values so that stats and humans can distinguish "reviewer said stop" from "edit generation blew up."

**New stage values** (strings in the attempt log `stage` field):

| Failure point | `stage` value | `decision` value |
|---------------|---------------|------------------|
| `create_edits(...)` returns error | `"edit_generation"` | `"error"` |
| `apply_edits(...)` returns error | `"edit_application"` | `"error"` |
| Reviewer says stop | (existing) `"review"` | `"abort"` |

The `decision` field `"error"` is the key discriminator. Existing attempt logs use `"proceed"`, `"retry"`, or `"abort"` — all of which imply the reviewer ran. `"error"` means the loop never reached review.

**Attempt log content for error cases:**
- `output`: the error message string (from the `EditError` or `ApplyError` Display impl)
- `usage_this_call`: `None` (no LLM call completed for `ApplyError`; for `EditError`, the call failed so no usage to record)
- `usage_cumulative`: current `state.usage` snapshot
- `review_decision`: `"error"`

### Implementation direction

1. For `EditError` at `src/loop.rs:544-551`:
   - Before the `?` return, construct an `AttemptLog` with `stage: "edit_generation"`, `decision: "error"`, `output` containing the error Display string.
   - Call `write_attempt_log(...)` and `state.checkpoint(...)`.

2. For `ApplyError` at `src/loop.rs:568-572`:
   - Same pattern, with `stage: "edit_application"`, `decision: "error"`.

3. Also write `final.json` (from Task 1) with the appropriate `edit_error` or `apply_error` outcome before returning the error.

### Why

Makes logs sufficient for diagnosis even when failure happens before `review(...)`. The distinct vocabulary prevents stats from misclassifying these as reviewer aborts.

### Touch points

- `src/loop.rs`

### Risk

Medium (must avoid breaking existing attempt-log consumers — the new `decision: "error"` value must be handled gracefully by stats).

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `edit_error_writes_attempt_and_checkpoint` | Trigger `create_edits` failure. Attempt log exists with `stage: "edit_generation"`, `decision: "error"`. Checkpoint reflects incremented attempt. `final.json` has outcome `"edit_error"`. |
| `apply_error_writes_attempt_and_checkpoint` | Trigger `apply_edits` failure. Attempt log exists with `stage: "edit_application"`, `decision: "error"`. Checkpoint updated. `final.json` has outcome `"apply_error"`. |
| `error_attempt_does_not_count_as_abort` | A run that ends via `EditError` has `steps_aborted == 0` in its attempt log metadata (i.e., the error decision is not conflated with abort). |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **165 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 3 until Task 2 is verified.**

---

## Task 3: Make stats outcome explicit (use `final.json` when present)

### What

Update `summarize_run()` to read `final.json` first (if available) and map that to `RunOutcome`; only fall back to heuristic inference for legacy runs without `final.json`.

Also surface terminal reason text in run summary when available.

The new `"error"` decision value from Task 2 must be handled: stats should not count `decision: "error"` attempts as aborts. Error-stage attempts are infrastructure failures, not review decisions.

### Why

Avoids misclassifying runs that fail outside review-controlled paths.

### Touch points

- `src/stats.rs`

### Risk

Medium (backward-compat and formatting changes).

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `summarize_run_prefers_final_log_outcome` | Run with `final.json` outcome `"edit_error"` — stats reports `EditError`, not heuristic fallback. |
| `summarize_run_error_decision_not_counted_as_abort` | Attempt with `decision: "error"` does not increment `steps_aborted`. |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **167 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 4 until Task 3 is verified.**

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

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `summarize_run_legacy_without_final_log_still_works` | Old-format logs (no `final.json`) still produce valid summary via heuristic path. |
| `legacy_attempt_without_stage_deserializes` | Attempt log missing `stage` field deserializes with sensible default (e.g., `"review"`). |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **169 passing, 1 ignored.**

**Codex reasoning level: medium**

**Do not start Task 5 until Task 4 is verified.**

---

## Task 5: Documentation updates for observability semantics

### What

Document final-log semantics and new outcome fidelity behavior:
1. `AGENTS.md` runtime output map — add `final.json` entry.
2. `AGENTS.md` architectural invariants — add failure observability invariant.
3. `README.md` status/stats interpretation notes (brief).

### Changes to AGENTS.md

- **Runtime output directory:** Add `final.json` line:
  ```
  final.json                          Written once on run exit (outcome, step, message)
  ```
- **Architectural invariants:** Add: "Every run exit after planning produces a `final.json` with explicit outcome. Stats uses `final.json` as source of truth when present; falls back to heuristic inference for legacy logs."
- **Baseline:** Update to final test count (expected ≥169).
- **Phase table:** Phase 12 → `✅ Done`.
- **Phases complete:** `Phases 1–11 complete` → `Phases 1–12 complete`.
- **Priority order:** `PHASE12.md` → next phase file.

### Why

Prevent operator confusion and preserve parity with runtime behavior.

### Touch points

- `AGENTS.md`
- `README.md`

### Risk

Low.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

No test count change.

**Codex reasoning level: low**

---

## Implementation order summary

| Task | Scope | Files touched | Reasoning level |
|------|-------|---------------|-----------------|
| 1. Persist terminal outcome (`final.json`) | New log artifact | `loop.rs` | High |
| 2. Log/checkpoint pre-runner failures | Error-path logging | `loop.rs` | High |
| 3. Stats outcome from `final.json` | Stats rewrite | `stats.rs` | High |
| 4. Backward compatibility | Legacy tolerance | `loop.rs`, `stats.rs` | Medium |
| 5. Documentation | Docs closure | `AGENTS.md`, `README.md` | Low |

**Do not start a later task until the preceding task is verified passing.**

---

## Verification Plan

After each task:

```bash
cargo test
cargo clippy -- -D warnings
```

Target after Phase 12:
- `cargo test`: **169+ passed, 1 ignored**
- `cargo clippy -- -D warnings`: clean

Suggested targeted checks:

```bash
rg -n "final.json|FinalLog|write_attempt_log|LoopError::Edit|LoopError::Apply" src/loop.rs
rg -n "summarize_run|RunOutcome|final|\"error\"" src/stats.rs
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
