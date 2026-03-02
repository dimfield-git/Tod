# AGENTS.md — Tod

## Operating Principles

- **Suggest first, then wait.** Propose changes as diffs. Do not apply until approved. Always show a diff or exact file/line patch when proposing implementation — no prose-only suggestions.
- **Do not run commands without asking.** Suggest exact commands and wait for confirmation.
- Prefer small, reviewable diffs — one logical change per task.
- Follow existing patterns in the codebase. Do not invent new conventions.
- Do not refactor, rename, or reorganize unless explicitly asked.
- Do not add crate dependencies without approval.
- Do not invent new public functions or types to make something compile. If unsure whether something exists, search the repo first or ask.
- Preserve all existing tests unless a change explicitly requires modification.
- When multiple approaches exist, state the tradeoff and recommend one.
- **One phase at a time.** Do not work across phase boundaries. Complete and verify the current phase before starting the next. If a requested change touches files outside the current phase scope, stop and ask before proceeding.
- **Priority order:** Current instructions (see [`PHASE11.md`](PHASE11.md)) → Future phases.
- **Per-task done:** Each change must include tests added/updated if applicable, a suggested verification step, and updates to docs/README/examples if CLI surface changed.

## Repo Identity

Tod is a minimal Rust coding agent that operates from the terminal. It plans work via LLM, generates JSON edit batches, validates and applies them transactionally, runs cargo pipelines, and iterates until success or cap.

**"Done" means:** `cargo test` passes (baseline: 160 passing, 1 ignored), `cargo clippy -- -D warnings` clean, binary runs.

Linux-only. No GUI dependencies. Phases 1–11 complete.

Core design principle: **"LLM generates, everything else constrains."**

## Phases

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Core architecture skeleton — module layout, core types (`RunConfig`, `EditAction`, `RunState`) | ✅ Done |
| 2 | JSON edit schema — `WriteFile`/`ReplaceRange` actions, path validation, content size limits | ✅ Done |
| 3 | LLM layer — `LlmProvider` trait, Anthropic implementation, JSON extraction with fence/preamble handling | ✅ Done |
| 4 | Execution loop — plan → edit → apply → run → review cycle, iteration caps | ✅ Done |
| 5 | Runner — cargo pipeline execution, transactional edit apply with rollback, strict mode (`fmt --check` + clippy) | ✅ Done |
| 6 | Logging & reproducibility — `.tod/` directory, `state.json` checkpoint, structured attempt/plan logs, workspace fingerprint, resume with drift detection, status command | ✅ Done |
| 7 | Observability — `stats.rs` module, read-only analysis from structured logs, per-run and cross-run metrics, CLI `stats` command | ✅ Done |
| 8 | Hardening + budget enforcement — TempSandbox extraction, atomic checkpoints, explicit truncation flag, provider config via env, token tracking + cap | ✅ Done |
| 9 | Working prototype — end-to-end live validation, context window management, LLM retry, init command, final packaging | ✅ Done |
| 10 | External usability — naming consistency, `--project` flag for status/stats, shared utilities, structured errors, LICENSE | ✅ Done |
| 11 | Reliability accounting — pre-resume token cap guard, planner usage in plan.json, stats request count fix, field rename | ✅ Done |

**Current instructions: see [`PHASE11.md`](PHASE11.md)**

## Golden Path Commands

```
Build:      cargo build
Test:       cargo test
Lint:       cargo clippy -- -D warnings
Format:     cargo fmt --all --check
Strict:     cargo fmt --all --check → cargo clippy -- -D warnings → cargo test
Run:        cargo run -- run --project /path/to/project "goal"
Dry run:    cargo run -- run --dry-run "goal"
Init:       cargo run -- init <name>
Status:     cargo run -- status
Stats:      cargo run -- stats --last 5
```

No external system dependencies beyond a Rust toolchain.

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
```

**Runtime output directory** (created by Tod when running against a target project):

```
<project_root>/.tod/
  state.json                          RunState checkpoint (overwritten atomically each time)
  logs/<run_id>/
    plan.json                         Written once after planning (includes usage data from Phase 11+)
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
- Path safety: relative-only, no `..`, no absolute, symlink-aware escape guard. Project root comes from `RunConfig.project_root` (set via CLI `--project`). All path validation is relative to that.
- State structs (`RunState`, `StepState`) derive `Serialize` + `Deserialize`. Checkpoint writes to `.tod/state.json` atomically (tmp + rename); resume loads from it. Fingerprint detects workspace drift between runs.
- Context building lives in `context.rs` with explicit byte budgets. Planner context (128 KiB), step context (64 KiB), retry context (8 KiB). All truncation is UTF-8 safe.
- LLM retry (429, 500, 502, 503, network errors) is handled inside `AnthropicProvider::complete()` with exponential backoff + jitter. The orchestration loop never sees transient transport failures.
- Resume must not issue LLM calls when checkpoint usage already meets or exceeds the token cap.
- Stats request counts reflect all billed API calls (planner + editor). Planner usage is recorded in `plan.json`. Legacy logs without planner usage are handled gracefully.
