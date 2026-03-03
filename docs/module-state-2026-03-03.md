# Module State Review (2026-03-03)

Date: 2026-03-03  
Scope: Full review of every module in `src/` (no code changes)  
Reviewer: Senior engineer assessment

Validation baseline used for this review:
- `cargo test`: **178 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

## Executive Summary

Tod is in a strong post-Phase-13 state for a terminal-first coding agent prototype:
- Runtime correctness guardrails are now materially better than earlier phases (checkpoint-fresh fingerprints, resume profile persistence, v2 content-aware drift detection, run-id collision hardening).
- Module boundaries are mostly coherent, with one major concentration risk in `src/loop.rs` (orchestration + state schema + persistence + resume + most tests).
- Safety and path-jailing model are robust and conservative.
- Observability quality is high for post-plan flows, but still has one structural blind spot: failures before `RunState` creation (planner-stage failures) do not produce run artifacts.
- Documentation and strategy files lagged behind code and should continue to be treated as phase deliverables.

## Snapshot Table

| Module | Approx LOC | Test count | Current state |
|---|---:|---:|---|
| `src/main.rs` | 219 | 4 | Stable entrypoint; small invariant debt removed in Phase 13 |
| `src/cli.rs` | 272 | 10 | Solid CLI parsing/conversion; simple and predictable |
| `src/config.rs` | 91 | 2 | Clean immutable config core |
| `src/context.rs` | 470 | 9 | Strong budget/format logic; conservative path handling |
| `src/planner.rs` | 274 | 11 | Well-scoped planner surface and semantic validation |
| `src/editor.rs` | 220 | 8 | Correct batching/validation bridge; minor cleanup opportunity |
| `src/schema.rs` | 747 | 28 | Strong validation backbone; central safety module |
| `src/runner.rs` | 645 | 19 | Transactional edits + pipeline execution are solid |
| `src/reviewer.rs` | 167 | 9 | Pure decision logic, easy to reason about |
| `src/llm.rs` | 425 | 15 | Provider abstraction is clean; retry policy isolated |
| `src/loop.rs` | 2163 | 42 | Functional core is robust; maintainability hotspot |
| `src/stats.rs` | 1056 | 19 | Good reporting/compatibility; coupled to loop internals |
| `src/util.rs` | 49 | 3 | Minimal and correct |
| `src/test_util.rs` | 43 | 0 | Useful shared temp sandbox utility |

---

## `src/main.rs`

### Responsibility
- Program entrypoint.
- Command dispatch (`run`, `resume`, `status`, `stats`, `init`).
- Process exit code policy and operator-facing stderr/stdout messaging.

### What is working well
- Dispatch is straightforward and readable.
- Provider initialization errors are handled explicitly with non-zero exits.
- Phase 13 removed non-test `expect` in run config conversion, aligning behavior with invariant expectations.
- `init` flow includes `.tod/` gitignore idempotency.

### Risks / debt
- `init_project` and `append_tod_gitignore` are still embedded in `main.rs`; this is acceptable but mixes entrypoint and project-scaffolding concerns.
- Exit code semantics are mostly consistent but not formally documented as an interface contract.

### Test posture
- 4 focused init tests; behavior is covered for creation, gitignore insertion, idempotency, existing-dir failure.

### Recommendation
- Keep stable for now; optional future extraction of init functionality into a dedicated module if CLI surface grows.

---

## `src/cli.rs`

### Responsibility
- CLI argument model via clap derive.
- Conversion from `Command::Run` to `RunConfig`.

### What is working well
- Good parse coverage for default and flag-heavy paths.
- `parse_max_iters` prevents invalid zero values.
- `into_run_config` is deterministic and keeps config construction centralized.

### Risks / debt
- `max_total_iterations` is derived as `max_iters * 5` with no explicit CLI control; this is simple but limits tunability.
- `resume` intentionally does not reconstruct full runtime profile from CLI (now correctly delegated to persisted `RunProfile` in loop state).

### Test posture
- 10 tests; parsing and conversion behavior are well covered.

### Recommendation
- No immediate changes required; revisit only if operator demand appears for separate total-cap tuning.

---

## `src/config.rs`

### Responsibility
- Immutable runtime configuration data (`RunConfig`, `RunMode`) and defaults.

### What is working well
- Small, explicit, clear defaults.
- Good foundation for deterministic execution policy.

### Risks / debt
- None significant at current scale.

### Test posture
- 2 sanity tests are adequate for module complexity.

### Recommendation
- Keep as-is; avoid overengineering.

---

## `src/context.rs`

### Responsibility
- Builds bounded planner context, step file context, and retry context.
- Handles truncation and UTF-8-safe snapping.

### What is working well
- Budget constants and truncation logic are explicit and robust.
- Excludes `.git`, `target`, `.tod` during tree walk.
- Step context validates paths using schema safety checks.
- Omission notes provide actionable transparency to the model.

### Risks / debt
- `read_to_string` model can fail on non-UTF8 files in targeted paths; that is safe but can block progress on mixed-content repositories.
- `MAX_TREE_DEPTH` is reused by fingerprint walk in `loop.rs`, creating an implicit cross-concern coupling.

### Test posture
- 9 tests cover budget limits, truncation behavior, exclusion rules, and formatting.

### Recommendation
- Keep behavior; document non-UTF8 limitations and optionally add a byte-preview fallback mode in a future phase.

---

## `src/planner.rs`

### Responsibility
- Calls LLM for plan generation.
- Validates plan structure and path safety semantics.

### What is working well
- Prompt boundary is explicit; output constrained to JSON.
- Semantic validation catches empty steps/files and disallowed paths.
- Returns usage metadata when available.

### Risks / debt
- Planner-stage failures currently terminate before run artifact generation (tracked elsewhere, not a bug in this module itself).

### Test posture
- 11 tests cover parse edge cases and validation failures.

### Recommendation
- Stable; preserve prompt immutability and validation strictness.

---

## `src/editor.rs`

### Responsibility
- Calls LLM for edit batch generation.
- Parses and validates edits against schema rules.

### What is working well
- Clear separation of LLM generation vs schema validation.
- Strongly typed error surface (`Llm`, `Parse`, `Validation`).
- Maintains usage propagation to loop.

### Risks / debt
- `_format_file_context` function-pointer binding is a placeholder-style coupling anchor; harmless but stylistically awkward.

### Test posture
- 8 tests, including fence parsing and path safety failure modes.

### Recommendation
- Optional cleanup in a maintainability phase; no behavior urgency.

---

## `src/schema.rs`

### Responsibility
- Canonical edit schema types.
- Path/content/range/batch validation.
- JSON extraction helper for model responses.

### What is working well
- This is the safety backbone of the agent.
- Path validation includes lexical checks and existing-ancestor canonical checks for symlink escape defense.
- Batch-level conflict checks are strong (duplicate writes, write/replace conflicts, overlapping ranges).
- JSON extraction supports direct/fenced/preamble variants.

### Risks / debt
- `extract_json` still uses a broad first-`{`/last-`}` fallback; robust for many cases but vulnerable to noisy multi-block responses.
- TOCTOU remains theoretically possible between validation and apply under adversarial concurrent filesystem mutation (outside nominal threat model).

### Test posture
- 28 tests; strongest test surface in the project outside loop orchestration.

### Recommendation
- Next-phase candidate: strengthen extraction for multi-block markdown without broad behavior changes.

---

## `src/runner.rs`

### Responsibility
- Applies validated edits transactionally.
- Executes quality pipeline (`build/test` or `fmt/clippy/test`).
- Truncates and normalizes pipeline output for retry loops.

### What is working well
- Snapshot + rollback strategy is clean and understandable.
- ReplaceRange preserves newline style and trailing newline semantics.
- Pipeline stage list is explicit and static (no shell injection surface).
- Output truncation is UTF-8 aware.

### Risks / debt
- Rollback failure can still leave partial state (properly surfaced via `ApplyError::Rollback`, but operationally painful).
- No integration-level tests for full cargo pipeline on fixture projects (current unit focus is appropriate but leaves some OS/toolchain assumptions unexercised).

### Test posture
- 19 tests with broad functional coverage for edit operations and output handling.

### Recommendation
- Keep core design; optional future fixture-based integration tests for command-stage behavior.

---

## `src/reviewer.rs`

### Responsibility
- Pure policy for `Proceed` vs `Retry` vs `Abort` based on run result and cap.

### What is working well
- Tiny, deterministic, and easy to reason about.
- Error context formatting is simple and stable.

### Risks / debt
- Policy is intentionally simple; future sophistication (e.g., failure class heuristics) belongs behind explicit product decisions.

### Test posture
- 9 focused tests; excellent for module scope.

### Recommendation
- Keep unchanged unless product policy changes.

---

## `src/llm.rs`

### Responsibility
- Defines provider abstraction.
- Implements Anthropic provider with retries/backoff and response parsing.
- Tracks usage metadata structure.

### What is working well
- Retry logic is isolated and bounded.
- Environment-driven model/max token settings are explicit.
- Error taxonomy is useful and not leaky.

### Risks / debt
- Jitter source is time-derived pseudo randomness; acceptable but not truly random.
- Request accounting at higher layers currently depends on usage presence in some paths; this is mostly a loop/stats concern but starts here with `Option<Usage>`.

### Test posture
- 15 tests including env behavior, retryability logic, usage semantics.

### Recommendation
- Keep API stable; future provider expansion should stay behind `LlmProvider` and preserve blocking semantics unless a deliberate async migration is chosen.

---

## `src/loop.rs`

### Responsibility
- Core orchestration lifecycle (`run`, `resume`, step loop).
- State model (`RunState`, `StepState`), checkpointing, fingerprinting, log emission.
- Terminal outcome semantics.

### What is working well
- Phase 13 improvements materially strengthened determinism:
  - checkpoint-scoped fingerprint refresh before every runtime checkpoint in `run_from_state`.
  - persisted resume execution profile (`RunProfile`) with legacy compatibility path.
  - versioned fingerprint format with v2 content-aware hashing and v1 migration strategy.
  - run-id uniqueness with fractional timestamp + suffix fallback.
- Best-effort atomic checkpoint strategy is solid.
- Exit-path logging after planning is comprehensive (`final.json` and per-attempt logs).
- Test suite is deep (42 tests in this module alone).

### Risks / debt
- Primary maintainability hotspot: this file now contains orchestration, state schema, fingerprint algorithm, log schema, and a very large test suite.
- Planner-stage failure still has no run-scoped artifact because run ID/state is established after planning succeeds.
- Stats remains coupled to loop-defined log structs (`AttemptLog`, `PlanLog`, `FinalLog`, `RunState`).

### Test posture
- 42 tests covering loop controls, checkpoint/logging semantics, resume compatibility, profile persistence, and fingerprint version migration.

### Recommendation
- Highest-priority technical debt item: split log schema and/or persistence helpers into dedicated module(s) while preserving behavior.

---

## `src/stats.rs`

### Responsibility
- Reads `.tod` state/log artifacts.
- Summarizes single and multi-run outcomes.
- Formats operator-facing summaries.

### What is working well
- Correctly prefers `final.json` for modern run outcome truth, with legacy heuristic fallback.
- Handles legacy missing fields via serde defaults inherited from loop log structs.
- Sorting semantics for run IDs remain valid after run-id format hardening.

### Risks / debt
- Strong coupling to loop internals (`use crate::r#loop::{AttemptLog, FinalLog, PlanLog, RunState}`) increases change blast radius.
- Multi-run aggregate buckets collapse some distinct outcome classes (e.g., cap/token cap grouped for aggregate reporting).

### Test posture
- 19 tests with broad coverage across modern and legacy artifact patterns.

### Recommendation
- Next-phase candidate: decouple stats from loop internals by introducing a neutral log-schema module shared by writer and reader paths.

---

## `src/util.rs`

### Responsibility
- Shared warning emission helper and macro.
- UTF-8-safe preview helper.

### What is working well
- Minimal and correct.
- Used in error/reporting surfaces where truncation safety matters.

### Risks / debt
- None significant.

### Test posture
- 3 focused tests.

### Recommendation
- Keep unchanged.

---

## `src/test_util.rs`

### Responsibility
- Shared `TempSandbox` for test isolation and cleanup.

### What is working well
- Simple and effective RAII cleanup model.
- Supports deterministic temp dir naming with process/test counter.

### Risks / debt
- `unwrap()` usage is appropriate in test-only code.

### Test posture
- Consumed indirectly by many module tests.

### Recommendation
- Keep unchanged.

---

## Cross-Module Findings

## Strengths to preserve
- Typed error boundaries across modules.
- Blocking provider + pure reviewer model simplifies runtime reasoning.
- Validation-before-apply safety discipline (`schema` + `runner`).
- High unit test density and good regression targeting.

## Main risks to prioritize
1. `loop.rs` concentration risk (change velocity and review complexity).
2. Stats/log-schema coupling to loop internals.
3. Planner-stage failure observability gap (no run artifact before `RunState` exists).

## Recommended near-term focus
- Next phase should target observability/schema cohesion rather than feature expansion:
  - shared log schema extraction,
  - planner-stage terminal artifact strategy,
  - stats aggregate outcome fidelity improvements,
  - optional light decomposition of loop helpers.

