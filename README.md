# Tod

Tod is a minimal Rust coding agent that plans work with an LLM, generates JSON edit batches, validates and applies edits transactionally, runs cargo pipelines, and iterates until success or a configured cap.

## Quick Start

```bash
cargo build
export ANTHROPIC_API_KEY="sk-..."
cargo run -- init myproject
cargo run -- run --project ./myproject "Add a CLI flag --name and print hello, <name>"
```

## Commands

- `init <name>`: scaffold a new Rust project via `cargo init` and add `.tod/` to `.gitignore`.
- `run [FLAGS] --project <path> "<goal>"`: run a new agent session for a goal.
- `resume --project <path> [--force]`: continue from `.tod/state.json`.
- `status`: show summary for the latest run in the current project.
- `stats [--last N]`: summarize recent run history from `.tod/logs/`.

## Configuration

- `ANTHROPIC_API_KEY`: required API key for Anthropic.
- `TOD_MODEL`: optional model override (default `claude-sonnet-4-5-20250929`).
- `TOD_RESPONSE_MAX_TOKENS`: optional provider response cap (default `4096`).

## Run Flags

- `--strict`: run `cargo fmt --all --check`, `cargo clippy -- -D warnings`, then `cargo test`.
- `--max-iters <N>`: max iterations per plan step.
- `--max-tokens <N>`: token budget cap (`0` disables cap).
- `--dry-run`: generate and validate edits without filesystem mutation or cargo execution.
- `--project <path>`: target project root.

## Project Structure

```text
src/
  main.rs       entry point, CLI dispatch, provider init
  loop.rs       orchestration, state/checkpoint/logging, resume
  context.rs    planner/step/retry context building with byte budgets
  planner.rs    plan prompt and semantic validation
  editor.rs     edit prompt and edit generation
  runner.rs     transactional edit apply + cargo pipeline execution
  reviewer.rs   proceed/retry/abort decision logic
  schema.rs     edit schema, extraction, path and batch validation
  llm.rs        provider trait + Anthropic implementation
  cli.rs        clap CLI definitions and argument conversion
  config.rs     run configuration types
  stats.rs      read-only run history analysis
  test_util.rs  shared test sandbox helpers
```

## Status

Prototype: Phases 1-9 complete.
