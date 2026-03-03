# Module State Review (2026-03-03, Post-Phase 17)

Date: 2026-03-03  
Scope: full module-by-module review of `src/` against current behavior  
Reviewer posture: senior software engineering + applied-agent reliability

Validation baseline:
- `cargo test`: **215 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

## Executive Summary

Module health is strong overall:
- Safety and compatibility invariants remain intact.
- Phase 17 materially improved operator-facing behavior and observability contracts.
- Structural risk remains concentrated in `src/loop.rs` (size + orchestration centrality).

## Snapshot Table

| Module | LOC | Test count | State | Notes |
|---|---:|---:|---|---|
| `src/main.rs` | 281 | 4 | Stable | Startup banners, enriched success output, failure log pointer, stdout/stderr contract separation. |
| `src/cli.rs` | 310 | 10 | Stable | Operational help text expanded; parse behavior unchanged and covered. |
| `src/config.rs` | 91 | 2 | Stable | Immutable config contract remains clean. |
| `src/context.rs` | 470 | 9 | Stable | Deterministic context building with budget enforcement remains intact. |
| `src/planner.rs` | 274 | 11 | Stable | Strict plan validation remains a strength. |
| `src/editor.rs` | 220 | 8 | Stable | Typed model->batch bridge remains constrained by schema validation. |
| `src/schema.rs` | 747 | 28 | Strong | Core safety boundary with broad regression coverage. |
| `src/runner.rs` | 645 | 19 | Strong | Transactional apply + deterministic pipeline staging remain robust. |
| `src/reviewer.rs` | 167 | 9 | Stable | Small deterministic proceed/retry/abort policy layer. |
| `src/llm.rs` | 425 | 15 | Stable | Provider abstraction and retry behavior remain bounded. |
| `src/log_schema.rs` | 99 | 1 | Stable | Pure schema/serde boundary preserved. |
| `src/loop_io.rs` | 185 | 4 | Stable | Persistence/run-id boundary remains clean and best-effort. |
| `src/loop.rs` | 2854 | 61 | Watch | High coverage and improved extraction, but still primary complexity hotspot. |
| `src/stats.rs` | 1530 | 32 | Stable+ | Strong output-contract and compatibility coverage; growing contract surface. |
| `src/util.rs` | 49 | 3 | Stable | Focused utility helpers, low change risk. |
| `src/test_util.rs` | 43 | 0 | Stable | Temp sandbox helper remains fit-for-purpose. |

---

## Phase-17 Delta by Module

### `src/loop.rs`
- Added pure decision helpers (`review_handling`, terminal-outcome mapping, step progression helpers).
- Added lifecycle progress messaging (step/attempt/review/resume) on stderr.
- Added actionable error guidance in `LoopError::Display`.
- Expanded `LoopReport` with run-level tokens/requests/log path.
- Added helper-table tests and guidance/report regression coverage.

### `src/main.rs`
- Added startup banner/dry-run banner (stderr).
- Added enriched success output (`tokens`, `requests`, `logs`).
- Added coarse failure pointer (`tod: logs at .tod/logs/`) for run/resume errors.

### `src/stats.rs`
- Added JSON contract-key stability tests.
- Added human-format contract stability tests.
- Added edge-outcome and legacy-compatibility regression tests.

### `src/cli.rs`
- Added explicit operational help text for cap/strict/dry-run/resume/json options.

### Docs/runtime surface
- Added machine-readable output contract docs and Phase 17 implementation log.

---

## Module-by-Module Findings

## `src/main.rs`
State: stable.
- Runtime output behavior is clearer and still keeps stdout clean for data output.

## `src/cli.rs`
State: stable.
- Surface area growth remains controlled and parse coverage remains strong.

## `src/config.rs`
State: stable.
- No immediate concerns; immutable configuration remains straightforward.

## `src/context.rs`
State: stable.
- Deterministic and budget-aware; future large-repo relevance tuning remains a candidate.

## `src/planner.rs`
State: stable.
- Validation strictness remains appropriate and should be preserved.

## `src/editor.rs`
State: stable.
- Typed error surface and schema gate remain well-scoped.

## `src/schema.rs`
State: strong.
- Continues to be the central execution guardrail.

## `src/runner.rs`
State: strong.
- Transactional semantics and rollback behavior are critical strengths.

## `src/reviewer.rs`
State: stable.
- Simple policy remains robust and easy to reason about.

## `src/llm.rs`
State: stable.
- Retry behavior is explicit; provider monoculture is strategic, not immediate correctness risk.

## `src/log_schema.rs`
State: stable.
- Boundary remains clean data+serde only.

## `src/loop_io.rs`
State: stable.
- Best-effort writes and atomic checkpoint semantics remain intact.

## `src/loop.rs`
State: watch.
- Coverage is high and recent decomposition helped.
- Still the primary change-risk surface due to orchestration density.

## `src/stats.rs`
State: stable+.
- Output contracts are much better protected.
- Continued growth warrants disciplined contract-version awareness.

## `src/util.rs`, `src/test_util.rs`
State: stable.
- Minimal, focused, low risk.

---

## Risk Register (Current)

1. `loop.rs` size/centrality  
   Severity: high  
   Mitigation: continue pure-helper extraction and table-test coverage.

2. Request/usage accounting precision on all terminal paths  
   Severity: medium-high  
   Mitigation: harden request-count invariants with explicit tests around pre/post-LLM failure paths.

3. CLI output contract drift (stdout/stderr interplay)  
   Severity: medium  
   Mitigation: add command-level integration contract tests for human/JSON modes.

4. Context/edit scaling in larger codebases  
   Severity: medium-high  
   Mitigation: improve context relevance and deterministic change summarization before major edit-contract expansion.

---

## Recommended Phase-18 Module Focus

1. `src/loop.rs`: accounting integrity hardening + additional extraction of pure decisions.
2. `src/main.rs`: output-policy controls (`--quiet` behavior) and precise failure guidance handoff.
3. `src/stats.rs`: command-contract integration coverage and compatibility confidence expansion.
4. `src/loop_io.rs` + `src/log_schema.rs`: keep compatibility defaults stable while enabling more precise run-level observability pointers.
