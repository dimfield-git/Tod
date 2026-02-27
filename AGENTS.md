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
- **Priority order:** Current phase (see `PHASE8.md`) → Future phases.
- **Per-task done:** Each change must include tests added/updated if applicable, and a suggested verification step.

## Repo Identity

Tod is a minimal Rust coding agent that operates from the terminal. It plans work via LLM, generates JSON edit batches, validates and applies them transactionally, runs cargo pipelines, and iterates until success or cap.

**"Done" means:** `cargo test` passes (baseline: 121 passing, 1 ignored), `cargo clippy -- -D warnings` clean, binary runs.

Linux-only. No GUI dependencies. Phases 1–7 complete. Phase 8 (hardening + budget enforcement) is next.

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
| 8 | Hardening + budget enforcement — TempSandbox extraction, atomic checkpoints, explicit truncation flag, provider config via env, token tracking + cap | **Next** |
| 9 | Future extensions — patch mode, git branch isolation, local model support, budget enforcement by cost | Not started |

**Current phase instructions: see [`PHASE8.md`](PHASE8.md)**

## Golden Path Commands

```
Build:      cargo build
Test:       cargo test
Lint:       cargo clippy -- -D warnings
Format:     cargo fmt --all --check
Strict:     cargo fmt --all --check → cargo clippy -- -D warnings → cargo test
Run:        cargo run -- run --project /path/to/project "goal"
Dry run:    cargo run -- run --dry-run "goal"
```

No external system dependencies beyond a Rust toolchain.

## Project Map

```
src/
  main.rs       Entry point, CLI dispatch (run, resume, status, stats), provider init
  loop.rs       Orchestration, RunState/StepState, fingerprint, checkpoint, logging, resume
  schema.rs     EditAction types, JSON extraction, path + batch validation
  config.rs     RunConfig, RunMode — immutable after construction
  cli.rs        clap derive CLI, argument-to-config conversion (resume: --project, --force)
  llm.rs        LlmProvider trait, Anthropic implementation (ureq, blocking)
  planner.rs    Plan creation prompt, plan semantic validation
  editor.rs     Edit creation prompt, format_file_context()
  runner.rs     Transactional edit apply, cargo pipeline execution
  reviewer.rs   Proceed / Retry / Abort decision logic (pure, no LLM)
  stats.rs      Read-only analysis of .tod/ logs, per-run and cross-run metrics

docs/
  tod-architecture.html   Interactive module diagram (GitHub Pages)
  loop-design-final.md    Loop design rationale, state struct docs
  phase6-design.md        Phase 6 design document (logging, checkpoint, resume)
  changes-2026-02-23.md   Detailed change log for loop wiring session
```

**Runtime output directory** (created by Tod when running against a target project):

```
<project_root>/.tod/
  state.json                          RunState checkpoint (overwritten atomically each time)
  logs/<run_id>/
    plan.json                         Written once after planning
    step_N_attempt_M.json             One per edit→apply→run→review cycle
```

Tests are inline (`#[cfg(test)] mod tests`) in each module. Shared test utilities in `test_util.rs` (crate-level `#[cfg(test)]` module).

## Architectural Invariants

- All I/O goes through `runner.rs`. Core logic in other modules is pure or trait-abstracted.
- All errors are typed enums via `thiserror`. No `.unwrap()` in non-test code.
- No global mutable state. All run state lives in `RunState` / `StepState` structs.
- No async. All LLM calls are blocking via `ureq`. Tokio is explicitly excluded.
- `SYSTEM_PROMPT` constants in `planner.rs` and `editor.rs` are **read-only**. Do not modify these unless explicitly asked — they are product logic, not ordinary strings.
- Do not change CLI flag names or semantics without approval.
- Do not change JSON schema tags (`write_file`, `replace_range`) without approval.
- Edit application is transactional: snapshot before mutation, rollback on any failure.
- Path safety: relative-only, no `..`, no absolute, symlink-aware escape guard. Project root comes from `RunConfig.project_root` (set via CLI `--project`). All path validation is relative to that.
- State structs (`RunState`, `StepState`) derive `Serialize` + `Deserialize`. Checkpoint writes to `.tod/state.json`; resume loads from it. Fingerprint detects workspace drift between runs.
- LLM provider trait returns `LlmResponse` (text + optional usage). Loop is the sole accumulator of token usage in `RunState`.

## Coding Standards

- Rust 2021 edition. No MSRV constraint.
- `cargo fmt` is non-negotiable. Run before any commit.
- Clippy with `-D warnings`. No allowed lints without justification.
- `unsafe` is forbidden.
- Typed errors (`thiserror`) everywhere. `anyhow` is not used.
- Blocking HTTP only (`ureq`). No async runtime.
- Test helpers use shared `TempSandbox` from `test_util.rs` with RAII `Drop` guard for cleanup.

## Testing Policy

- Every change must leave `cargo test` at ≥ 121 passing, 0 failing.
- Every new public function gets at least one test.
- Tests live in `#[cfg(test)] mod tests` at the bottom of each module.
- Use `TempSandbox` from `crate::test_util` for filesystem tests.
- The one ignored test (`llm.rs` live API smoke) stays ignored in normal runs.
- No network calls from tests except the ignored smoke test.

## Safety Boundaries

- Never log or print API keys or tokens.
- Only `cargo` commands may be executed by the runner.
- Do not weaken path sandbox checks to make something work.
- No new network-calling code outside `llm.rs`.

## Environment

- IDE: RustRover (JetBrains)
- OS: Linux
- Shell: bash
- Repo location: `~/Agents/Tod/`
- LLM provider: Anthropic (Claude) via `ANTHROPIC_API_KEY` env var
- Provider config: `TOD_MODEL` (default: `claude-sonnet-4-5-20250929`), `TOD_RESPONSE_MAX_TOKENS` (default: `4096`)
- Key dependencies: `ureq` (HTTP), `serde`/`serde_json` (JSON), `clap` (CLI), `sha2` (fingerprint), `chrono` (timestamps)
