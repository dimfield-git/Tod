# Phase 9 Implementation Log (2026-02-28)

This document records all code and validation changes made to implement Phase 9 (`PHASE9.md`) in Tod.

## Scope

Implemented all five Phase 9 tasks in order:
1. End-to-end live run validation
2. Context window management (`context.rs`)
3. LLM retry on transient failure
4. `init` command
5. README + package metadata

Verification was run after each code task boundary with:

```bash
cargo test
cargo clippy -- -D warnings
```

Final verification state:
- `cargo test`: 148 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 1: End-to-end live run

### Validation work

- Created target project at `/home/dim/Agents/tod-test-target` using `cargo init`.
- Ran Tod in dry-run mode against the target goal:
  - `Add a CLI flag --name that takes a string and prints hello, <name>`
- Ran Tod in real mode (non-dry-run) against the same goal.
- Verified target project build and runtime behavior:
  - `cargo build`
  - `cargo run -- --name world`
- Verified run reporting via:
  - `agent status`
  - `agent stats`

### Findings

- Both dry-run and real-run succeeded after allowing networked API calls.
- `.tod/logs/` and `.tod/state.json` were produced with nonzero token usage.
- Generated target behavior worked (`Hello, world!`).
- `status`/`stats` currently operate from current directory rather than accepting `--project`.

### Deliverable

- Added `docs/live-run-log.md` with command transcript, outcomes, and deferred notes.

---

## Task 2: Context window management (`context.rs`)

### Added

- `src/context.rs` (new) with:
  - Context budgets:
    - `MAX_PLANNER_CONTEXT_BYTES = 128 * 1024`
    - `MAX_STEP_CONTEXT_BYTES = 64 * 1024`
    - `MAX_RETRY_CONTEXT_BYTES = 8 * 1024`
    - `MAX_TREE_DEPTH = 12`
    - `MAX_LISTED_FILES = 200`
  - `ContextError`:
    - `Io { path, cause }`
    - `InvalidPath { step_index, path, reason }`
  - Context builders:
    - `build_planner_context(...)`
    - `build_step_context(...)`
    - `build_retry_context(...)`
  - Helpers:
    - `truncate_context(...)`
    - `collect_paths(...)`
    - `format_file_context(...)`
    - internal truncation/fit helpers for UTF-8 and line-safe clipping

### Updated

- `src/loop.rs`
  - Removed in-file context helper implementations.
  - Imported `crate::context`.
  - Replaced:
    - `build_project_context(...)` -> `context::build_planner_context(...)`
    - `build_step_file_context(...)` -> `context::build_step_context(...)`
  - Replaced inline retry prefixing with `context::build_retry_context(...)`.
  - Added `impl From<ContextError> for LoopError`.
  - Kept `MAX_TREE_DEPTH` usage in fingerprint walker via `context::MAX_TREE_DEPTH`.

- `src/editor.rs`
  - Moved `format_file_context` implementation out to `context.rs`.
  - Imports `format_file_context` from `context` and keeps it referenced.
  - Removed duplicated formatter test from editor tests.

- `src/main.rs`
  - Added `mod context;`

### Added tests (`src/context.rs`)

- `planner_context_within_budget`
- `planner_context_truncates_large_tree`
- `step_context_within_budget`
- `step_context_truncates_large_files`
- `retry_context_truncates`
- `retry_context_prefixes_header`
- `format_file_context_numbers_lines` (moved coverage)
- `collect_paths_excludes_hidden_dirs`

### Result

- Context building is centralized in `context.rs` with explicit byte budgets.
- Planner/step/retry context growth is bounded and UTF-8 safe.

---

## Task 3: Retry on LLM transient failure

### Updated (`src/llm.rs`)

- Added retry policy constants:
  - `MAX_RETRIES = 3`
  - `INITIAL_BACKOFF_MS = 1000`
- Added retry classification helper:
  - `is_retryable_status(status)` for `429`, `500`, `502`, `503`
- Added backoff/jitter helpers:
  - `pseudo_random_offset(...)`
  - `sleep_with_jitter(attempt)`
- Updated `AnthropicProvider::complete(...)`:
  - Wraps request/response flow in retry loop.
  - Retries on network send failures (up to max retries).
  - Retries on response-read failures (up to max retries).
  - Retries on retryable HTTP statuses (up to max retries).
  - Preserves existing non-retryable API error behavior.
  - Emits retry warnings via `eprintln!`.

### Added tests (`src/llm.rs`)

- `is_retryable_429`
- `is_retryable_500`
- `not_retryable_400`
- `not_retryable_401`

### Result

- Transient LLM transport/API failures are handled inside provider logic with backoff.

---

## Task 4: `init` command

### Updated (`src/main.rs`)

- Replaced `init` stub with real command flow:
  - `Command::Init { name }` now calls `init_project(&name)`.
  - Prints success message or exits with error message.
- Added:
  - `init_project(name: &str) -> Result<(), String>`
    - delegates to `cargo init <name>`
    - surfaces command stderr on failure
    - appends `.tod/` to generated `.gitignore`
  - `append_tod_gitignore(project_dir: &Path) -> Result<(), String>`
    - avoids duplicate `.tod/` entries
    - preserves newline handling

### Added tests (`src/main.rs`)

- `init_creates_project`
- `init_adds_tod_to_gitignore`
- `init_does_not_duplicate_gitignore_entry`
- `init_fails_on_existing_dir`

### Result

- `tod init <name>` now scaffolds usable project roots and ignores runtime state by default.

---

## Task 5: README + Cargo.toml metadata

### Updated

- `Cargo.toml`
  - package `name`: `agent` -> `tod`
  - added:
    - `description`
    - `license`
    - `repository`

- `README.md`
  - Replaced with concise prototype-level documentation including:
    - what Tod is
    - quick start
    - command list (`init`, `run`, `resume`, `status`, `stats`)
    - env configuration (`ANTHROPIC_API_KEY`, `TOD_MODEL`, `TOD_RESPONSE_MAX_TOKENS`)
    - run flags (`--strict`, `--max-iters`, `--max-tokens`, `--dry-run`, `--project`)
    - abbreviated project structure
    - prototype status statement

### Result

- Project metadata and top-level docs are now prototype-ready for first-time use.

---

## Notes

- `src/runner.rs` has one formatting-only change introduced by `cargo fmt` during verification (no behavior change).
- Task order and phase boundary constraints were preserved throughout.
