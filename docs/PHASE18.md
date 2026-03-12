# PHASE18.md — Alpha Integrity + Operator Control

Read `AGENTS.md` first. All operating principles and safety rules apply.

**Implementation order: Tasks must be executed in sequence (1 → 2 → 3). Stop after Task 3 for validation checkpoint. Tasks 4 → 5 → 6 → 7 may proceed after checkpoint is acknowledged. Do not start Task 6 before Task 5 is complete (`--quiet` must exist before contract-testing it).**

---

## Goal

Get Tod from "tests pass" to "usable alpha that produces trustworthy output on real Rust projects." Fix known correctness bugs in accounting, reduce error-path maintenance risk, eliminate code duplication, then harden operator control and output contracts.

Primary outcomes:
- Request/token accounting is correct under all terminal paths, including plan-error.
- Error-path teardown is consolidated into a single helper.
- Duplicate code is eliminated.
- Failure surfaces provide precise run-level log pointers.
- Operators can suppress lifecycle chatter via `--quiet`.
- Command-level stdout/stderr contracts are test-protected.

---

## Why This Phase Now

Phase 17 added lifecycle messaging, actionable errors, and enriched completion output. Code review identified two accounting bugs that make those new surfaces untrustworthy: the plan-error path silently drops request counts, and the pre-increment timing inflates counts on pre-contact failures. These must be fixed before validation runs can produce meaningful results.

The error-path boilerplate and code duplication are maintenance risks that compound with each new phase. Fixing them now reduces the cost of every future change.

The remaining operator control and contract work (precise log pointers, `--quiet`, output contract tests) hardens the surfaces Phase 17 introduced and prepares Tod for use beyond the developer's own machine.

---

## Design Decisions (Locked)

1. Preserve existing safety/compatibility behavior unless a task explicitly changes semantics.
2. Keep `log_schema.rs` pure data+serde and `loop_io.rs` persistence/identity boundary.
3. No patch-mode/provider expansion/git worktree orchestration in this phase.
4. Stdout remains reserved for command output and JSON payloads.
5. `--quiet` only controls cosmetic lifecycle progress messages; it must not suppress errors.
6. Request-count semantic (locked): `llm_requests` counts **observed provider responses** — calls where the provider was contacted and returned a response (success or API error). A request sent upstream but lost before any response arrives is not counted. This is not "billed requests" and not "attempted requests"; it is "responses observed." Transport-level retries within a single logical call are not counted.
7. `terminate_run` helper signature is guidance, not gospel — adapt to borrow-checker realities, but preserve the consolidation intent.

---

## Baseline (Start of Phase 18)

- `cargo test`: **215 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1–17 complete

---

## Task 1: Accounting Integrity (Bug Fixes + Tests)

**Stop after this task. Do not continue unless tests pass.**

### Problem

Two concrete bugs identified during code review:

1. **Plan-error request gap.** When `create_plan` fails, no `RunState` is created, so the plan LLM request is never counted. The `final.json` artifact records `plan_error` with zero requests and zero tokens — even when the provider was contacted and returned a response.

2. **Pre-increment timing.** `llm_requests` increments before `create_edits` is called. If the provider call fails before sending, the count is inflated by one.

### Fix

**1a.** In the plan-error path, if the provider returned a response (success or error with parseable body), capture usage from that response and record `llm_requests: 1` in the `final.json` artifact. If the call failed before any provider response was observed (transport failure, timeout, no body), record `llm_requests: 0`. The metric must reflect what was observed, not what may have been billed upstream.

**1b.** Move `state.llm_requests += 1` to after the `create_edits` call returns (success or LLM-level failure), so it counts calls where the provider was actually contacted. Accumulate usage in the same location.

**1c.** Add a doc comment on `RunState::llm_requests` defining the semantic: "Observed provider responses — LLM provider calls where a response was received (success or API error). Requests lost in transit before any response are not counted. Transport-level retries within a single logical call are not counted."

### Tests (all mandatory)

- Plan-error path writes `llm_requests >= 1` in the artifact.
- Planner failure before `RunState` creation still records the request in the artifact. This is the core of the plan-error accounting bug — it must be proven, not assumed.
- Edit-generation failure **after** LLM response (provider contacted, response received) counts the request.
- Edit-generation failure **before** LLM contact (provider never reached) does not count a request. Requires a mock provider that errors without calling.
- Assert using `contains`/field assertions, not fragile full-string matching.

### Constraints

- Keep request-count semantics consistent with `AGENTS.md` and Design Decision 6.
- No changes to run outcomes, artifact shape, or checkpoint timing beyond the accounting fix.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 2: Error-Path Boilerplate Extraction (`terminate_run`)

**Stop after this task. Do not continue unless tests pass.**

### Problem

`run_from_state` has six early-return error paths that each repeat the same ceremony: `refresh_fingerprint → checkpoint → write_final_log → return Err`. That's ~50 lines of duplicated teardown logic. Each new error path requires copying all four steps correctly.

### Fix

Extract a helper (adapt signature as needed for borrow-checker compatibility):

```rust
fn terminate_run(
    state: &mut RunState,
    config: &RunConfig,
    err: LoopError,
) -> LoopError {
    state.refresh_fingerprint(&config.project_root);
    state.checkpoint(config);
    if let Some(outcome) = terminal_outcome_for_error(&err) {
        state.write_final_log(
            config,
            outcome,
            Some(state.step_index),
            final_attempt_for_attempt_counter(state.step_state.attempt),
            Some(err.to_string()),
        );
    }
    err
}
```

Replace each raw teardown sequence with: `return Err(terminate_run(state, config, err));`

### Tests

- All existing tests still pass (behavior-preserving refactor).
- Grep confirms no remaining raw `refresh_fingerprint → checkpoint → write_final_log` sequences outside the helper.

### Constraints

- This is a pure refactor. No semantic changes to run outcomes, artifacts, or checkpoint behavior.
- `terminate_run` must stay narrow: refresh fingerprint, checkpoint, final-log write, return error. No policy decisions, no message formatting, no extra branching. If it starts growing, you are moving `loop.rs` complexity sideways instead of reducing it.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 3: `run_mode_label` Dedup + Phase Docs Checkpoint

**Stop after this task. Do not continue unless tests pass.**

### Problem

`run_mode_label` is defined identically in both `main.rs` and `loop.rs`. This is a Codex artifact from task-by-task implementation without a global dedup pass.

### Fix

Keep one copy in a shared location (`util.rs` or `config.rs` — whichever makes sense given that it operates on `RunMode`) and import it at both call sites.

### Tests

- `cargo build` confirms no duplicate symbol issues.
- Grep confirms single definition.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## ⏸ VALIDATION CHECKPOINT

**After Task 3, stop and wait for acknowledgement before proceeding.**

Tasks 1–3 fix the known correctness and maintainability issues required for trustworthy alpha validation runs. The operator will run Tod against real projects at this point (see `tod-alpha-plan.md` Track 4 test matrix).

**Acceptance rule for validation runs:** For every failed run, the operator must be able to answer three questions without rerunning: (1) what happened, (2) where it happened, (3) whether accounting and output were truthful. If the logs and CLI output cannot answer all three, the core loop is not yet trustworthy and Tasks 4–7 should not proceed until the gap is addressed.

Tasks 4–7 may proceed only after the validation evidence confirms the core loop is sound.

---

## Task 4: Precise Failure Log Pointers

**Stop after this task. Do not continue unless tests pass.**

### What

Improve operator recovery speed by surfacing the exact run-level log directory on failure, not just the generic `.tod/logs/` pointer.

### Scope

- Enrich error-path reporting so `run`/`resume` failures include the specific `run_id` log location (e.g., `.tod/logs/run_003/`) when available.
- Preserve typed errors; do not introduce global mutable state.
- When `run_id` is not yet allocated (pre-plan failures), fall back to the generic `.tod/logs/` pointer.

### Constraints

- Do not change exit codes.
- Maintain compatibility defaults for legacy checkpoints/artifacts.
- Derive the log pointer from state/report context at the CLI boundary (`main.rs`). Do not thread `run_id` into `LoopError` variants — error types should remain about failure semantics; the CLI boundary enriches presentation.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 5: Output Policy Control (`--quiet`)

**Stop after this task. Do not continue unless tests pass.**

### What

Add operator control over lifecycle progress verbosity.

### Scope

- Add `--quiet` flag for `run` and `resume` commands.
- When enabled, suppress lifecycle progress banners/messages (startup, plan-ready, step-entry, attempt, review-outcome, resume confirmation).
- Keep stderr errors and stdout command output unchanged.
- Thread the quiet flag through `RunConfig` (or equivalent) to the emission sites in `loop.rs` and `main.rs`.

### Constraints

- `--quiet` is cosmetic only; it must never affect control flow, return values, or exit codes.
- Errors must always print regardless of `--quiet`.
- `--quiet` is output/emission policy, not execution policy. Prefer passing it as an emission flag to the sites that print lifecycle messages (e.g., a `quiet: bool` on `RunConfig` or a dedicated emission context) rather than widening core domain structs with output concerns.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 6: Command-Level Output Contract Tests

**Stop after this task. Do not continue unless tests pass.**

### What

Protect stdout/stderr behavior from regression in both human and JSON command paths.

### Scope

- Add integration-style tests for:
  - `status` human vs `--json` output shape
  - `stats` human vs `--json` output shape
  - lifecycle message suppression under `--quiet`
  - error output behavior (stderr guidance present, stdout clean)
- Assert stable keys/fragments using `contains`-style checks, not brittle exact-whitespace snapshots.

### Constraints

- Do not duplicate existing contract tests in `stats.rs`; extend coverage to command-dispatch boundaries.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 7: Documentation and Phase Closure

**Stop after this task. Do not continue unless tests pass.**

### Scope

- Update `AGENTS.md`:
  - Phase 18 status → Done
  - Update baseline test count
  - Add Phase 18 outcomes
  - Add `--quiet` behavior to workflow safety invariants
  - Add request-count doc-comment semantic to request counting section
- Update `docs/runbook.md`:
  - Add `--quiet` usage guidance
  - Update failure pointer examples with precise log paths
  - Add output contract notes
- Update `README.md`:
  - Status → Phases 1–18 complete
  - Refresh baseline test count
  - Document `--quiet` flag
- Write `docs/phase18-implementation-log-<date>.md` with task-by-task verification timeline.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Out of Scope (Phase 19+)

- Patch/diff edit contract.
- Multi-provider expansion.
- Git worktree orchestration engine.
- Async runtime migration.
- Major reviewer-policy redesign.
- Alpha validation test matrix (manual, operator-driven — see `tod-alpha-plan.md` Track 4).
