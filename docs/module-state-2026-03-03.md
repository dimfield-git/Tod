# Module State Review (2026-03-03, Post-Phase 15)

Date: 2026-03-03  
Scope: Full review of every module in `src/` against current tree behavior  
Reviewer: Senior engineer assessment

Validation baseline used for this review:
- `cargo test`: **193 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

## Executive Summary

Tod is in a strong post-Phase-15 state for a terminal-first Rust coding agent:
- The Phase 15 boundary split landed: `log_schema.rs` is pure types, `loop_io.rs` owns persistence/identity, and `loop.rs` is orchestration.
- Legacy compatibility and artifact contracts are actively protected by tests.
- Safety fundamentals remain strong (`schema` validation + transactional `runner` apply).
- The main technical concentration risk is still `src/loop.rs` size and centrality.
- The main product gap is workflow/productization maturity, not core correctness.

## Snapshot Table

| Module | LOC | Test count | Current state |
|---|---:|---:|---|
| `src/main.rs` | 221 | 4 | Stable command dispatch and init handling |
| `src/cli.rs` | 272 | 10 | Predictable clap surface; good conversion tests |
| `src/config.rs` | 91 | 2 | Clean immutable config core |
| `src/context.rs` | 470 | 9 | Solid context budgeting and path-checked file loading |
| `src/planner.rs` | 274 | 11 | Well-scoped planning and semantic validation |
| `src/editor.rs` | 220 | 8 | Strong typed bridge from LLM text to validated edit batches |
| `src/schema.rs` | 747 | 28 | Safety backbone; strongest validation module |
| `src/runner.rs` | 645 | 19 | Transactional edit apply + deterministic cargo stage execution |
| `src/reviewer.rs` | 167 | 9 | Small, deterministic policy engine |
| `src/llm.rs` | 425 | 15 | Stable provider abstraction and bounded retry behavior |
| `src/log_schema.rs` | 99 | 1 | Correctly reduced to pure schema/serde concerns |
| `src/loop_io.rs` | 185 | 4 | New persistence + run identity boundary, good extraction quality |
| `src/loop.rs` | 2368 | 48 | Robust orchestration with deep tests; still central hotspot |
| `src/stats.rs` | 1180 | 23 | Compatibility-aware summaries with improved coverage |
| `src/util.rs` | 49 | 3 | Minimal and correct utility helpers |
| `src/test_util.rs` | 43 | 0 | Effective shared TempSandbox support |

---

## `src/main.rs`

Responsibility:
- Entry point, command dispatch, provider initialization, exit code behavior.

Current state:
- Stable and readable.
- `run` config conversion handles non-run paths safely.
- `status`/`stats` error messaging is practical.

Risk/debt:
- `init_project` helpers remain in main; acceptable but mixed concerns.

Recommendation:
- Keep stable; only extract if CLI surface grows materially.

---

## `src/cli.rs`

Responsibility:
- Clap command model and conversion of `run` args into `RunConfig`.

Current state:
- Good defaults and validation (`--max-iters >= 1`).
- Straightforward conversion semantics.

Risk/debt:
- `max_total_iterations` is derived from `max_iters * 5`; no independent CLI control.

Recommendation:
- Keep as-is unless operators request separate total-cap tuning.

---

## `src/config.rs`

Responsibility:
- Immutable runtime configuration and mode definitions.

Current state:
- Clean defaults and low complexity.

Recommendation:
- Keep unchanged.

---

## `src/context.rs`

Responsibility:
- Planner context and per-step context assembly with byte budgets.

Current state:
- Conservative, deterministic context collection and truncation.
- Reuses path validation safeguards.

Risk/debt:
- Non-UTF8 targeted files are not represented in context.
- Large repo context relevance may still be coarse.

Recommendation:
- Future: add improved relevance heuristics and optional non-UTF8 fallback summaries.

---

## `src/planner.rs`

Responsibility:
- Plan prompt + response parsing/validation.

Current state:
- Strong validation gates and predictable plan object output.

Recommendation:
- Preserve strictness; avoid loosening schema constraints.

---

## `src/editor.rs`

Responsibility:
- Edit prompt generation and validated edit-batch creation.

Current state:
- Clear typed error boundary and schema-backed output control.

Recommendation:
- Stable; maintain strict output contract discipline.

---

## `src/schema.rs`

Responsibility:
- Canonical edit schema, JSON extraction, and path/range/batch validation.

Current state:
- Strongest safety module with broad regression coverage.
- Symlink-aware path safety checks are conservative and appropriate.

Risk/debt:
- JSON extraction is robust but could be hardened further for noisy multi-block responses.

Recommendation:
- Candidate for targeted extraction-hardening phase.

---

## `src/runner.rs`

Responsibility:
- Transactional edit application and quality pipeline execution.

Current state:
- Rollback semantics are clear and typed.
- Stage lists are static by mode (`default`: build+test, `strict`: fmt+clippy+test).
- Output truncation is UTF-8 safe.

Risk/debt:
- Rollback failure still leaves potential partial state in worst-case filesystem errors (properly surfaced).

Recommendation:
- Preserve design; add integration fixtures only if behavior drift appears.

---

## `src/reviewer.rs`

Responsibility:
- Pure decision policy for proceed/retry/abort.

Current state:
- Small and deterministic with good tests.

Recommendation:
- Keep unchanged unless policy goals change.

---

## `src/llm.rs`

Responsibility:
- Provider trait and Anthropic implementation with retry logic.

Current state:
- Retry behavior is contained within provider; higher layers avoid double-counting retries.
- Env-driven model/max-token configuration is explicit.

Risk/debt:
- Single-provider implementation limits operational flexibility.

Recommendation:
- Add second provider in a dedicated phase without changing trait semantics.

---

## `src/log_schema.rs`

Responsibility:
- Shared log struct definitions and serde defaults.

Current state:
- Phase 15 objective met: no persistence logic remains.

Recommendation:
- Keep this module pure data-only.

---

## `src/loop_io.rs`

Responsibility:
- Persistence primitives and run identity allocation.

Current state:
- Extraction quality is good.
- Best-effort write semantics preserved.
- Checkpoint atomic tmp+rename pattern preserved.

Recommendation:
- Continue routing all loop persistence/identity concerns through this boundary.

---

## `src/loop.rs`

Responsibility:
- Core orchestration (`run`, `resume`, loop state transitions, compatibility decisions).

Current state:
- Behavior is robust with deep test coverage.
- Fingerprint compatibility logic is now pure and table-tested.
- Planner-error artifact path now uses shared identity/persistence helpers.

Risk/debt:
- Still the biggest module and primary maintenance hotspot.

Recommendation:
- Continue phase-scoped extraction of coherent concerns while preserving behavior.

---

## `src/stats.rs`

Responsibility:
- Read-only run/log summarization and formatting.

Current state:
- Strong compatibility behavior (modern + legacy artifacts).
- Correctly handles plan-error-only runs.
- Outcome and request counters are explicit and test-backed.

Risk/debt:
- Human-readable format is useful, but no structured export mode for tooling.

Recommendation:
- Future phase: add optional machine-readable summaries (JSON output mode).

---

## `src/util.rs` and `src/test_util.rs`

Current state:
- Both are minimal and fit-for-purpose.

Recommendation:
- Keep unchanged.

---

## Cross-Module Findings

Strengths to preserve:
- Typed error surfaces and deterministic control flow.
- Safety-first validation-before-apply pipeline.
- Compatibility-conscious persistence and stats behavior.
- High and growing test coverage in critical modules.

Main risks to prioritize next:
1. Productization gap (workflow safety, docs/runbooks, adoption ergonomics).
2. `loop.rs` concentration risk over continued feature growth.
3. Precision/efficiency gap for large-codebase edit operations.

Recommended near-term focus:
- Phase 16 should target operator usability and safe workflow adoption while keeping behavior-preserving refactors incremental.
