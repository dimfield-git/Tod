# Tod
<img width="1536" height="1024" alt="ChatGPT Image Feb 22, 2026, 01_15_18 AM" src="https://github.com/user-attachments/assets/6f44d7c4-0bf3-4198-ab92-cfdf3417d28e" />

A minimal Rust coding agent. Give it a goal, it plans the work, writes edits, runs checks, and iterates until each step is complete or a cap is reached.

## How it works

Tod operates as a sequential loop:

```text
goal -> planner -> editor -> runner -> reviewer -> repeat or done
```

1. Planner creates ordered implementation steps.
2. Editor generates JSON edit batches (`write_file`, `replace_range`) for one step.
3. Runner applies edits and executes quality checks.
4. Reviewer decides to proceed, retry with error context, or abort.

The LLM never touches the filesystem directly. All writes are validated and applied by local Rust code.

## Architecture

```text
src/
  main.rs       entry point and CLI command dispatch
  loop.rs       orchestration loop and run caps
  schema.rs     edit schema, JSON extraction, path and batch validation
  config.rs     run mode and loop/runtime limits
  cli.rs        clap CLI and RunConfig conversion
  llm.rs        LlmProvider trait and Anthropic provider
  planner.rs    plan creation prompt and semantic plan validation
  editor.rs     edit creation prompt and batch generation
  runner.rs     transactional edit apply and cargo pipeline execution
  reviewer.rs   proceed/retry/abort decision logic
```

## Safety and reliability guarantees

- Relative-path sandbox checks with traversal rejection.
- Symlink-aware path escape guard for existing ancestors.
- Edit batch semantic validation:
  - duplicate `write_file` to same path rejected
  - `write_file` + `replace_range` on same path rejected
  - overlapping `replace_range` segments rejected
- Transactional edit apply with rollback on failure.
- Strict mode is non-mutating (`cargo fmt --check`).
- Loop enforces both per-step and total-iteration caps.
- Runner output is size-capped before retry feedback.

## CLI

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-..."

# Run the agent
cargo run -- run --project /path/to/project "your goal here"

# Strict checks (fmt --check, clippy -D warnings, test)
cargo run -- run --strict "your goal here"

# Validate flow without writes or cargo invocations
cargo run -- run --dry-run "your goal here"
```

`--max-iters` is validated and must be `>= 1`.

`init`, `resume`, and `status` still print placeholders.

## Test and lint

```bash
cargo test
cargo clippy -- -D warnings
```

Current test status: 91 passing, 1 ignored (live API smoke test).

## Change documentation

See `docs/changes-2026-02-23.md` for a detailed breakdown of the loop wiring, safety hardening, and validation updates.
