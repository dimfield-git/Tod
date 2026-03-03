# PHASE11.md — Reliability Accounting

**Read AGENTS.md first.** All operating principles, coding standards, and safety rules apply.

---

## Goal

Close correctness and accounting gaps that can produce misleading run state or metrics. After Phase 10, Tod is externally usable — but resume can issue LLM calls when the token budget is already exhausted, planner usage is invisible to stats, and request counts undercount. Phase 11 fixes all three so that operators can trust the numbers.

Five tasks in order. Task 1 is a targeted safety guard. Tasks 2–3 are the accounting core. Task 4 is a low-risk rename that piggybacks on the stats changes. Task 5 is the display update.

---

## Baseline

- `cargo test`: 154 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean
- Phases 1–10 complete

---

## Task 1: Pre-resume token cap guard

### What

`resume()` in `loop.rs` (lines 643–668) loads the checkpoint and checks the fingerprint, then calls `run_from_state()` directly. If the checkpoint was written at or above the token cap, this issues at least one more LLM call before the in-loop token check at line 553 can fire.

Add a token cap guard between the fingerprint check (line 664) and the `run_from_state()` call (line 667). Use the existing `LoopError::TokenCapExceeded { used, cap }` variant — no new error variant needed.

### Design decisions

- Threshold: `>=` (not `>`). The in-loop checks at lines 495 and 553 use `>`, which means a run can end cleanly with `usage.total() == max_tokens`. On resume, that means zero budget remains, so `>=` is correct.
- Message: the `TokenCapExceeded` Display (line 424) already says `"token budget exceeded: used {used} tokens, cap was {cap}"`. This is sufficient — the user sees the numbers and understands no work was done.
- Only guard when `max_tokens > 0` (matching the existing pattern at lines 495 and 553).

### Changes

In `resume()` (`src/loop.rs`), after `state.fingerprint = current;` (line 664) and before `run_from_state(provider, config, &mut state)` (line 667), add:

```rust
if state.max_tokens > 0 && state.usage.total() >= state.max_tokens {
    return Err(LoopError::TokenCapExceeded {
        used: state.usage.total(),
        cap: state.max_tokens,
    });
}
```

### Tests

Add to `src/loop.rs` tests:

| Test | Setup | Assertion |
|------|-------|-----------|
| `resume_at_token_cap_returns_error` | Write a checkpoint with `usage` at cap (`max_tokens: 100`, `usage.input_tokens: 60, usage.output_tokens: 40`). Call `resume()`. | Returns `LoopError::TokenCapExceeded { used: 100, cap: 100 }`. Provider is never called (use empty `QueueProvider`). |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **155 passing, 1 ignored.**

**Codex reasoning level: medium**

**Do not start Task 2 until Task 1 is verified.**

---

## Task 2: Record planner usage in plan.json

### What

`PlanLog` (lines 140–147 of `loop.rs`) has no usage fields. Stats therefore cannot account for planner LLM costs. Extend `PlanLog` with an optional usage field and wire it through `write_plan_log()`.

### Changes

**2a: Extend `PlanLog` struct** (`src/loop.rs`, lines 140–147)

Add after the `plan` field:

```rust
#[serde(default)]
pub usage: Option<Usage>,
```

**2b: Update `write_plan_log()` signature** (`src/loop.rs`, lines 251–266)

Change from:

```rust
fn write_plan_log(&self, config: &RunConfig) {
```

To:

```rust
fn write_plan_log(&self, config: &RunConfig, usage: Option<Usage>) {
```

Inside the function, change the `PlanLog` construction to include:

```rust
usage,
```

**2c: Update call site in `run()`** (`src/loop.rs`, line 504)

Change from:

```rust
state.write_plan_log(config);
```

To:

```rust
state.write_plan_log(config, plan_usage.clone());
```

Note: `plan_usage` is already in scope from line 488 as `Option<Usage>`. The `clone()` is needed because `plan_usage` was borrowed on line 491.

### Backward compatibility

Old `plan.json` files lack the `usage` field. The `#[serde(default)]` attribute ensures `usage` deserializes as `None` from legacy logs. No migration needed.

### Tests

Add to `src/loop.rs` tests:

| Test | Setup | Assertion |
|------|-------|-----------|
| `plan_log_includes_usage` | Use `UsageProvider` that returns usage with the plan response. Run a dry-run. Read `plan.json` from `.tod/logs/`. | `plan.json` contains `"usage"` key with `input_tokens` and `output_tokens` matching the provided values. |
| `plan_log_without_usage_deserializes` | Write a `plan.json` without a `usage` field (legacy format). Deserialize as `PlanLog`. | Succeeds with `usage: None`. |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **157 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 3 until Task 2 is verified.**

---

## Task 3: Fix stats request accounting

### What

`summarize_run()` in `stats.rs` (lines 233–236) counts `llm_requests` as edit calls with non-None `usage_this_call`. This undercounts because: (a) the planner call is not represented in attempt logs, and (b) any edit call that returns `usage: None` is excluded.

After Task 2, `plan.json` carries optional usage. Stats should read it and include the planner call in request counts.

Additionally, `RunSummary.retries_per_step` (line 35 of `stats.rs`) is semantically misnamed — it stores attempt counts, not retry counts. Rename it while we're modifying the struct.

### Changes

**3a: Rename `retries_per_step` → `attempts_per_step`** (`src/stats.rs`)

In `RunSummary` (line 35):

```rust
pub attempts_per_step: Vec<usize>,
```

Update all references in `stats.rs`:
- Construction in `summarize_run()` (~line 210–213): `retries_per_step` → `attempts_per_step`
- `format_run_summary()` (lines 356–362): the `retries_per_step` iterator → `attempts_per_step`
- `format_run_summary()` (line 390): `retries_per_step.len()` → `attempts_per_step.len()`
- All test assertions referencing `retries_per_step` → `attempts_per_step`

**3b: Split request counts** (`src/stats.rs`)

Replace `llm_requests: u64` (line 40) in `RunSummary` with:

```rust
pub llm_requests_total: u64,
pub llm_requests_plan: u64,
pub llm_requests_edit: u64,
```

In `summarize_run()`, after reading the plan log (line 130), compute `llm_requests_plan`:

```rust
let llm_requests_plan: u64 = if plan_log.usage.is_some() { 1 } else { 0 };
```

Keep the existing edit request count logic (lines 233–236) but assign to `llm_requests_edit`:

```rust
let llm_requests_edit = attempt_logs
    .iter()
    .filter(|log| log.usage_this_call.is_some())
    .count() as u64;
```

Compute total:

```rust
let llm_requests_total = llm_requests_plan + llm_requests_edit;
```

Update the `RunSummary` construction to use all three fields.

**3c: Include planner usage in token totals** (`src/stats.rs`)

Currently token totals come from the last attempt log's `usage_cumulative` (lines 226–232). This already includes planner usage because `RunState.usage` accumulates it in `run()` (line 492). So token totals are already correct — no change needed for `input_tokens`, `output_tokens`, `total_tokens`.

Verify this assumption by checking that `usage_cumulative` in the last attempt log reflects the planner call. If it does not (because `usage_cumulative` is set from `state.usage` which accumulates planner usage at line 492), flag as a discrepancy.

**3d: Update `format_run_summary()` display** (`src/stats.rs`)

Change the tokens line from:

```
Tokens:     {in} in / {out} out ({requests} requests)
```

To:

```
Tokens:     {in} in / {out} out ({total} requests: {plan} plan, {edit} edit)
```

When `llm_requests_plan == 0` and `total_tokens > 0`, append `" (planner usage unknown — legacy logs)"` to the tokens line.

**3e: Update `MultiRunSummary` and `summarize_runs()`** if needed

`summarize_runs()` does not currently aggregate `llm_requests` into `MultiRunSummary`. If it doesn't reference the field, no change is needed there. Verify and leave unchanged if so.

### Tests

Update existing tests that reference `retries_per_step` to use `attempts_per_step`.

Update existing tests that reference `llm_requests` to use the new split fields.

Add to `src/stats.rs` tests:

| Test | Setup | Assertion |
|------|-------|-----------|
| `summarize_run_counts_plan_request` | Write `plan.json` with a `usage` field. Write one attempt with `usage_this_call`. | `llm_requests_plan == 1`, `llm_requests_edit == 1`, `llm_requests_total == 2` |
| `summarize_run_legacy_plan_no_usage` | Write `plan.json` without a `usage` field (legacy). Write one attempt with `usage_this_call`. | `llm_requests_plan == 0`, `llm_requests_edit == 1`, `llm_requests_total == 1` |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **159 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 4 until Task 3 is verified.**

---

## Task 4: Update `format_run_summary` token display tests

### What

The `format_run_summary_shows_tokens` and `format_run_summary_hides_zero_tokens` tests in `stats.rs` (lines 699–736) construct `RunSummary` literals. After Task 3, the struct has new fields (`llm_requests_total`, `llm_requests_plan`, `llm_requests_edit`, `attempts_per_step`). These tests must compile with the updated struct.

Also verify that the legacy annotation logic works: when `llm_requests_plan == 0` and `total_tokens > 0`, the display includes the legacy warning.

### Changes

Update the two existing test functions to use the new field names. Add a test:

| Test | Setup | Assertion |
|------|-------|-----------|
| `format_run_summary_legacy_plan_annotation` | Construct `RunSummary` with `llm_requests_plan: 0`, `total_tokens: 1000`. | Rendered output contains `"legacy"`. |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Expected: **160 passing, 1 ignored.**

**Codex reasoning level: low**

**Do not start Task 5 until Task 4 is verified.**

---

## Task 5: AGENTS.md update

### What

Update `AGENTS.md` to reflect Phase 11 completion.

### Changes

- **Baseline:** Update `154 passing` → final count (expected ≥160), `1 ignored`
- **Phase table:** Phase 10 remains `✅ Done`. Add Phase 11 row:
  - Scope: "Reliability accounting — pre-resume token cap guard, planner usage in plan.json, stats request count fix, field rename"
  - Status: `✅ Done`
- **Phases complete:** `Phases 1–10 complete` → `Phases 1–11 complete`
- **Priority order:** Change `ROADMAP.md` → `PHASE11.md` if the current instructions line still references `ROADMAP.md`, or leave as-is if it already points to the current phase file. (Check before editing.)
- **Architectural invariants:** Add: "Stats request counts reflect all billed API calls (planner + editor). Planner usage is recorded in `plan.json`. Legacy logs without planner usage are handled gracefully."
- **Runtime output directory:** Update `plan.json` description from "Written once after planning" to "Written once after planning (includes usage data from Phase 11+)"

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
| 1. Pre-resume token cap guard | Safety guard | `loop.rs` | Medium |
| 2. Planner usage in plan.json | Schema extension | `loop.rs` | High |
| 3. Stats request accounting | Stats rewrite | `stats.rs` | High |
| 4. Display test updates | Test maintenance | `stats.rs` | Low |
| 5. AGENTS.md update | Documentation | `AGENTS.md` | Low |

**Do not start a later task until the preceding task is verified passing.**
