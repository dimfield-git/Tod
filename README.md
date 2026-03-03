# Tod

Tod is a minimal Rust coding agent that plans work with an LLM, generates JSON edit batches, validates and applies edits transactionally, runs cargo quality pipelines, and iterates until success or configured caps.

Core design principle: LLM generates intent; deterministic Rust code constrains execution.

- [Tod Architecture Map](docs/webpages/tod-architecture-map_v1.0.html)
- [Tod Architecture Map — Alternative Edition](docs/webpages/tod-mindmap_v1.1.html)

## Current Status

- Phases 1-17 complete
- Baseline validation: `cargo test` (`215 passed, 1 ignored`) and `cargo clippy -- -D warnings` clean

## Requirements

- Rust toolchain (cargo + rustc)
- Linux-first workflow (terminal-only)
- `ANTHROPIC_API_KEY` for `run`/`resume`

## Quick Start

```bash
cargo build
export ANTHROPIC_API_KEY="sk-..."

# scaffold a target project
cargo run -- init myproject

# run Tod against that project
cargo run -- run --project ./myproject "Add a CLI flag --name and print hello, <name>"

# inspect latest run
cargo run -- status --project ./myproject
cargo run -- status --project ./myproject --json
cargo run -- stats --project ./myproject --last 5
cargo run -- stats --project ./myproject --last 5 --json
```

During `run`/`resume`, Tod emits lifecycle progress to stderr (startup, plan, step/attempt/review, resume confirmation). Stdout remains reserved for command output and `--json` payloads.

## Commands

- `init <name>`
- `run [--project <path>] [--strict] [--max-iters <N>] [--dry-run] [--max-tokens <N>] <goal>`
- `resume [--project <path>] [--force]`
- `status [--project <path>] [--json]`
- `stats [--project <path>] [--last <N>] [--json]`

Operator guidance:

- See `docs/runbook.md` for mode selection, cap tuning, resume/force usage, and failure recovery actions.

### Run flags

- `--project <path>`: target project root (default `.`)
- `--strict`: pipeline is `cargo fmt --all --check`, `cargo clippy -- -D warnings`, then `cargo test`
- default mode pipeline: `cargo build`, then `cargo test`
- `--max-iters <N>`: max iterations per step (`N >= 1`)
- `--dry-run`: generate/validate/log edits without writing files or running cargo
- `--max-tokens <N>`: global token cap (`input + output`, `0` disables cap)

## Runtime Artifacts

Tod writes runtime state under the target project's `.tod/` directory.

```text
<project_root>/.tod/
  state.json
  logs/<run_id>/
    plan.json
    final.json
    step_<n>_attempt_<m>.json
```

Artifact semantics:

- `plan.json`: plan snapshot after planner success
- `step_<n>_attempt_<m>.json`: per-attempt edit/apply/run/review log
- `final.json`: terminal outcome source of truth when present
- `state.json`: checkpoint for `resume`

`stats` compatibility behavior:

- Prefers `final.json` for outcome/message
- Falls back to attempt-log inference for legacy runs without `final.json`
- Supports plan-error runs that only have `final.json` (no `plan.json`)

## Resume and Compatibility

- Checkpoints persist run execution profile (`mode`, `dry_run`, `max_runner_output_bytes`)
- `resume` reuses checkpoint profile when present
- Fingerprints are versioned:
  - v1 legacy: `(path,size)` hash
  - v2 current: content-aware hash
- Legacy checkpoints remain resumable; v1->v2 same-size drift caveat is preserved
- Run IDs are timestamp-based and collision-safe with suffixes (`_2`, `_3`, ...)

## Environment Variables

- `ANTHROPIC_API_KEY` (required for LLM-backed commands)
- `TOD_MODEL` (optional, default `claude-sonnet-4-5-20250929`)
- `TOD_RESPONSE_MAX_TOKENS` (optional, default `4096`)

## Architecture

```text
src/
  main.rs         entry point + command dispatch (run/resume/status/stats/init)
  cli.rs          clap command model + run config conversion
  config.rs       run configuration types
  context.rs      planner/step/retry context building + budget enforcement
  planner.rs      plan prompt + plan validation
  editor.rs       edit prompt + edit batch generation
  schema.rs       edit schema + JSON extraction + path/range/batch validation
  runner.rs       transactional edit apply + cargo stage execution
  reviewer.rs     proceed/retry/abort policy
  llm.rs          LLM provider trait + Anthropic implementation + retries
  log_schema.rs   log structs + serde defaults (types only)
  loop_io.rs      persistence primitives + run identity allocation
  loop.rs         orchestration state machine + checkpointing + resume
  stats.rs        read-only run/log summarization and formatting
  util.rs         shared warning + UTF-8-safe preview helper
  test_util.rs    shared temp sandbox helper (tests only)
```

## Development

Run local quality gates:

```bash
cargo test
cargo clippy -- -D warnings
```

Interactive architecture reference:

- `docs/tod-architecture.html`
