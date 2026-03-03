# AGENTS.md -- Tod

## Operating Principles

- Work in small, reviewable changes. Prefer one logical change per task.
- Preserve behavior unless the task explicitly calls for semantic changes.
- Follow existing patterns in this codebase; avoid introducing new conventions without a reason.
- Do not add dependencies without explicit approval.
- Keep all runtime errors typed and actionable.
- Keep target-project filesystem mutation confined to `runner.rs` pathways.
- Respect phase boundaries: complete and verify current phase scope before moving on.
- Treat docs and operator-facing behavior as part of done, not follow-up work.

## Definition of Done

A change is complete only when:
- `cargo test` passes (baseline: **215 passed, 1 ignored**)
- `cargo clippy -- -D warnings` is clean
- behavior and docs are aligned for any changed runtime surface

## Repo Identity

Tod is a minimal Rust coding agent that:
1. plans with an LLM,
2. generates JSON edit batches,
3. validates and applies edits transactionally,
4. runs cargo quality pipelines,
5. iterates until success or explicit caps.

Platform assumptions:
- Linux-first
- terminal-only (no GUI)
- blocking execution model (no async runtime)

Current phase state:
- Phases 1-17 complete
- Phase 18 planned (see `PHASE18.md`)

Core design principle:
- **LLM generates intent; deterministic Rust code constrains execution.**

## Project Map

```text
src/
  main.rs         entry point + command dispatch (run/resume/status/stats/init)
  cli.rs          clap command model + run config conversion
  config.rs       run configuration types (immutable after construction)
  context.rs      planner/step/retry context building + budget enforcement
  planner.rs      plan prompt + plan validation
  editor.rs       edit prompt + edit batch generation
  schema.rs       edit schema + JSON extraction + path/range/batch validation
  runner.rs       transactional edit apply + cargo stage execution
  reviewer.rs     proceed/retry/abort policy
  llm.rs          LLM provider trait + Anthropic implementation + retries
  log_schema.rs   shared log structs (RunnerLog, AttemptLog, PlanLog, FinalLog) -- pure data + serde
  loop_io.rs      persistence primitives, run identity allocation, best-effort JSON writers
  loop.rs         orchestration state machine + checkpointing + resume + LoopReport emission
  stats.rs        read-only run/log summarization and formatting
  util.rs         shared warning + UTF-8-safe preview helper
  test_util.rs    shared temp sandbox helper (tests only)

docs/
  runbook.md                        operator decision guidance (Phase 16)
  ux-audit-2026-03-03.md            UX gap analysis and recommendations
  phase*-implementation-*.md        phase implementation logs
  codebase-assessment.md            architecture and risk assessment
  strategic-plan.md                 roadmap and phase sequencing
  module-state-2026-03-03.md        module-by-module state review
```

## Runtime Artifacts

```text
<project_root>/.tod/
  state.json
  logs/<run_id>/
    plan.json
    final.json
    step_<n>_attempt_<m>.json
```

## Architectural Invariants

- No `.unwrap()`/`.expect()` in non-test runtime paths.
- No global mutable runtime state.
- No async runtime.
- Planner/editor system prompts are product logic; change only with explicit intent.
- JSON action tags are stable contract (`write_file`, `replace_range`).
- Path safety must remain strict: relative-only, no traversal, sandbox containment, symlink-aware checks.
- Edit apply remains transactional with rollback on failure.

Resume and checkpoint invariants:
- Checkpoint fingerprint must represent workspace state at checkpoint time.
- Fingerprints are versioned:
  - v1 legacy: `(path,size)` hash
  - v2 current: content-aware hash
- Resume must keep legacy compatibility for old checkpoints.
- Resume should reuse originating execution profile when checkpoint profile exists.
- Fingerprint compatibility decisions are isolated in pure, table-testable logic.

Observability invariants:
- Every post-plan terminal path should write `final.json`.
- Planner-stage failures must also write `final.json` with `outcome: "plan_error"`.
- Planner-stage `plan_error` runs may not have `plan.json`; stats must still summarize them.
- Stats prefers `final.json` as source of truth and falls back for legacy logs.
- Legacy artifacts must remain deserializable via defaults where practical.

Module boundary invariants:
- `log_schema.rs` owns log struct types and serde defaults only. No IO, no formatting.
- `loop_io.rs` owns persistence primitives and run identity allocation. All writes are best-effort (never propagate). Some call sites warn on failure; others may be silent. Preserve current warning/silent behavior per call site. Checkpoint writes use atomic tmp+rename to prevent corruption.
- `loop.rs` owns orchestration flow. Delegates persistence and identity to `loop_io.rs`.

Workflow safety invariants:
- `run()` emits an informational dirty-workspace warning to stderr when the target project has uncommitted git changes. This warning is non-blocking -- it never prevents a run.
- The dirty-workspace check is silent when git is unavailable or the project is not a git repo.
- The dirty-workspace check does not apply to `resume` (fingerprint check covers drift) or `--dry-run` (no mutation).
- Lifecycle progress messages are stderr-only cosmetic output (`eprintln!`): best-effort, non-blocking, and never allowed to affect control flow, return values, or exit codes.
- Stdout remains clean for command output and `--json` machine-readable output.

Request counting semantics:
- A request is one logical LLM intent: one plan call = 1 request, one edit call = 1 request.
- Internal retries in `llm.rs` do not increment the request count.
- Usage fields (tokens) reflect what the successful response returned.
- Retry observability (count, latency) is a separate concern for future phases if needed.

## Quality and Testing Expectations

- Add or update tests when behavior changes.
- Favor deterministic tests with `TempSandbox`.
- Keep tests colocated in-module unless cross-module integration requires otherwise.
- Avoid weakening existing safety tests in `schema`, `runner`, `loop`, and `stats`.

## Phase History

| Phase | Scope | Status |
|---|---|---|
| 1 | Scaffolding: CLI/config/schema/path validation | Done |
| 2 | LLM integration: provider trait + Anthropic + planner extraction | Done |
| 3 | Editor flow: edit generation + apply integration | Done |
| 4 | Runner pipeline + output handling | Done |
| 5 | Full orchestration loop + retry/cap behavior | Done |
| 6 | Logging/checkpoint/resume baseline | Done |
| 7 | Strict mode gating and reviewer policy hardening | Done |
| 8 | Hardening: atomic checkpoint, budgets, shared test sandbox | Done |
| 9 | Working prototype validation and context extraction | Done |
| 10 | External usability + naming + project-root handling | Done |
| 11 | Reliability accounting and token-cap resume guard | Done |
| 12 | Failure observability and final outcome fidelity | Done |
| 13 | Resume determinism + fingerprint v2 + run-id hardening | Done |
| 14 | Observability/schema cohesion and metrics fidelity | Done |
| 15 | Loop surface reduction + compatibility hardening | Done |
| 16 | Operator usability + workflow safety | Done |
| 17 | Observability fidelity + orchestration maintainability + operator UX | Done |
| 18 | Observability integrity + operator control + output contract reliability | Planned |

## Phase 15 Outcomes

Completed outcomes:
- Establish clean three-module boundary: `log_schema.rs` (types), `loop_io.rs` (persistence + identity), `loop.rs` (orchestration).
- Eliminate duplicated run identity allocation logic.
- Isolate fingerprint compatibility decisions into pure, table-testable logic.
- Add regression coverage protecting legacy compatibility and artifact contracts.

Design decisions locked for Phase 15:
- `write_plan_error_artifact` moves from `log_schema.rs` to `loop_io.rs`, leaving `log_schema.rs` as pure data + serde.
- Run-id allocation returns a struct (`RunIdentity`), not bare strings. All call sites go through one helper.
- All persistence writes are best-effort (warn, don't propagate). Checkpoint writes must preserve the atomic tmp+rename pattern.
- No new user-facing capabilities this phase.

## Phase 16 Outcomes

Completed outcomes:
- Added operator runbook at `docs/runbook.md` with mode matrix, cap tuning, resume/force guidance, and failure recovery decisions.
- Added non-blocking dirty-workspace warning in `run()` for mutable runs (`git -C <project_root> status --porcelain`), with silent fallback when git is unavailable or repo checks fail.
- Extracted pure cap guards in `loop.rs`: `check_iteration_cap(&RunState)` and `check_token_cap(&RunState)`; kept checkpoint/final-log behavior inline and unchanged.
- Added machine-readable single-line JSON output for `tod status --json` and `tod stats --json` via `serde_json::json!`.
- Expanded regression coverage for dirty-workspace detection, cap helper behavior, CLI parsing for `--json`, and stats JSON formatters.

Phase 16 locked decisions retained:
- No changes to path safety, transactional apply semantics, or log compatibility defaults.
- `log_schema.rs` remains pure data + serde.
- No major feature-surface expansion (no patch mode, provider expansion, or git worktree orchestration engine).

## Phase 17 Scope (Locked)

Primary objective:
- Strengthen observability contracts and compatibility confidence.
- Continue reducing orchestration complexity in `loop.rs`.
- Make Tod communicative: lifecycle progress messaging, actionable errors, enriched completion output.
- Improve first-time operator experience through CLI help enrichment.

Locked deliverables:
1. **Observability hardening + compatibility audit**: tighten JSON/human output contract tests, audit edge-outcome summaries, document machine-readable output expectations.
2. **Orchestration extraction**: continue pure-helper decomposition of `loop.rs` decision logic with table tests.
3. **Run lifecycle messaging**: startup banner in `main.rs`, plan/step/attempt/review progress messages in `loop.rs`, resume confirmation. All stderr, all cosmetic, no control flow impact.
4. **Actionable errors + enriched output**: append operator guidance to `LoopError::Display`, extend `LoopReport` with token/log fields, enrich success output in `main.rs`. Populate from run-level accumulators in `RunState`, not per-attempt values.
5. **CLI help enrichment**: add operational context to clap help attributes (`--max-iters`, `--strict`, `--dry-run`, `--max-tokens`, `--force`, `--json`).

Design decisions locked for Phase 17:
- All lifecycle messages go to stderr via `eprintln!`. Stdout remains clean for piping and `--json`.
- No `--quiet` flag this phase.
- Lifecycle messages are best-effort cosmetic output: they must never affect control flow, return values, or exit codes.
- Error guidance is appended to `LoopError::Display`, not separate prints.
- `LoopReport` field additions are backward-compatible (internal struct, not serialized).
- Do not thread `run_id` through error types this phase.
- No major new features (no patch mode, no provider expansion, no git worktree engine).

## Phase 17 Outcomes

Completed outcomes:
- Hardened observability contracts: explicit JSON contract tests for `status --json` and `stats --json`, plus stable human-format coverage for core summary output.
- Added compatibility regression coverage for legacy/defaulted log fields and edge outcomes (`plan_error`, `token_cap`, `cap_reached`, `aborted`).
- Further reduced `loop.rs` decision-surface with pure helper extraction and table tests (terminal outcome mapping, review handling, step progression).
- Added lifecycle progress messaging to stderr across startup, planning, per-step/per-attempt transitions, review outcomes, and resume confirmation.
- Upgraded operator-facing errors with actionable guidance in `LoopError::Display`.
- Enriched completion output with run-level token/request usage and log path reporting via extended `LoopReport`.
- Improved CLI discoverability with operationally explicit help text for cap/strict/dry-run/resume/json flags.

Locked decisions retained in implementation:
- No changes to path safety, transactional apply/rollback semantics, or compatibility defaults.
- No new feature-surface expansions (patch mode, provider expansion, git worktree orchestration, or quiet-mode flag).

## Phase 18 Scope (Planned)

Primary objective:
- Harden observability and accounting integrity introduced in Phase 17.
- Improve operator control over lifecycle output without breaking stdout contracts.
- Continue behavior-preserving orchestration maintainability work.

Locked deliverables:
1. **Accounting invariant hardening**: isolate and verify request/token accounting transitions under all terminal paths.
2. **Precise failure log pointers**: improve run/resume error guidance with exact per-run log location when available.
3. **Lifecycle output control**: add `--quiet` for run/resume to suppress cosmetic progress messages only.
4. **Command output contract tests**: protect stdout/stderr behavior for human and `--json` modes.
5. **Orchestration extraction**: continue pure-helper decomposition in `loop.rs` without semantic drift.

Design decisions locked for Phase 18:
- No patch mode, provider expansion, or git worktree orchestration engine this phase.
- `--quiet` must never suppress errors; it only gates cosmetic lifecycle messages.
- Stdout remains clean for command output and machine-readable JSON payloads.
- Request counting semantics remain strict: one logical plan/edit LLM call equals one request.
