# Module State Review (2026-03-03, Post-Phase 16)

Date: 2026-03-03
Scope: full module-by-module review of `src/` against current behavior
Reviewer posture: senior software engineering + applied agent reliability

Validation baseline:
- `cargo test`: **203 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

## Executive Summary

Tod’s module health is solid overall:
- Safety and compatibility invariants remain intact.
- Phase 16 delivered operator-facing usability gains without destabilizing runtime logic.
- The main structural risk remains concentrated in `src/loop.rs` size/centrality.

## Snapshot Table

| Module | LOC | Test count | State | Notes |
|---|---:|---:|---|---|
| `src/main.rs` | 237 | 4 | Stable | Dispatch now supports JSON output routing for status/stats. |
| `src/cli.rs` | 292 | 10 | Stable | `status`/`stats` now include `--json`; parsing coverage present. |
| `src/config.rs` | 91 | 2 | Stable | Immutable config contract remains clean. |
| `src/context.rs` | 470 | 9 | Stable | Budgeted context assembly remains deterministic. |
| `src/planner.rs` | 274 | 11 | Stable | Strict semantic plan validation remains a strength. |
| `src/editor.rs` | 220 | 8 | Stable | Typed bridge from model output to schema-validated edits. |
| `src/schema.rs` | 747 | 28 | Strong | Core safety boundary; broad regression coverage. |
| `src/runner.rs` | 645 | 19 | Strong | Transactional apply and deterministic pipeline staging. |
| `src/reviewer.rs` | 167 | 9 | Stable | Small deterministic retry/abort policy layer. |
| `src/llm.rs` | 425 | 15 | Stable | Provider abstraction + retry semantics are bounded. |
| `src/log_schema.rs` | 99 | 1 | Stable | Pure schema/serde module boundary preserved. |
| `src/loop_io.rs` | 185 | 4 | Stable | Persistence and run-id boundary remains clean. |
| `src/loop.rs` | 2561 | 56 | Watch | Robust, heavily tested, but still the primary complexity hotspot. |
| `src/stats.rs` | 1269 | 25 | Stable+ | Now supports human + JSON output with compatibility-aware summarization. |
| `src/util.rs` | 49 | 3 | Stable | Focused utility helpers, low risk. |
| `src/test_util.rs` | 43 | 0 | Stable | Temp sandbox helper remains fit-for-purpose. |

---

## Phase-16 Delta by Module

### `src/loop.rs`
- Added dirty-workspace pre-run helper (`git status --porcelain`) with non-blocking warning behavior.
- Added pure cap-check helpers (`check_iteration_cap`, `check_token_cap`).
- Added focused regression tests for dirty-workspace and cap-helper logic.
- Net: better maintainability and workflow safety with preserved orchestration semantics.

### `src/cli.rs`
- Added `--json` flags for `status` and `stats` command variants.
- Updated parser tests for new command surface.

### `src/main.rs`
- Added dispatch branching for human vs JSON stats/status formatting.

### `src/stats.rs`
- Added compact JSON formatter functions for single-run and multi-run summaries.
- Added tests validating JSON parseability and key presence.

### Docs/runtime surface
- Added operator runbook (`docs/runbook.md`) and Phase 16 implementation log.

---

## Module-by-Module Findings

## `src/main.rs`
State: stable.
- Concern remains low; entrypoint behavior is explicit and test-backed.

## `src/cli.rs`
State: stable.
- CLI surface is growing but still coherent; conversion logic remains predictable.

## `src/config.rs`
State: stable.
- No immediate risks; keep immutable configuration contract unchanged.

## `src/context.rs`
State: stable.
- Deterministic, budget-aware context remains good.
- Future candidate: relevance improvements for larger repos.

## `src/planner.rs`
State: stable.
- Validation strictness should be preserved.

## `src/editor.rs`
State: stable.
- Error typing and schema validation boundary remain sound.

## `src/schema.rs`
State: strong.
- Continues to be the core execution guardrail.

## `src/runner.rs`
State: strong.
- Transactional semantics and static runner stages are appropriate.

## `src/reviewer.rs`
State: stable.
- Simple policy remains robust and easy to reason about.

## `src/llm.rs`
State: stable.
- Retry accounting model remains coherent.
- Single-provider limitation remains strategic, not immediate correctness risk.

## `src/log_schema.rs`
State: stable.
- Boundaries are clean and should stay pure data/serde.

## `src/loop_io.rs`
State: stable.
- Best-effort write semantics and atomic checkpoint pattern remain intact.

## `src/loop.rs`
State: watch.
- Test coverage is strong.
- Maintenance risk remains due to combined orchestration responsibilities.
- Continue incremental extraction of pure decision logic in future phases.

## `src/stats.rs`
State: stable+.
- JSON output enables better machine consumption.
- Next step should be contract hardening and richer observability, not broad redesign.

## `src/util.rs`, `src/test_util.rs`
State: stable.
- Keep minimal.

---

## Risk Register (Current)

1. `loop.rs` centrality and size
- Severity: high
- Mitigation: continue behavior-preserving pure extraction and targeted tests.

2. Context/edit scaling in larger codebases
- Severity: medium-high
- Mitigation: improve context relevance and observability before major edit-contract expansion.

3. Provider monoculture
- Severity: medium
- Mitigation: defer until orchestration and telemetry surfaces are further stabilized.

---

## Recommended Phase-17 Module Focus

1. `src/loop.rs`: additional pure-helper extraction for terminal-path decision points.
2. `src/stats.rs`: strengthen machine-readable contract stability and coverage.
3. `src/loop_io.rs` + `src/log_schema.rs`: preserve compatibility guarantees while improving observability write-path clarity.
4. Reserve explicit integration points for UX requirements that will be provided later.
