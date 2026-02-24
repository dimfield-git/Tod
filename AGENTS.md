# AGENTS.md — Tod

## Operating Principles

- **Suggest first, then wait.** Propose changes as diffs. Do not apply until approved.
- **Do not run commands without asking.** Suggest `cargo test` or `cargo clippy` and wait for confirmation.
- Prefer small, reviewable diffs — one logical change per task.
- Follow existing patterns in the codebase. Do not invent new conventions.
- Do not refactor, rename, or reorganize unless explicitly asked.
- Do not add crate dependencies without approval.
- Preserve all existing tests unless a change explicitly requires modification.
- When multiple approaches exist, state the tradeoff and recommend one.
- **One phase at a time.** Do not work across phase boundaries. Complete and verify the current phase before starting the next.

## Repo Identity

Tod is a minimal Rust coding agent that operates from the terminal. It plans work via LLM, generates JSON edit batches, validates and applies them transactionally, runs cargo pipelines, and iterates until success or cap.

**"Done" means:** `cargo test` passes (baseline: 97 passing, 1 ignored), `cargo clippy -- -D warnings` clean, binary runs.

Linux-only. No GUI dependencies. Currently in active development — Phase 6 (logging & reproducibility) is next.

Core design principle: **"LLM generates, everything else constrains."**

## Golden Path Commands

```
Build:      cargo build
Test:       cargo test
Lint:       cargo clippy -- -D warnings
Run:        cargo run -- run --project /path/to/project "goal"
Strict:     cargo run -- run --strict "goal"
Dry run:    cargo run -- run --dry-run "goal"
```

No external system dependencies beyond a Rust toolchain.

## Project Map

```
src/
  main.rs       Entry point, CLI dispatch, provider init
  loop.rs       Orchestration, RunState/StepState, context building
  schema.rs     EditAction types, JSON extraction, path + batch validation
  config.rs     RunConfig, RunMode — immutable after construction
  cli.rs        clap derive CLI, argument-to-config conversion
  llm.rs        LlmProvider trait, Anthropic implementation (ureq, blocking)
  planner.rs    Plan creation prompt, plan semantic validation
  editor.rs     Edit creation prompt, format_file_context()
  runner.rs     Transactional edit apply, cargo pipeline execution
  reviewer.rs   Proceed / Retry / Abort decision logic (pure, no LLM)

docs/
  tod-architecture.html   Interactive module diagram (GitHub Pages)
  loop-design-final.md    Loop design rationale, state struct docs
  changes-2026-02-23.md   Detailed change log for loop wiring session
```

Tests are inline (`#[cfg(test)] mod tests`) in each module. No separate integration test crate yet.

## Architectural Invariants

- All I/O goes through `runner.rs`. Core logic in other modules is pure or trait-abstracted.
- All errors are typed enums via `thiserror`. No `.unwrap()` in non-test code.
- No global mutable state. All run state lives in `RunState` / `StepState` structs.
- No async. All LLM calls are blocking via `ureq`. Tokio is explicitly excluded.
- `SYSTEM_PROMPT` constants in `planner.rs` and `editor.rs` are **read-only**. Do not modify these unless explicitly asked — they are product logic, not ordinary strings.
- Edit application is transactional: snapshot before mutation, rollback on any failure.
- Path safety: relative-only, no `..`, no absolute, symlink-aware escape guard.
- State structs (`RunState`, `StepState`) derive `Serialize` + `Deserialize` for future checkpoint/resume support.

## Coding Standards

- Rust 2021 edition. No MSRV constraint.
- `cargo fmt` is non-negotiable. Run before any commit.
- Clippy with `-D warnings`. No allowed lints without justification.
- `unsafe` is forbidden.
- Typed errors (`thiserror`) everywhere. `anyhow` is not used.
- Blocking HTTP only (`ureq`). No async runtime.
- Test helpers use RAII `TempSandbox` with `Drop` guard for cleanup. Follow existing pattern in `runner.rs` / `loop.rs`.

## Testing Policy

- Every change must leave `cargo test` at ≥ 97 passing, 0 failing.
- Every new public function gets at least one test.
- Tests live in `#[cfg(test)] mod tests` at the bottom of each module.
- Use `TempSandbox` (RAII temp dir with Drop cleanup) for filesystem tests.
- The one ignored test (`llm.rs` live API smoke) stays ignored in normal runs.
- No network calls from tests except the ignored smoke test.

## Pending Fixes

These are known issues ready for implementation. Each is a bounded, testable task. **Start here before phase work** — use these to calibrate before taking on structural changes.

1. **`truncate_output` UTF-8 panic** (`runner.rs`) — Current implementation can panic on multi-byte UTF-8 boundaries. Switch to byte-level slicing with `is_char_boundary` fallback. Existing test `truncation_handles_multibyte_utf8` covers the case but verify edge cases.

2. **`ReplaceRange` O(n²)** (`runner.rs`) — The current line replacement uses `drain` + insert loop. Replace with `Vec::splice` for single-pass replacement. Already partially addressed but verify the implementation is truly single-operation.

3. **CRLF preservation loss** (`runner.rs`) — `ReplaceRange` may lose CRLF line endings in some edge cases. Add test coverage for round-trip CRLF fidelity and fix if needed.

4. **Test cleanup Drop guard** (`loop.rs`, `runner.rs`) — Ensure all test `TempSandbox` instances use the RAII Drop pattern consistently. Audit for any `fs::remove_dir_all` calls that should be Drop guards instead.

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
