# AGENTS.md — Tod

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
- `cargo test` passes (baseline: **193 passed, 1 ignored**)
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
- Phases 1–15 complete

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
  log_schema.rs   shared log structs (RunnerLog, AttemptLog, PlanLog, FinalLog) — pure data + serde
  loop_io.rs      persistence primitives, run identity allocation, best-effort JSON writers
  loop.rs         orchestration state machine + checkpointing + resume
  stats.rs        read-only run/log summarization and formatting
  util.rs         shared warning + UTF-8-safe preview helper
  test_util.rs    shared temp sandbox helper (tests only)

docs/
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
- `loop_io.rs` owns persistence primitives and run identity allocation. All writes are best-effort (warn on failure, never propagate). Checkpoint writes use atomic tmp+rename to prevent corruption.
- `loop.rs` owns orchestration flow. Delegates persistence and identity to `loop_io.rs`.

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

Do not expand into major new feature surfaces (patch mode, git isolation, local providers) until this maintainability and compatibility work is complete.
