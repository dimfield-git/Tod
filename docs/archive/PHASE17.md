# PHASE17.md - Observability, Orchestration Maintainability, and Operator UX

Read `AGENTS.md` first. All operating principles and safety rules apply.

**Implementation order: Tasks must be executed in sequence (1, 2, 3, 4, 5, 6). Do not start Task 4 before Task 1 contract tests are in place. Do not start Task 3 before Task 2 extractions are complete.**

---

## Goal

Phase 16 delivered workflow safety and machine-readable stats. Phase 17 strengthens operational confidence, continues orchestration decomposition, and makes Tod communicative to the operator during runs.

Primary outcomes:
- Stronger observability contracts and compatibility confidence.
- Smaller orchestration change blast radius in `loop.rs`.
- Operator-visible lifecycle messaging: Tod tells you what it's doing, not just what it did.
- Actionable error guidance and enriched completion output.
- Preserved compatibility and safety invariants.

---

## Why This Phase Now

Tod's internals are well-engineered but none of that quality is visible to the operator. Every run currently feels like a black box: launch, wait, get a terse result. Two categories of work are ready:

**Technical:** `loop.rs` remains the largest concentration point. Observability contracts (`--json`) exist but need stability guarantees for tooling trust.

**UX:** The operator has zero visibility into what is happening between launch and completion. Error messages explain what happened but never what to do. Success output is a single line with no context. These are high-impact, low-risk improvements that do not touch core safety invariants.

This phase blends both.

---

## Design Decisions (Locked)

1. Preserve behavior unless a task explicitly introduces a user-visible change.
2. Do not weaken path safety, transactional apply semantics, or compatibility defaults.
3. Keep `log_schema.rs` as pure data+serde and `loop_io.rs` as best-effort persistence/identity boundary.
4. Prefer pure/helper extraction and test hardening over large rewrites.
5. Do not begin patch-mode, provider expansion, or git worktree orchestration in this phase.
6. All operator-facing messages go to stderr via `eprintln!`. Stdout remains clean for piping and `--json` output.
7. No `--quiet` flag this phase. If needed later, it can gate the stderr messages added here.
8. Lifecycle messages are best-effort cosmetic output. They must never affect control flow, return values, or exit codes.

---

## Baseline (Start of Phase 17)

- `cargo test`: **203 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1-16 complete

---

## Task 1: Observability Contract Hardening and Compatibility Audit

### What
Strengthen stability of output contracts and increase confidence that modern and legacy artifacts remain readable and accurately summarized.

### Scope
- Add or tighten tests that protect key field presence and naming for `status --json` and `stats --json`.
- Add tests/fixtures that protect non-JSON human-readable formatting from accidental drift where stability is expected.
- Add targeted compatibility tests for legacy/defaulted fields where risk is non-trivial.
- Audit summaries for edge outcomes (`plan_error`, `token_cap`, `cap_reached`, `aborted`) under both human and JSON formatting paths.
- Document output contract expectations in `docs/runbook.md` (append a "Machine-Readable Output" section).

### Constraints
- No breaking changes to existing JSON field names introduced in Phase 16.
- Legacy artifact deserialization behavior remains unchanged.
- Compatibility-first: preserve existing deserialization defaults and fallback behavior.

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 2: Orchestration Surface Reduction (Behavior-Preserving)

### What
Continue extracting small pure decision helpers from `loop.rs` to reduce local complexity while preserving control flow and artifact semantics.

### Scope
- Identify finalization/retry-adjacent decision logic that can become pure helper functions (e.g., terminal outcome mapping, step-advance logic).
- Keep checkpoint writes, final-log writes, and return paths in the orchestration flow where side effects occur.
- Add focused table tests for extracted decision logic.

### Constraints
- No semantic change to terminal outcomes, checkpoint timing, or artifact paths.
- No new runtime dependencies.
- Existing tests must pass unchanged.

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 3: Run Lifecycle Messaging

### What
Make Tod communicative during runs. Add stderr progress messages at key orchestration transitions so the operator knows what is happening between launch and completion.

### Design

**Startup banner** -- print in `main.rs` immediately before calling `loop::run()`:
```
tod: running in <mode> mode on <project_root> (max <N> iters/step, <token_cap_description>)
```
Where `<token_cap_description>` is `"no token cap"` when `max_tokens == 0`, or `"max <N> tokens"` otherwise. Use `eprintln!`.

Do not print the banner for `--dry-run` -- instead print:
```
tod: dry-run mode on <project_root> (no filesystem writes)
```

**Plan created** -- print in `loop::run()` after successful plan creation:
```
tod: plan ready -- <N> step(s)
```

**Step entry** -- print in `run_from_state()` at the top of the outer while loop:
```
tod: step <X>/<total>: <step_description>
```
Truncate `step_description` to 80 characters if longer, append `"..."`. Truncation must be UTF-8 safe: use existing `util.rs` preview helper patterns, not byte slicing.

**Attempt start** -- print in `run_from_state()` after `step_state.attempt` is incremented:
```
tod: step <X>/<total>: attempt <Y>/<max>
```

**Review outcome** -- print after each review decision:
- Proceed: `tod: step <X>/<total>: attempt <Y> -- passed`
- Retry: `tod: step <X>/<total>: attempt <Y> -- retrying (<stage> failed)`
- Abort: `tod: step <X>/<total>: attempt <Y> -- aborted`

**Resume confirmation** -- print in `loop::resume()` after loading checkpoint and before entering `run_from_state()`:
```
tod: resuming "<goal>" from step <X>/<total> (<mode> mode, dry-run: <on|off>)
```
Truncate goal to 60 characters if longer, append `"..."`. Same UTF-8 safe truncation rule.

### Constraints
- All messages go to stderr via `eprintln!`. Stdout is untouched.
- Messages are cosmetic: they must not affect control flow, return values, exit codes, or test assertions.
- Do not add a `--quiet` flag this phase.
- Do not modify stdout output in Task 3. Stdout changes, if any, happen only in Task 4.
- Step descriptions come from `plan.steps[index].description` (already available on `step` in the loop).
- For resume, goal and mode come from the loaded `RunState` and effective config (already computed).

### Tests
- Lifecycle messages are stderr-only and cosmetic. Do not add assertion tests for message content (these are fragile and low-value).
- Verify that all existing tests pass unchanged. The messages must not interfere with any existing behavior.

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 4: Actionable Errors and Enriched Completion Output

### What
Make error messages guide the operator toward a next action. Enrich success output with what changed and where to look.

### Design

**Actionable error guidance** -- update the `LoopError::Display` impl in `loop.rs` to append a guidance line to each variant:

| Variant | Current message (unchanged) | Appended guidance |
|---|---|---|
| `TotalIterationCap` | `"reached total iteration cap: N"` | `" -- try increasing --max-iters or narrowing the goal"` |
| `TokenCapExceeded` | `"token budget exceeded: used N tokens, cap was M"` | `" -- try increasing --max-tokens or reducing step count"` |
| `Aborted` | `"run aborted at step N: reason"` | `" -- check .tod/logs/ for failure details"` |
| `Plan` | `"plan failed: ..."` | `" -- check goal phrasing and project structure"` |
| `Edit` | `"edit creation failed ..."` | `" -- review the attempt log in .tod/logs/"` |
| `Apply` | `"edit application failed ..."` | `" -- workspace may need manual inspection; check .tod/logs/"` |
| `FingerprintMismatch` | `"workspace has changed ..."` | (already has `"use --force to override"` -- no change) |
| `NoCheckpoint` | `"no .tod/state.json found ..."` | (self-explanatory -- no change) |
| `Io` | `"I/O error ..."` | (context-dependent -- no change) |
| `InvalidPlanPath` | `"invalid plan path ..."` | (self-explanatory -- no change) |

The guidance is part of the `Display` output, so it appears everywhere the error is printed (including `main.rs`'s `eprintln!("run failed: {e}")`).

**Log path on failure** -- in `main.rs`, after printing the error for `run` and `resume` commands, print the log directory path to stderr via `eprintln!`:
```
tod: logs at .tod/logs/
```
This is a static pointer since the run_id is not available in `main.rs` after an error. Do not thread `run_id` through error types in this phase; keep the failure log pointer coarse.

**Enriched success output** -- update the success `println!` in `main.rs` for both `run` and `resume` to include token usage and log pointer. This requires `LoopReport` to carry additional data.

Extend `LoopReport` in `loop.rs`:
```rust
pub struct LoopReport {
    pub steps_completed: usize,
    pub total_iterations: usize,
    pub input_tokens: u64,    // new
    pub output_tokens: u64,   // new
    pub llm_requests: u64,    // new
    pub log_dir: String,      // new
}
```

Populate `LoopReport.{input_tokens, output_tokens, llm_requests}` from the run-level accumulated usage counters already tracked in `RunState` (`state.usage` and `state.llm_requests`), not per-attempt values. Populate `LoopReport.log_dir` from `state.log_dir`. Keep request-count semantics as defined in AGENTS.md (retries do not increment; one plan call = 1 request, one edit call = 1 request).

Update `RunState::report()` to populate these fields.

Update `main.rs` success output to:
```
completed <N> step(s) in <M> iteration(s)
  tokens: <in> in / <out> out (<requests> requests)
  logs: <log_dir>/
```

If `input_tokens` and `output_tokens` are both 0 (dry-run with test provider), skip the tokens line.

### Constraints
- Error guidance is appended to `Display`, not a separate print: keeps it atomic.
- The extra log pointer line on failure prints to stderr via `eprintln!`, not stdout.
- `LoopReport` field additions are backward-compatible (internal struct, not serialized).
- Do not change exit codes.
- Do not thread `run_id` through error types in this phase.
- Prefer `contains()` assertions for error display tests; avoid exact string equality. If any existing test does exact equality on error messages, update it to use `contains()` with the original message fragment.

### Tests
- For each updated `LoopError` variant, add a test that the `Display` output `contains()` both the original message fragment and the new guidance fragment.
- For `LoopReport`, verify the new fields are populated correctly from a synthetic `RunState`.
- Existing error-handling tests must pass (update assertion style to `contains()` if needed).

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 5: CLI Help Enrichment

### What
Improve first-time operator experience by adding operational context to clap help attributes.

### Design

Update help text in `cli.rs` for these arguments:

| Argument | Current | New |
|---|---|---|
| `--max-iters` | (clap default) | `"Max fix iterations per plan step (total cap = this x 5)"` |
| `--strict` | `"Use strict mode (fmt + clippy + test)."` | `"Run fmt --check, clippy -D warnings, and test each attempt (slower, stricter)"` |
| `--dry-run` | (clap default) | `"Validate and log edits without writing to disk or running cargo"` |
| `--max-tokens` | (clap default) | `"Max total tokens (input + output) for the entire run. 0 = no limit"` |
| `--force` (resume) | (clap default) | `"Continue even if workspace has changed since last checkpoint"` |
| `--json` (status/stats) | (clap default) | `"Output as single-line JSON for tooling"` |

### Constraints
- Help text changes only. No behavior changes.
- Use clap `help = "..."` attribute, not `long_help`.

### Tests
- No new tests needed (help text is not behavior).
- Existing CLI parse tests must pass unchanged.

### Reasoning level
Low

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 6: Documentation and Phase Closure

### What
Align docs and phase status after implementation.

### Scope
- Update `AGENTS.md`:
  - Phase 17 status to `Done`.
  - Baseline to final test count.
  - Add Phase 17 Outcomes section.
  - Add Phase 18 Priority handoff section.
  - Add lifecycle messaging contract to Architectural Invariants (stderr-only, cosmetic, no control flow impact).
  - Update `LoopReport` documentation if project map or struct comments need alignment.
- Update `README.md`:
  - Status line to `Phases 1-17`.
  - Mention lifecycle progress output in usage section.
- Update `docs/runbook.md`:
  - Add any new guidance from Task 1 (machine-readable output section).
  - Reference actionable error guidance.
- Write `docs/phase17-implementation-log-<date>.md` with task-by-task log and verification timeline.

### Reasoning level
Low

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Out of Scope (Phase 18+)

- Patch/diff edit mode.
- Multi-provider expansion.
- Full git branch/worktree orchestration.
- Async runtime migration.
- Major reviewer-policy redesign.
- `--quiet` flag to suppress lifecycle messages.
- ANSI color output / `NO_COLOR` convention.
- Post-run file-change summary (which specific files were modified).
- Threading `run_id` through error types for precise failure log paths.
