# Tod

<img width="1536" height="1024" alt="Tod — Minimal Rust Coding Agent" src="https://github.com/user-attachments/assets/6f44d7c4-0bf3-4198-ab92-cfdf3417d28e" />

A minimal Rust coding agent. Give it a goal, it plans the work, writes edits, runs checks, and iterates until each step is complete or a cap is reached.

The LLM never touches the filesystem directly. All writes are validated and applied by local Rust code.

---

## Architecture

Tod operates as a sequential loop with strict separation between generation (LLM) and execution (local Rust):

```
goal → planner → editor → runner → reviewer → repeat or done
```

1. **Planner** asks the LLM for an ordered list of implementation steps.
2. **Editor** asks the LLM for JSON edit batches (`write_file`, `replace_range`) for one step.
3. **Runner** validates and applies edits transactionally, then executes the cargo quality pipeline.
4. **Reviewer** decides — using pure logic, no LLM — whether to proceed, retry with error context, or abort.

The orchestrator (`loop.rs`) drives this cycle, tracks progress in serializable state structs (`RunState` / `StepState`), and checkpoints at every exit path.

**[Interactive module diagram →](https://dimfield-git.github.io/Tod/tod-architecture.html)**

### Module map

```
src/
  main.rs       CLI dispatch, provider init, loop invocation
  loop.rs       orchestration, state management, context building
  schema.rs     edit types, JSON extraction, path and batch validation
  config.rs     RunConfig, RunMode, immutable run settings
  cli.rs        clap CLI definition, argument-to-config conversion
  llm.rs        LlmProvider trait, Anthropic implementation
  planner.rs    plan creation prompt, plan semantic validation
  editor.rs     edit creation prompt, file context formatting
  runner.rs     transactional edit apply, cargo pipeline execution
  reviewer.rs   proceed / retry / abort decision logic
```

---

## Core design principle

**"LLM generates, everything else constrains."**

The model produces plans and edits. Every other component exists to validate, bound, or reject what the model produces. This separation keeps the system deterministic and observable — the LLM is a black box, but the control flow around it is not.

---

## Safety guarantees

**Path safety** — Relative-path sandbox checks with lexical traversal rejection. Symlink-aware escape guard for existing ancestors. No absolute paths, no `..` components.

**Edit validation** — Duplicate `write_file` to same path rejected. Mixed `write_file` + `replace_range` on same path rejected. Overlapping `replace_range` segments rejected. Content size capped at 512 KiB per edit, 20 edits per batch.

**Transactional apply** — All touched files are snapshotted before mutation. On any failure, the entire batch is rolled back to original state.

**Execution** — Only `cargo` commands are executed. Strict mode uses non-mutating `cargo fmt --check`. Both per-step and total iteration caps are enforced. Runner output is size-capped before being fed back to the LLM.

---

## Usage

### Set up

```bash
export ANTHROPIC_API_KEY="sk-..."
```

### Run the agent

```bash
# Default mode (build + test)
cargo run -- run --project /path/to/project "your goal here"

# Strict mode (fmt --check + clippy -D warnings + test)
cargo run -- run --strict "your goal here"

# Dry run — validates flow without writing to disk or running cargo
cargo run -- run --dry-run "your goal here"

# Custom iteration cap
cargo run -- run --max-iters 10 "your goal here"
```

### Other commands

```bash
cargo run -- init <project_name>   # placeholder
cargo run -- resume                # placeholder
cargo run -- status                # placeholder
```

`--max-iters` must be ≥ 1. Total iterations default to 5× per-step cap.

---

## Run modes

| Mode | Pipeline | Use case |
|------|----------|----------|
| **Default** | `cargo build` → `cargo test` | Normal development |
| **Strict** | `cargo fmt --all --check` → `cargo clippy -- -D warnings` → `cargo test` | CI-grade quality |

---

## State and checkpointing

All mutable state lives in two serializable structs:

**RunState** owns the plan, tracks step progress, and copies config-derived caps so checkpoints are self-contained. **StepState** is nested inside RunState and tracks the current step's attempt counter and retry context. It resets cleanly at each step boundary.

Every exit path — success, retry, abort, or iteration cap — checkpoints before returning. The checkpoint method is currently a stub; Phase 6 will wire it to `.tod/state.json` for logging and resume support.

---

## Test and lint

```bash
cargo test
cargo clippy -- -D warnings
```

Current: **97 passing**, 1 ignored (live API smoke test), clippy clean.

---

## Project status

This is a learning project focused on understanding agent control structures, not just running them. Each design decision is documented and the architecture prioritizes observability and determinism over feature breadth.

### What's implemented

- Full agent loop: plan → edit → apply → run → review → iterate
- Tagged enum JSON schema with robust extraction (handles fences, preamble, garbage)
- Transactional edit application with rollback
- Path sandbox with lexical and symlink-aware validation
- Edit batch semantic validation (duplicates, conflicts, overlaps)
- Explicit state structs with Serialize/Deserialize
- Checkpoint stubs at every exit path
- CLI with run mode, iteration caps, dry-run support
- Blocking LLM provider trait with Anthropic implementation

### What's next

- **Phase 6:** Logging & reproducibility — wire checkpoints to disk, per-step log files, enable `resume` and `status` commands
- **Phase 7–8:** Observability — iteration counts, failure causes, correction patterns
- **Future:** Patch mode, git branch isolation, local model support, budget enforcement

---

## Documentation

- [`docs/tod-architecture.html`](https://dimfield-git.github.io/Tod/tod-architecture.html) — Interactive module architecture diagram
- `docs/loop-design-final.md` — loop.rs design rationale and state struct documentation
- `docs/changes-2026-02-23.md` — Detailed change log for the loop wiring session

---

## License

Not yet specified.
