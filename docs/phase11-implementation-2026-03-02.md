# Phase 11 Implementation Log (Reliability Accounting)

Date: 2026-03-02  
Scope source: `PHASE11.md`

This document records the end-to-end implementation of Phase 11: reliability accounting.

## 1. Objective

Phase 11 addressed three operator-trust gaps:

1. Resume could still issue LLM calls when token budget was already exhausted.
2. Planner usage was not persisted in `plan.json`, so planner cost was invisible to stats.
3. Stats request accounting undercounted and used a semantically misleading field name.

The phase also included display/test updates and project-documentation closure.

## 2. Files changed

- `src/loop.rs`
- `src/stats.rs`
- `AGENTS.md`

## 3. Task-by-task implementation

## Task 1: Pre-resume token cap guard

### Changes

In `resume()` (`src/loop.rs`), added a pre-loop cap check after fingerprint verification and before `run_from_state(...)`:

```rust
if state.max_tokens > 0 && state.usage.total() >= state.max_tokens {
    return Err(LoopError::TokenCapExceeded {
        used: state.usage.total(),
        cap: state.max_tokens,
    });
}
```

### Tests

Added:
- `resume_at_token_cap_returns_error`

This test writes a checkpoint at exact cap (`100`) and asserts resume returns:
- `LoopError::TokenCapExceeded { used: 100, cap: 100 }`

### Verification

- `cargo test`: **155 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## Task 2: Record planner usage in `plan.json`

### Changes

1. Extended `PlanLog` in `src/loop.rs`:

```rust
#[serde(default)]
pub usage: Option<Usage>,
```

2. Updated `write_plan_log()` signature:
- before: `fn write_plan_log(&self, config: &RunConfig)`
- after:  `fn write_plan_log(&self, config: &RunConfig, usage: Option<Usage>)`

3. Updated `run()` call site:
- before: `state.write_plan_log(config);`
- after:  `state.write_plan_log(config, plan_usage.clone());`

### Tests

Added:
- `plan_log_includes_usage`
- `plan_log_without_usage_deserializes`

Coverage includes both new-format and legacy-format compatibility.

### Verification

- `cargo test`: **157 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## Task 3: Fix stats request accounting

### Changes

In `src/stats.rs`:

1. Renamed semantic field:
- `retries_per_step` -> `attempts_per_step`

2. Replaced single request field with split accounting:

```rust
pub llm_requests_total: u64,
pub llm_requests_plan: u64,
pub llm_requests_edit: u64,
```

3. Updated `summarize_run()` to compute:
- `llm_requests_plan` from `plan_log.usage.is_some()`
- `llm_requests_edit` from attempt logs with `usage_this_call.is_some()`
- `llm_requests_total = plan + edit`

4. Updated `format_run_summary()` tokens line:
- now displays total + split request counts
- appends legacy annotation when planner usage is unknown for token-bearing runs

5. Verified token total logic remained correct:
- derived from `usage_cumulative` in the final attempt log, which includes planner usage via `RunState.usage` accumulation in `run()`.

### Tests

Updated existing stats tests for renamed/new fields.

Added:
- `summarize_run_counts_plan_request`
- `summarize_run_legacy_plan_no_usage`

### Verification

- `cargo test`: **159 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## Task 4: Update token display tests

### Changes

Updated `RunSummary` test literals in `src/stats.rs` to match new fields:
- `attempts_per_step`
- `llm_requests_total`
- `llm_requests_plan`
- `llm_requests_edit`

Added:
- `format_run_summary_legacy_plan_annotation`

This validates legacy note rendering when planner usage is unavailable and tokens are non-zero.

### Verification

- `cargo test`: **160 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## Task 5: AGENTS.md update

### Changes

Updated `AGENTS.md` to reflect completion state:

1. Baseline:
- from `154 passing` to `160 passing` (`1 ignored`)

2. Phase status:
- `Phases 1–10 complete, Phase 11 next` -> `Phases 1–11 complete`
- Phase 11 table row status -> `✅ Done`

3. Runtime output docs:
- `plan.json` description now notes usage data inclusion from Phase 11+

4. Architectural invariants:
- added planner+editor request accounting invariant
- noted graceful handling for legacy logs without planner usage

### Verification

- `cargo test`: **160 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## 4. Final outcome

Phase 11 is complete and validated.

Delivered guarantees:

1. Resume does not issue additional LLM calls once token budget is exhausted (`>=` guard).
2. Planner usage is persisted in `plan.json` for accounting.
3. Stats request counts now explicitly separate planner and editor calls.
4. Legacy logs remain readable/compatible via defaulted optional usage fields.
5. User-facing status/docs now reflect Phase 11 completion and accounting semantics.

Final validation state:
- `cargo test`: **160 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
