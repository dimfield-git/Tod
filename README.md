# Tod

<img width="1536" height="1024" alt="Tod — Minimal Rust Coding Agent" src="https://github.com/user-attachments/assets/6f44d7c4-0bf3-4198-ab92-cfdf3417d28e" />

Tod is a minimal Rust coding agent for terminal-driven project changes.
It plans work with an LLM, generates JSON edit batches, validates and applies edits transactionally, runs cargo checks, and iterates until success or a cap is reached.

The LLM never writes files directly. Local Rust code owns validation, filesystem mutation, rollback, and execution.

---

## Architecture

Core loop:

```text
goal -> planner -> editor -> runner -> reviewer -> repeat/done
```

1. `planner.rs` requests an ordered plan (`Plan`, `PlanStep`) from the LLM.
2. `editor.rs` requests a validated `EditBatch` (`write_file` / `replace_range`) for one step.
3. `runner.rs` applies edits transactionally, runs cargo pipeline stages, and reports structured failures.
4. `reviewer.rs` decides `Proceed`, `Retry`, or `Abort` with pure logic.
5. `loop.rs` orchestrates state, checkpoints, attempt logs, retries, caps, and resume.

Module map:

```text
src/
  main.rs       CLI dispatch, provider init
  cli.rs        clap CLI and argument-to-config conversion
  config.rs     RunConfig / RunMode
  llm.rs        LlmProvider trait, Anthropic provider, usage tracking
  planner.rs    plan prompt + semantic validation
  editor.rs     edit prompt + batch validation wiring
  runner.rs     transactional edit apply + cargo execution
  reviewer.rs   deterministic review decision logic
  loop.rs       orchestration, checkpoints, logs, resume, budget cap
  schema.rs     edit schema, extraction, path and batch validation
  stats.rs      run-history analysis and formatting
  test_util.rs  shared TempSandbox test helper (tests only)
```

Interactive architecture diagram:
- https://dimfield-git.github.io/Tod/tod-architecture.html

---

## Safety and constraints

- Relative path jail only (no absolute paths, no `..`, symlink escape guarded).
- Edit batch validation enforces limits and conflict rules.
- Transactional apply with rollback on any failure.
- Runner executes only cargo stages.
- Runner output is size-capped; truncation is tracked explicitly as a boolean.
- Checkpoints are atomic (`state.json.tmp` -> rename to `state.json`).

---

## Configuration

Required env var:

```bash
export ANTHROPIC_API_KEY="sk-..."
```

Optional provider env vars:

```bash
export TOD_MODEL="claude-sonnet-4-5-20250929"
export TOD_RESPONSE_MAX_TOKENS="4096"
```

Token budget cap per run (CLI):

```bash
--max-tokens <u64>
```

`0` means no token cap.

---

## CLI usage

Run:

```bash
# Default mode: cargo build -> cargo test
cargo run -- run --project /path/to/project "your goal"

# Strict mode: fmt --check -> clippy -D warnings -> test
cargo run -- run --strict "your goal"

# Dry run: no disk writes, no cargo execution
cargo run -- run --dry-run "your goal"

# Set per-step cap and token budget
cargo run -- run --max-iters 8 --max-tokens 20000 "your goal"
```

Resume:

```bash
cargo run -- resume --project /path/to/project
cargo run -- resume --project /path/to/project --force
```

Status/stats:

```bash
cargo run -- status
cargo run -- stats --last 5
```

---

## Runtime data layout

```text
<project_root>/.tod/
  state.json
  logs/<run_id>/
    plan.json
    step_N_attempt_M.json
```

`RunState` is checkpointed across exits and resume.
Attempt logs include runner result, truncation flag, and token usage snapshots.

---

## Token tracking and budget enforcement

- Each LLM call returns `LlmResponse { text, usage }`.
- `loop.rs` accumulates usage in `RunState.usage` and counts `RunState.llm_requests`.
- Usage is checkpointed, so resume continues from existing totals.
- When `--max-tokens > 0` and cumulative input+output tokens exceed cap, the run aborts deterministically with `TokenCapExceeded`.

---

## Quality gates

Golden path commands:

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --all --check
```

Current baseline after Phase 8 implementation:
- `cargo test`: 133 passing, 1 ignored, 0 failing
- `cargo clippy -- -D warnings`: clean

---

## Project status

- Phases 1-7: complete
- Phase 8 (hardening + budget enforcement): complete
- Phase 9: future extensions

---

## Documentation

- `PHASE8.md` — Phase 8 task specification
- `docs/phase8-implementation-2026-02-27.md` — Phase 8 implementation change log
- `docs/loop-design-final.md` — orchestration and state design rationale
- `docs/changes-2026-02-23.md` — prior loop wiring change log
- `docs/tod-architecture.html` — interactive architecture view

---

## License

Not yet specified.
