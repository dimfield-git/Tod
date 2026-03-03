# AGENTS.md — Tod

## Operating Principles

- **Suggest first, then wait.** Propose changes as diffs. Do not apply until approved. Always show a diff or exact file/line patch when proposing implementation — no prose-only suggestions.
- **Do not implement in the same response as proposing a diff.** Wait for approval, then implement.
- **When proposing changes, provide either a `git diff` style patch or a complete file replacement block.**
- **Do not run commands without asking.** Suggest exact commands and wait for confirmation.
- Prefer small, reviewable diffs — one logical change per task.
- Follow existing patterns in the codebase. Do not invent new conventions.
- Do not refactor, rename, or reorganize unless explicitly asked.
- Do not add crate dependencies without approval.
- Do not invent new public functions or types to make something compile. If unsure whether something exists, search the repo first or ask.
- Preserve all existing tests unless a change explicitly requires modification.
- When multiple approaches exist, state the tradeoff and recommend one.
- **One phase at a time.** Do not work across phase boundaries. Complete and verify the current phase before starting the next. If a requested change touches files outside the current phase scope, stop and propose the minimal exception set before editing.
- **Priority order:** `PHASE13.md` → Future phases.
- **Per-task done:** Each change must include tests added/updated if applicable, a suggested verification step, and updates to docs/README/examples if CLI surface changed.

## Repo Identity

Tod is a minimal Rust coding agent that operates from the terminal. It plans work via LLM, generates JSON edit batches, validates and applies them transactionally, runs cargo pipelines, and iterates until success or cap.

**"Done" means:** `cargo test` passes (baseline: 178 passing, 1 ignored), `cargo clippy -- -D warnings` clean, binary runs.

Linux-only. No GUI dependencies. Phases 1–13 complete.

Core design principle: **"LLM generates, everything else constrains."**

## Project Map

```
src/
  main.rs       Entry point, CLI dispatch (run, resume, status, stats, init), provider init
  cli.rs        clap derive CLI, argument-to-config conversion (resume: --project, --force)
  config.rs     RunConfig, RunMode — immutable after construction
  context.rs    Context building + byte budgets (planner, step, retry), collect_paths, truncation
  editor.rs     Edit creation prompt, imports format_file_context from context.rs
  llm.rs        LlmProvider trait, LlmResponse (text + usage), Anthropic impl (ureq, blocking), retry with backoff
  loop.rs       Orchestration, RunState/StepState, fingerprint, checkpoint, logging, resume
  planner.rs    Plan creation prompt, plan semantic validation
  reviewer.rs   Proceed / Retry / Abort decision logic (pure, no LLM)
  runner.rs     Transactional edit apply, cargo pipeline execution
  schema.rs     EditAction types, JSON extraction, path + batch validation
  stats.rs      Read-only analysis of .tod/ logs, per-run and cross-run metrics
  util.rs       Shared helpers: safe_preview, warn! macro
  test_util.rs  Shared TempSandbox for tests (#[cfg(test)] only)

docs/
  tod-architecture.html   Interactive module diagram (GitHub Pages)
  loop-design-final.md    Loop design rationale, state struct docs
  live-run-log.md         Phase 9 live run transcript and outcomes
  phase6-design.md        Phase 6 design document (logging, checkpoint, resume)
  changes-2026-02-23.md   Detailed change log for loop wiring session
  codebase-assessment.md  Post-Phase-10 architecture and correctness analysis
  strategic-plan.md       Prioritized remaining work and phase roadmap
```

**Runtime output directory** (created by Tod when running against a target project):

```
<project_root>/.tod/
  state.json                          RunState checkpoint (overwritten atomically each time)
  logs/<run_id>/
    plan.json                         Written once after planning (includes usage data from Phase 11+)
    final.json                        Written once on run exit (outcome, step, message)
    step_N_attempt_M.json             One per edit→apply→run→review cycle
```

Tests are inline (`#[cfg(test)] mod tests`) in each module. Shared test utilities in `test_util.rs` (crate-level `#[cfg(test)]` module).

## Architectural Invariants

- All target-project filesystem mutation goes through `runner.rs`. Core logic in other modules is pure or trait-abstracted.
- All errors are typed enums. No `.unwrap()` in non-test code.
- No global mutable state. All run state lives in `RunState` / `StepState` structs.
- No async. All LLM calls are blocking via `ureq`. Tokio is explicitly excluded.
- `SYSTEM_PROMPT` constants in `planner.rs` and `editor.rs` are **read-only**. Do not modify these unless explicitly asked — they are product logic, not ordinary strings.
- Do not change CLI flag names or semantics without approval.
- Do not change JSON schema tags (`write_file`, `replace_range`) without approval.
- Edit application is transactional: snapshot before mutation, rollback on any failure.
- Path safety: relative-only, no `..`, no absolute, symlink-aware escape guard. Project root comes from `RunConfig.project_root` (set via CLI `--project`).
- Resume must not issue LLM calls when checkpoint usage already meets or exceeds the token cap.
- Stats request counts reflect all billed API calls (planner + editor). Planner usage is recorded in `plan.json`. Legacy logs without planner usage are handled gracefully.
- Every run exit after planning produces `final.json` with an explicit terminal outcome. Stats uses `final.json` as source of truth when present and falls back to heuristic inference for legacy logs.
- Checkpoint fingerprint must reflect workspace state at the moment of checkpoint write, not an earlier snapshot. `checkpoint()` is `&self`; callers refresh fingerprint before persisting.
- Resume must use the originating run's execution profile (mode, dry-run, runner output cap) when a stored profile exists. Legacy checkpoints without a profile fall back to caller defaults.
- Fingerprints are versioned. New checkpoints write v2 content-aware hashes; legacy v1 checkpoints remain resumable via compatibility checks (file count + total bytes) until rewritten.
- Run IDs include fractional seconds and defend against directory collisions by suffixing (`_2`, `_3`, ...), while preserving lexical recency sorting for stats.

## Phase History

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Scaffolding — CLI, config, schema, path validation | ✅ Done |
| 2 | LLM integration — provider trait, Anthropic impl, planner JSON extraction | ✅ Done |
| 3 | Editor — edit generation, WriteFile + ReplaceRange apply | ✅ Done |
| 4 | Runner — cargo pipeline, output capture, truncation | ✅ Done |
| 5 | Loop — full orchestration, retry, abort, iteration caps | ✅ Done |
| 6 | Logging — .tod/ directory, checkpoint, attempt logs, resume | ✅ Done |
| 7 | Strict mode — gated cargo fmt/clippy, reviewer logic | ✅ Done |
| 8 | Hardening — TempSandbox extraction, atomic checkpoint, token budget, context caps | ✅ Done |
| 9 | Working prototype — live run validation, context.rs, retry backoff, init command | ✅ Done |
| 10 | External usability — naming consistency, --project flag, shared utilities, structured errors, LICENSE | ✅ Done |
| 11 | Reliability accounting — pre-resume token cap guard, planner usage in plan.json, stats request count fix, field rename | ✅ Done |
| 12 | Failure observability — terminal outcome log, pre-runner error logging, stats outcome fidelity | ✅ Done |
| 13 | Resume determinism — checkpoint fingerprint freshness, execution profile persistence, content-aware drift detection, run ID hardening | ✅ Done |
