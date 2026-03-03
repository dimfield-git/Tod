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
- `cargo test` passes (baseline: **178 passed, 1 ignored**)
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
- Phases 1–13 complete
- Phase 14 in progress

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
  log_schema.rs   shared log structs (RunnerLog, AttemptLog, PlanLog, FinalLog)
  loop.rs         orchestration state machine + checkpointing + resume + logs
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

Observability invariants:
- Every post-plan terminal path should write `final.json`.
- Planner-stage failures must also write `final.json` with `outcome: "plan_error"`.
- Stats prefers `final.json` as source of truth and falls back for legacy logs.
- Legacy artifacts must remain deserializable via defaults where practical.
- Log schema types live in `log_schema.rs`, not in orchestration modules.

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
| 14 | Observability/schema cohesion and metrics fidelity | In progress |

## Phase 14 Priority (Handoff)

Primary objective for the current phase:
- Decouple log schema from orchestration internals.
- Close planner-stage observability gap (pre-RunState failures emit `final.json`).
- Expand multi-run stats with explicit terminal outcome buckets.
- Harden request counting to logical call semantics, independent of usage-field presence.

Design decisions locked for Phase 14:
- Log schema structs live in `log_schema.rs`. Helper functions for pre-run artifact writing also land there.
- Request counting is logical (1 per intent), not transport-level. Retries are invisible to the count.
- Task 5 (light loop extraction) is conditional: if Tasks 1–2 sufficiently reduce loop.rs pressure, skip it.

Do not expand into major new feature surfaces (patch mode, git isolation, local providers) until this reliability work is complete.
