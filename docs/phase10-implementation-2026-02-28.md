# Phase 10 Implementation Log (2026-02-28)

This document records the full implementation of Phase 10: external usability.

Scope from `PHASE10.md`:
- Verify critical Phase 9 carryovers (determinism, naming consistency, token accounting)
- Complete naming and CLI ergonomics polish
- Extract shared utility helpers
- Introduce structured error data for core error surfaces
- Finalize project-facing docs/license hygiene

## 1. Task 1 verification: planner context path sort order

### Objective
Confirm that extracted context builder (`context.rs`) preserves deterministic file ordering from Phase 9.

### Checks
- `grep -n 'sort' src/context.rs`
- inspected `build_planner_context()`

### Result
- `build_planner_context()` already sorts collected paths before rendering:
  - `files.sort();` in `src/context.rs`
- No code change required.

## 2. Task 2 verification: binary name consistency (`agent` vs `tod`)

### Objective
Find remaining references to old binary name `agent`.

### Checks
- `grep -rn '"agent"' src/`
- `grep -rn 'name = "agent"' src/`
- `grep -rn 'agent' docs/live-run-log.md README.md`

### Result
- `src/cli.rs` still used:
  - `#[command(name = "agent", version)]`
  - test argv arrays with `"agent"`
- `docs/live-run-log.md` still referenced:
  - `/target/debug/agent status`
  - `/target/debug/agent stats`
- Verification complete; findings fed directly into Task 4.

## 3. Task 3 verification: `llm_requests` increment timing

### Objective
Confirm `RunState.llm_requests` increments only for successful, usage-bearing completions.

### Checks
- `grep -n 'llm_requests' src/loop.rs`
- inspected increment sites
- `grep -n 'llm_requests' src/llm.rs`

### Result
- Increments occur only inside:
  - `if let Some(usage) = &plan_usage` in `run()`
  - `if let Some(usage) = &call_usage` in `run_from_state()`
- No `llm_requests` logic exists in `llm.rs` retry path.
- No code change required.

## 4. Task 4 implementation: rename CLI identity to `tod`

### Files changed
- `src/cli.rs`
- `docs/live-run-log.md`

### Changes
- Updated clap binary declaration:
  - `#[command(name = "agent", version)]` -> `#[command(name = "tod", version)]`
- Updated all CLI tests to use `"tod"` argv[0]:
  - `parse_run_defaults`
  - `parse_run_strict_with_flags`
  - `parse_run_dry_run`
  - `parse_init`
  - `parse_status`
  - `parse_stats_default`
  - `parse_stats_with_last`
  - `run_config_conversion`
  - `non_run_returns_none`
  - `reject_zero_max_iters`
- Updated live run doc command examples from `target/debug/agent` to `target/debug/tod`.

### Verification
- `cargo test` -> pass (`148 passed`, `1 ignored`)
- `cargo clippy -- -D warnings` -> pass

## 5. Task 5 implementation: add `--project` to `status` and `stats`

### Files changed
- `src/cli.rs`
- `src/main.rs`

### CLI surface changes
- `status` now accepts:
  - `--project <path>` (default `"."`)
- `stats` now accepts:
  - `--project <path>` (default `"."`)
  - `--last <N>` (existing behavior retained)

### Wiring changes
- `Command::Status`:
  - from unit variant to `Status { project: PathBuf }`
  - main dispatch now calls `stats::summarize_current(&project)`
- `Command::Stats`:
  - from `{ last }` to `{ project, last }`
  - main dispatch now resolves `let tod_dir = project.join(".tod");`
  - calls `stats::summarize_runs(&tod_dir, last)`

### Tests updated
- `parse_status`:
  - asserts default `project == PathBuf::from(".")`
  - asserts `--project myproj` parses correctly
- `parse_stats_default`:
  - asserts default project and `last == 5`
- `parse_stats_with_last`:
  - asserts `--last 9`
  - asserts `--project myproj --last 9`

### Verification
- `cargo test` -> pass (`148 passed`, `1 ignored`)
- `cargo clippy -- -D warnings` -> pass

## 6. Task 6 implementation: shared `util.rs` + warning macro

### Files changed
- `src/util.rs` (new)
- `src/main.rs`
- `src/llm.rs`
- `src/schema.rs`
- `src/loop.rs`

### 6a: `safe_preview` deduplication
- Added `safe_preview()` to `src/util.rs`.
- Removed duplicate local `safe_preview()` implementations from:
  - `src/llm.rs`
  - `src/schema.rs`
- Added imports:
  - `use crate::util::safe_preview;` in both modules.

### 6b: `warn!` macro introduction
- Added warning function/macro in `src/util.rs`:
  - `pub fn warn(args: fmt::Arguments)`
  - `#[macro_export] macro_rules! warn`
- Registered module in root (`mod util;` in `main.rs`).
- Replaced `eprintln!("warning: ...")` call sites with `crate::warn!(...)` in:
  - `llm.rs` retry warnings
  - `loop.rs` checkpoint warnings

### New tests
Added in `src/util.rs`:
- `safe_preview_within_limit`
- `safe_preview_truncates`
- `safe_preview_multibyte`

### Verification
- `cargo test` -> pass (`151 passed`, `1 ignored`)
- `cargo clippy -- -D warnings` -> pass

## 7. Task 7 implementation: structured errors (`PathBuf` + `ErrorKind`)

### Files changed
- `src/context.rs`
- `src/loop.rs`
- `src/stats.rs`

### 7a: `ContextError` typed fields
- `ContextError::Io` changed from:
  - `{ path: String, cause: String }`
  - to `{ path: PathBuf, kind: io::ErrorKind, message: String }`
- `ContextError::InvalidPath.path` changed:
  - `String` -> `PathBuf`
- Updated all `ContextError` construction sites accordingly.
- Updated `Display` to render with `path.display()`.

### 7b: `LoopError` typed fields
- `LoopError::Io` changed from:
  - `{ path: String, cause: String }`
  - to `{ path: PathBuf, kind: io::ErrorKind, message: String }`
- `LoopError::InvalidPlanPath.path` changed:
  - `String` -> `PathBuf`
- Updated `From<ContextError> for LoopError` mapping to preserve typed fields.
- Updated `resume()` state parse error mapping to use:
  - `path: state_path.clone()`
  - `kind: io::ErrorKind::InvalidData`
  - `message: ...`
- Updated `Display` formatting to use `path.display()`.

### 7c: `StatsError` typed fields
- `StatsError::Io` changed from:
  - `{ path: String, cause: String }`
  - to `{ path: PathBuf, kind: io::ErrorKind, message: String }`
- `StatsError::InvalidLog.path` changed:
  - `String` -> `PathBuf`
- Updated all `stats.rs` constructors (`read_json`, directory iteration, invalid-state checks).
- Updated `Display` formatting to use `path.display()`.

### Additional lint fix
After first Task 7 pass, `clippy -D warnings` reported:
- `LoopError::Io.kind` never read.

Resolution:
- Updated `Display` pattern matches for all three typed Io variants to bind:
  - `kind: _kind`
- This preserves display output while making typed field explicitly read.

### New tests
- `src/context.rs`:
  - `context_error_io_display`
- `src/loop.rs`:
  - `loop_error_io_display`
  - `context_error_converts_to_loop_error`

### Verification
- `cargo test` -> pass (`154 passed`, `1 ignored`)
- `cargo clippy -- -D warnings` -> pass

## 8. Task 8 implementation: final docs update + MIT license

### Files changed
- `AGENTS.md`
- `README.md`
- `LICENSE` (new)

### `AGENTS.md` updates
- Updated "Done" baseline:
  - `148 passing` -> `154 passing` (with `1 ignored`)
- Updated phase progress:
  - from "Phases 1–9 complete, Phase 10 next"
  - to "Phases 1–10 complete"
- Phase table:
  - Phase 10 status -> `✅ Done`
- Added `util.rs` entry to project map.
- Adjusted phase reference wording to reflect completion state.

### `README.md` updates
- Commands section now reflects new CLI ergonomics:
  - `status [--project <path>]`
  - `stats [--project <path>] [--last N]`
- Added `util.rs` to project structure section.
- Updated status line:
  - "Phases 1-9 complete" -> "Phases 1-10 complete"

### `LICENSE` addition
- Added standard MIT license text.
- Values used per phase instruction:
  - year: `2026`
  - holder: `Ted Karlsson`

### Verification
- `cargo test` -> pass (`154 passed`, `1 ignored`)
- `cargo clippy -- -D warnings` -> pass

## Final Phase 10 outcome

All Phase 10 objectives in `PHASE10.md` are complete:
- Tier-1 carryover verifications completed
- CLI command identity consistently `tod`
- `status`/`stats` support `--project`
- Shared utilities (`safe_preview`, `warn!`) consolidated in `util.rs`
- Structured typed errors introduced for context/loop/stats surfaces
- `LICENSE` added and docs updated to reflect post-Phase-10 state

Final validation state:
- `cargo test`: `154 passed`, `1 ignored`, `0 failed`
- `cargo clippy -- -D warnings`: clean
