# Tod Codebase Assessment (Post-Phase 10)

Date: 2026-02-28  
Scope reviewed: `AGENTS.md`, `ROADMAP.md`, all files in `src/`, all files in `docs/`, `tod-living-list.md`, plus `README.md`, `Cargo.toml`, `LICENSE`.

Validation run on current tree:
- `cargo test`: **154 passed, 1 ignored, 0 failed** (155 total)
- `cargo clippy -- -D warnings`: **clean**

---

## A1. Architecture Map

### Module responsibilities, drift, and coupling

| Module | LOC | Single responsibility (actual) | Drift / boundary issues | Coupling assessment |
|---|---:|---|---|---|
| `src/main.rs` | 218 | Crate entrypoint, CLI dispatch, process exit semantics, `init` implementation | `init_project` + `.gitignore` mutator live in entrypoint (`main.rs:116-152`) instead of dedicated module | Moderate: depends on CLI, loop, stats, LLM provider |
| `src/cli.rs` | 272 | Command model + clap parsing + `RunConfig` conversion (`into_run_config`) | None significant | Narrow: primarily `config` boundary |
| `src/config.rs` | 91 | Immutable runtime configuration types | None | Very narrow, pure data |
| `src/context.rs` | 470 | Planner/step/retry context building with byte budgets | `MAX_TREE_DEPTH` reused by fingerprinting in `loop.rs` (`loop.rs:8`, `loop.rs:64-66`) couples unrelated concerns | Moderate: depends on `schema::validate_path` |
| `src/editor.rs` | 220 | Edit LLM call + parse/validate edit batches | Dead-ish import anchor: `_format_file_context` function pointer (`editor.rs:86`) indicates unresolved ownership of formatter | Narrow core coupling (`llm`, `planner::PlanStep`, `schema`) |
| `src/llm.rs` | 425 | Provider trait + Anthropic HTTP integration + retry/backoff | None major | Narrow to `util::safe_preview`; external API dependency isolated |
| `src/loop.rs` | 1311 | Orchestration state machine + checkpoint/log writing + fingerprint/resume | High cohesion pressure: orchestration, persistence, fingerprinting, logging, and many tests in one file | Wide hub (planner/editor/runner/reviewer/context/llm/config) |
| `src/planner.rs` | 274 | Plan LLM call + semantic plan validation | None | Narrow (`llm`, `schema::extract_json`) |
| `src/reviewer.rs` | 167 | Pure run-result decision policy | None | Very narrow (`runner::RunResult`) |
| `src/runner.rs` | 645 | Transactional edit apply + cargo pipeline execution | None significant | Moderate (`config`, `schema`) |
| `src/schema.rs` | 747 | Edit schema, extraction, path/range/batch validation | None major | Moderate: central validator consumed by multiple modules |
| `src/stats.rs` | 737 | Read-only log/state summarization + formatting | Depends on loop-internal log structs (`stats.rs:8`) instead of a neutral log schema module | Moderate/wide to `loop` types |
| `src/test_util.rs` | 43 | Shared test sandbox helper | None | Test-only |
| `src/util.rs` | 49 | Shared UTF-8 preview + warning macro | None | Narrow shared utility |

### Coupling summary

- Good narrow boundaries exist around `planner`, `editor`, `reviewer`, `config`, `util`.
- The widest coupling is still `loop.rs` (expected for orchestrator, but currently over-accumulated).
- A second coupling hotspot is `stats.rs` importing `AttemptLog/PlanLog/RunState` directly from `loop.rs` (`stats.rs:8`), which makes log evolution expensive.

---

## A2. Correctness & Invariants

### Iteration caps, token caps, context budgets

- Per-step cap and total cap are enforced in `run_from_state` (`loop.rs:520`, `loop.rs:523-527`, `loop.rs:619-624`).
- Token cap is enforced after plan/edit LLM calls (`loop.rs:495-501`, `loop.rs:553-559`).
- Context budgets are explicitly bounded in `context.rs` constants and truncation paths (`context.rs:8-14`, `context.rs:82-103`, `context.rs:136-172`, `context.rs:174-184`).

Assessment: **mostly enforced, one gap**.
- Gap: `resume()` does not pre-check whether loaded `state.usage.total()` already exceeds `state.max_tokens` before issuing another LLM call (`loop.rs:643-668`).

### LLM retry containment

- Retry behavior is contained inside `AnthropicProvider::complete()` (`llm.rs:137-227`), with retryable statuses centralized (`llm.rs:108-110`).
- Orchestration never implements transport retry policy itself.

Assessment: **meets invariant**.

### Fingerprint + resume + checkpoint round-trip

- `RunState`/`StepState` are serializable (`loop.rs:25`, `loop.rs:156`).
- Checkpoint uses atomic tmp+rename (`loop.rs:236-244`).
- Round-trip tests exist (`loop.rs:959-981`, `loop.rs:1199-1230`).
- Resume checks fingerprint unless `--force` (`loop.rs:656-663`).

Assessment: **round-trip behavior is solid; fingerprint strength is weak**.
- Fingerprint hashes `(path, size)` only (`loop.rs:50-52`, `loop.rs:97-100`), so same-size content edits can evade drift detection.

### Context ordering determinism

- Planner context sorts collected file paths (`context.rs:65`) before rendering.

Assessment: **deterministic for planner context**.

Potential nondeterminism to watch:
- Step context order follows plan `files` order (`context.rs:110-133`). If planner emits unstable file ordering for equivalent plans, step context order can vary.
- Verify by replaying the same plan JSON across machines and diffing generated step context.

### Concurrency assumptions

- No shared mutable state in production path.
- Blocking/sequential loop; no async runtime.

Assessment: **no concurrency hazard found in runtime path**.

---

## A3. Safety Model

### Path validation / sandbox escape

- Validation checks empty, absolute, parent traversal, lexical containment, and existing-ancestor canonicalization (`schema.rs:156-207`, `schema.rs:403-412`).

Assessment: **strong for intended model**.

Residual risk:
- `runner::apply_edits` trusts pre-validated paths and directly does `sandbox_root.join(path)` (`runner.rs:160`, `runner.rs:178`) without revalidation.
- This is safe for current call path (`editor` -> `validate_batch` -> `apply_edits`), but remains TOCTOU-exposed if a symlink is altered between validation and write by an external process.
- Verify with an integration test that swaps a symlink target between validation and apply.

### Transactional rollback

- Snapshot-before-mutate implemented (`runner.rs:93`, `runner.rs:105-130`), rollback on first apply failure (`runner.rs:96-99`).

Assessment: **good baseline transactional behavior**.

Edge case:
- Rollback itself can fail and return `ApplyError::Rollback` (`runner.rs:132-155`), potentially leaving partial restore.

### Command restrictions

- Runner pipeline commands are static cargo-only stages (`runner.rs:287-299`).
- No LLM-provided shell command execution path exists.

Assessment: **cargo-only whitelist effectively enforced**.

### UTF-8 boundary safety

- `safe_preview` uses char boundary checks (`util.rs:17-26`).
- Context truncation uses boundary-safe snapping (`context.rs:186-197`, `context.rs:328-334`).
- Runner truncation avoids invalid boundaries when no newline exists (`runner.rs:325-351`).

Assessment: **UTF-8 handling is consistently defensive**.

---

## A4. Observability & Stats

### Can failed runs be diagnosed from logs alone?

- Success/failure after runner execution is well logged per attempt (`loop.rs:291-307`).
- Plan is logged (`loop.rs:250-266`).

Gap:
- `EditError` and `ApplyError` exits do not write attempt logs or checkpoint before return (`loop.rs:542-548`, `loop.rs:565-569`).
- Result: some failures are visible only on stderr in the current process, not reconstructible from `.tod/logs` alone.

### Error message quality

- Typed, actionable error surfaces with path + message in loop/context/stats (`loop.rs:374-436`, `context.rs:36-57`, `stats.rs:68-83`).

Assessment: **good overall**.

### Token usage tracking accuracy/completeness

- Token totals come from cumulative usage snapshots in attempt logs (`stats.rs:226-233`).
- `llm_requests` in stats is computed as count of attempt logs with `usage_this_call` (`stats.rs:233-236`).

Gap:
- Planner call usage is not represented as an attempt log entry, so request counts underreport by at least one successful planner call in normal runs.
- Calls returning `usage: None` are excluded from request counting entirely.

### Stats semantics: billed requests vs transport retries

- Good: transport retries are internal to provider and do not create loop attempts (`llm.rs:148-193`).
- Ambiguous/inaccurate: stats currently represent "usage-bearing edit calls in attempts," not total billed requests across the run.

---

## A5. Code Quality

### Naming and consistency

- Naming is generally consistent and phase-aligned.
- One semantic mismatch: `RunSummary.retries_per_step` actually stores attempt counts per step (`stats.rs:35`, `stats.rs:210-214`).

### Duplication and extraction

- Core utility duplication is mostly resolved (`util.rs`).
- Remaining extraction debt is concentrated in `loop.rs` (1311 LOC) and `stats` <- `loop` type coupling.

### Error typing and structure

- Typed enums are consistently used in runtime modules.
- No major untyped-error regressions found.

### Cohesion and function scope

- `run_from_state` is coherent but large and does many concerns (context build, usage accounting, apply/run, review, logging, checkpointing) (`loop.rs:511-637`).

### Test coverage and gaps

Strengths:
- Very high unit test density; current baseline passes.

Obvious gaps:
- No test for resume when checkpoint already exceeds token cap.
- No test asserting checkpoint/log emission on edit/apply failure paths.
- No integration test for symlink TOCTOU during apply.
- No test validating stats request counts include planner call semantics.

### `unwrap`/`expect` in non-test code

- Invariant says no unwraps in non-test code (`AGENTS.md:104`).
- Non-test `expect` exists in `main.rs` (`main.rs:29-31`).

Assessment: **minor but explicit invariant violation**.

### Dead code/stale comments

- `editor.rs` contains an unused function-pointer binding solely to reference `format_file_context` (`editor.rs:86`).
- No `TODO/FIXME/XXX` markers found.

---

## A6. UX Surface

### CLI surface correctness

- Commands and flags in code are coherent and parse tests cover combinations (`cli.rs`, tests at `cli.rs:147-272`).
- `--project` is wired for `run`, `resume`, `status`, `stats` (`cli.rs:37-83`, `main.rs:61-112`).

### `--project` behavior coverage

- Parsing coverage exists.
- Runtime routing for status/stats correctly uses provided project path (`main.rs:88-112`).

### Documentation alignment

- `README.md` mostly matches current behavior (`README.md:17-37`, `README.md:59-61`).
- `live-run-log.md` contains stale note claiming `status/stats` lack `--project` (`docs/live-run-log.md:95`), which is no longer true.
- Architecture pages are materially stale vs current code:
  - old LLM trait signature shown as `Result<String, LlmError>` (`docs/index.html:134`, `docs/tod-architecture.html:134`) vs `LlmResponse` in code (`llm.rs:64-74`)
  - outdated loop/context interactions and command surface.
- `AGENTS.md` project map references docs files not present in `docs/` (`AGENTS.md:83-86`).

Assessment: **binary UX is good; documentation parity is inconsistent**.

---

## B. Phase 9 & 10 Implementation Quality

### What landed well

- Phase 9 context budget extraction is robust and tested (`context.rs`, multiple budget tests).
- Provider retry is well-contained and conservative (`llm.rs:108-227`).
- Phase 10 typed errors materially improved diagnostics (`loop.rs`, `context.rs`, `stats.rs`).
- CLI naming and `--project` support are now consistent in runtime code.

### What is brittle

- `loop.rs` remains a high-change-risk hotspot (large surface area + many responsibilities).
- Resume correctness still depends on a size-only fingerprint.
- Observability is incomplete for pre-runner failures (edit/apply creation path).
- Stats request semantics currently undercount planner/usage-none completions.
- Documentation drift is accumulating (especially architecture HTML + live log notes).

### Patterns to reinforce

- Keep provider retry logic isolated from orchestration.
- Keep typed errors with `PathBuf` + `io::ErrorKind` as the standard.
- Preserve budget caps and UTF-8-safe truncation approach across new features.

### Patterns to correct

- Require checkpoint/log coverage on every early exit path in orchestrator.
- Decouple stats log schema from `loop.rs` internals.
- Treat doc parity checks as part of phase completion criteria.

---

## Targeted Recommendations (Actionable)

| Recommendation | Why | Scope | Expected risk |
|---|---|---|---|
| Add pre-resume token-cap guard | Prevent issuing extra LLM call when cap already exceeded | `loop.rs` (`resume` / `run_from_state`) + tests | Low |
| Write attempt/checkpoint records for edit/apply error exits | Make failures diagnosable from logs and resumable state | `loop.rs`, maybe `AttemptLog` extension | Medium |
| Fix `llm_requests` accounting semantics | Stats currently undercount planner request and usage-none completions | `loop.rs` log schema + `stats.rs` summarization + tests | Medium |
| Strengthen fingerprint to content hashing (or hybrid) | Current same-size edits can evade drift detection | `loop.rs` fingerprint format + resume compatibility tests | Medium |
| Decompose `loop.rs` by concern | Reduce regression risk and review cost | Extract modules for fingerprint/log/checkpoint orchestration helpers | Medium |
| Remove non-test `expect` in `main` | Align with invariant and improve failure consistency | `main.rs:29-31` | Low |
| Repair documentation parity | Prevent operator confusion and stale architecture mental model | `docs/live-run-log.md`, `docs/index.html`, `docs/tod-architecture.html`, `AGENTS.md` map | Low |
