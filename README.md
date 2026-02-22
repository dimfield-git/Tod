# Tod

A minimal Rust coding agent. Give it a goal, it plans the work, writes the edits, runs the tests, and iterates until the build is green.

## How it works

Tod operates as a sequential loop:

```
goal → planner → editor → runner → reviewer → repeat (or done)
```

1. **Planner** — takes a goal and project context, produces an ordered list of concrete steps
2. **Editor** — takes a single step and current file contents, produces validated file edits (`WriteFile` or `ReplaceRange`)
3. **Runner** — applies edits to disk, runs `cargo build` / `cargo test` (or `fmt` + `clippy` in strict mode)
4. **Reviewer** — inspects the result, decides: proceed to next step, retry with error context, or abort

The LLM never touches the filesystem directly. Every edit is parsed from JSON, validated against a path jail and size limits, then applied by Tod's own code.

## Architecture

```
src/
  main.rs       — entry point, mod declarations
  schema.rs     — EditAction enum, EditBatch, validation, JSON extraction
  config.rs     — RunMode (default/strict), RunConfig, iteration limits
  cli.rs        — clap-based CLI: init, run, status, resume
  llm.rs        — LlmProvider trait, AnthropicProvider (blocking HTTP via ureq)
  planner.rs    — system prompt + Plan/PlanStep types
  editor.rs     — system prompt + EditBatch generation from plan steps
  runner.rs     — edit application + cargo pipeline execution
  reviewer.rs   — pure-logic decision: proceed / retry / abort
  loop.rs       — orchestration (not yet built)
```

## Design decisions

- **Blocking, not async** — the agent loop is sequential. ureq over reqwest, no tokio.
- **Validation separate from deserialization** — serde checks JSON shape, validation checks safety (path traversal, size limits, range bounds).
- **LLM never sees the filesystem** — the loop reads files and passes context in; the loop writes edits out.
- **Sonnet for speed** — fast and cheap enough to iterate. Model is configurable.
- **No retry in the provider** — retry logic belongs in the loop, not the HTTP layer.
- **Pure-logic reviewer** — no LLM call. Success → proceed, failure under cap → retry, failure at cap → abort. Keeps it simple, saves tokens.
- **Truncated runner output** — compiler errors capped at 4 KiB (configurable), snapped to nearest line boundary. Keeps the fixer context clean and focused.

## Current status

| Module      | Status   | Tests |
|-------------|----------|-------|
| schema.rs   | complete | 25    |
| config.rs   | complete | 2     |
| cli.rs      | complete | 7     |
| llm.rs      | complete | 3 (+1 ignored smoke test) |
| planner.rs  | complete | 8     |
| editor.rs   | complete | 8     |
| runner.rs   | complete | 14    |
| reviewer.rs | complete | 9     |
| loop.rs     | not started | —  |

77 tests passing, 1 ignored (live API smoke test).

## Usage

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-..."

# Run tests
cargo test

# Run the smoke test (requires live API key)
cargo test smoke_real_api_call -- --ignored
```

CLI commands (stubs until loop.rs is complete):
```bash
cargo run -- init myproject
cargo run -- run --project /path/to/project "your goal here"
cargo run -- run --strict --dry-run "your goal here"
cargo run -- status
```

## Dependencies

- `serde` / `serde_json` — serialization
- `clap` — CLI parsing (derive)
- `ureq` — blocking HTTP client
