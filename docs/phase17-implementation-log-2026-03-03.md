# Phase 17 Implementation Log (2026-03-03)

## Scope

Phase 17 closed five product-facing tracks:
1. Observability contract hardening and compatibility audit
2. Orchestration decision-surface reduction in `loop.rs`
3. Run lifecycle messaging on stderr
4. Actionable errors plus enriched completion output
5. CLI help enrichment

## Task-by-Task Log

## Task 1: Observability Contract Hardening + Compatibility Audit

Implemented:
- Added JSON contract-key stability tests for `status --json` and `stats --json` in `src/stats.rs`.
- Added human-format contract tests for run and multi-run summaries in `src/stats.rs`.
- Added edge-outcome coverage (`plan_error`, `token_cap`, `cap_reached`, `aborted`) across human and JSON formatting paths.
- Added compatibility regressions for legacy/defaulted log fields (missing runner stage, plan-error final without optional message).
- Appended a machine-readable output contract section to `docs/runbook.md`.

Files touched:
- `src/stats.rs`
- `docs/runbook.md`

Verification:
- `cargo test` -> `210 passed, 1 ignored, 0 failed`
- `cargo clippy -- -D warnings` -> clean

## Task 2: Orchestration Surface Reduction (Behavior-Preserving)

Implemented:
- Extracted pure helpers in `src/loop.rs`:
  - `review_handling(...)`
  - `terminal_outcome_for_error(...)`
  - `final_attempt_for_attempt_counter(...)`
  - `next_step_progress(...)`
- Updated orchestration flow to consume helpers while keeping side effects inline.
- Added table-style helper tests for outcome mapping, review handling, and step progression.

Files touched:
- `src/loop.rs`

Verification:
- `cargo test` -> `214 passed, 1 ignored, 0 failed`
- `cargo clippy -- -D warnings` -> clean

## Task 3: Run Lifecycle Messaging

Implemented:
- Added startup banner in `main.rs` for mutable runs and dry-run banner for `--dry-run`.
- Added stderr lifecycle messages in `loop.rs`:
  - plan-ready message
  - step-entry message with UTF-8-safe truncation
  - attempt-start message
  - review-outcome message (`passed`/`retrying`/`aborted`)
  - resume confirmation message with UTF-8-safe goal truncation
- Preserved stdout for command output/JSON payloads.

Files touched:
- `src/main.rs`
- `src/loop.rs`

Verification:
- `cargo test` -> `214 passed, 1 ignored, 0 failed`
- `cargo clippy -- -D warnings` -> clean

## Task 4: Actionable Errors + Enriched Completion Output

Implemented:
- Appended actionable guidance to `LoopError::Display` variants per Phase 17 spec.
- Extended `LoopReport` with:
  - `input_tokens`
  - `output_tokens`
  - `llm_requests`
  - `log_dir`
- Populated report usage/request fields from run-level `RunState` accumulators.
- Updated `main.rs` success output for `run`/`resume`:
  - base completion line
  - token/request line (hidden when in/out tokens are both zero)
  - logs line
- Added stderr failure pointer on `run`/`resume` errors:
  - `tod: logs at .tod/logs/`
- Added tests for actionable error display guidance (`contains` style) and report field population.

Files touched:
- `src/loop.rs`
- `src/main.rs`

Verification:
- `cargo test` -> `215 passed, 1 ignored, 0 failed`
- `cargo clippy -- -D warnings` -> clean

## Task 5: CLI Help Enrichment

Implemented:
- Updated clap `help` text in `src/cli.rs` for:
  - `--max-iters`
  - `--strict`
  - `--dry-run`
  - `--max-tokens`
  - `--force`
  - `--json` (`status`/`stats`)
- No behavior changes.

Files touched:
- `src/cli.rs`

Verification:
- `cargo test` -> `215 passed, 1 ignored, 0 failed`
- `cargo clippy -- -D warnings` -> clean

## Task 6: Docs + Phase Closure

Implemented:
- Updated `AGENTS.md`:
  - Phase 17 status -> Done
  - baseline test count -> `215 passed, 1 ignored`
  - added Phase 17 outcomes
  - added Phase 18 priority handoff
  - added lifecycle messaging invariant (stderr-only, cosmetic, no control-flow impact)
  - aligned `loop.rs` project-map description with LoopReport ownership
- Updated `README.md`:
  - status -> Phases 1-17 complete
  - baseline test count refreshed
  - documented lifecycle progress messaging behavior (stderr-only)
- Updated `docs/runbook.md`:
  - added actionable error output pointer notes
- Added this implementation log document.

Files touched:
- `AGENTS.md`
- `README.md`
- `docs/runbook.md`
- `docs/phase17-implementation-log-2026-03-03.md`

## Verification Timeline

1. After Task 1
   - `cargo test` passed (`210 passed, 1 ignored`)
   - `cargo clippy -- -D warnings` clean
2. After Task 2
   - `cargo test` passed (`214 passed, 1 ignored`)
   - `cargo clippy -- -D warnings` clean
3. After Task 3
   - `cargo test` passed (`214 passed, 1 ignored`)
   - `cargo clippy -- -D warnings` clean
4. After Task 4
   - `cargo test` passed (`215 passed, 1 ignored`)
   - `cargo clippy -- -D warnings` clean
5. After Task 5
   - `cargo test` passed (`215 passed, 1 ignored`)
   - `cargo clippy -- -D warnings` clean
6. After Task 6
   - `cargo test` passed (`215 passed, 1 ignored`)
   - `cargo clippy -- -D warnings` clean
