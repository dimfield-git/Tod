# PHASE10.md — External Usability

**Read AGENTS.md first.** All operating principles, coding standards, and safety rules apply.

---

## Goal

Make Tod usable by someone who didn't build it. Phase 9 landed the working prototype; Phase 10 makes it honest, consistent, and externally presentable. No new agent capabilities — this is naming, CLI ergonomics, shared utilities, and error hygiene.

Eight tasks in order. Tasks 1–3 are the gate (verify Phase 9 landed correctly). Tasks 4–8 are the work. Tasks 4–6 are mechanical and low-risk. Task 7 is the largest change. Task 8 is documentation housekeeping.

---

## Baseline

- `cargo test`: 148 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean
- Phases 1–9 complete

---

## Task 1: Verify Phase 9 — sort order in `context.rs`

### What

Confirm that `collect_paths()` in `context.rs` sorts file paths before building context. The original `build_project_context()` in `loop.rs` had `files.sort()` after collecting. If this didn't survive the extraction, prompt content becomes non-deterministic across machines.

### Check

```bash
grep -n 'sort' src/context.rs
```

### Expected

A `files.sort()` or equivalent call after `collect_paths()` returns, before the paths are used to build context strings.

### If missing

Add `files.sort();` immediately after the `collect_paths()` call in `build_planner_context()`. No new test needed — the existing `planner_context_within_budget` test can be extended to assert sorted output if desired.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

No test count change.

**Do not start Task 2 until Task 1 is verified.**

---

## Task 2: Verify Phase 9 — binary name consistency

### What

Confirm that all references to the old binary name `agent` have been replaced with `tod`. Phase 9 Task 5 renamed the crate in `Cargo.toml`, but the clap declaration in `cli.rs` and all test parse calls still use `"agent"`.

### Check

```bash
grep -rn '"agent"' src/
grep -rn 'name = "agent"' src/
grep -rn 'agent' docs/live-run-log.md README.md
```

### Expected

The first two commands will show hits in `cli.rs` (the `#[command(name = "agent")]` attribute and all test `parse(&["agent", ...])` calls). This confirms the rename didn't fully land. The docs may also reference `agent status` / `agent stats`.

### Action

This verification feeds directly into Task 4. Record what needs changing and proceed.

**Do not start Task 3 until Task 2 is verified.**

---

## Task 3: Verify Phase 9 — `llm_requests` increment timing

### What

Confirm `llm_requests` in `RunState` increments only on successful LLM completions (responses that return usage data), not on transport retries handled inside the provider.

### Check

```bash
grep -n 'llm_requests' src/loop.rs
```

### Expected

`llm_requests` increments inside the `if let Some(usage) = &call_usage` block, after successful `create_plan()` and `create_edits()` calls. It should **not** appear inside `llm.rs` retry logic.

### If wrong

Move the increment so it's gated on `Some(usage)`. This is the semantically correct position — "billed requests" should match what the provider charges for.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

No test count change.

**Do not start Task 4 until Task 3 is verified.**

---

## Task 4: Rename CLI `"agent"` → `"tod"`

### What

Fix the binary name mismatch. The crate is `tod` in `Cargo.toml` but the CLI still declares `#[command(name = "agent")]` and every test hardcodes `"agent"` as argv[0]. This is a sharp edge for users and breaks every doc snippet.

### Pre-check

Confirm the hits from Task 2, plus the clap attribute specifically:

```bash
grep -rn '#\[command(name = "agent"' src/
grep -rn '"agent"' src/
```

### Changes

**`src/cli.rs`**

Change the clap attribute:

```rust
// Before
#[command(name = "agent", version)]
// After
#[command(name = "tod", version)]
```

Update every test that calls `parse()` or `Cli::try_parse_from()` — replace `"agent"` with `"tod"` in all argv arrays. The affected tests are:

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

**`docs/live-run-log.md`**

Replace any `agent status`, `agent stats`, or `agent run` references with `tod status`, `tod stats`, `tod run`.

**`README.md`**

If any references to `agent` as a command remain, fix them. (Phase 9 Task 5 may have already handled the README — verify before editing.)

### Tests

No new tests. All existing CLI tests must pass with the renamed argv.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

148 passing, 1 ignored.

**Do not start Task 5 until Task 4 is verified.**

---

## Task 5: Add `--project` to `status` and `stats`

### What

Currently `status` and `stats` operate from the current working directory. Adding `--project <path>` makes them usable from anywhere — essential for anyone who isn't the developer standing in the project root.

### Changes

**`src/cli.rs`** — modify the `Command` enum:

```rust
/// Show the status of the last run.
Status {
    /// Path to the target project root.
    #[arg(short, long, default_value = ".")]
    project: PathBuf,
},

/// Analyze run history.
Stats {
    /// Path to the target project root.
    #[arg(short, long, default_value = ".")]
    project: PathBuf,

    /// Number of recent runs to summarize.
    #[arg(long, default_value = "5")]
    last: usize,
},
```

**`src/main.rs`** — wire the new fields:

```rust
// Before
Command::Status => match stats::summarize_current(std::path::Path::new(".")) {
// After
Command::Status { project } => match stats::summarize_current(&project) {
```

```rust
// Before
Command::Stats { last } => {
    match stats::summarize_runs(std::path::Path::new(".tod"), last) {
// After
Command::Stats { project, last } => {
    let tod_dir = project.join(".tod");
    match stats::summarize_runs(&tod_dir, last) {
```

**`src/cli.rs` tests** — update affected tests:

- `parse_status`: change `Command::Status` match to `Command::Status { .. }`. Add a case with `--project myproj` that asserts `project == PathBuf::from("myproj")`.
- `parse_stats_default`: change match to `Command::Stats { last, .. }`.
- `parse_stats_with_last`: change match to `Command::Stats { last, .. }`. Add a case with `--project myproj --last 9`.
- `non_run_returns_none`: no change needed (already matches on non-Run).

### Tests

Update existing tests as described above. Add at least two new assertions:

| Test | Assertion |
|------|-----------|
| `parse_status` (extended) | `--project myproj` sets `project` to `PathBuf::from("myproj")` |
| `parse_stats_with_last` (extended) | `--project myproj --last 9` sets both fields correctly |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

148 passing, 1 ignored. (Test count unchanged — assertions added to existing tests, no new test functions required.)

**Do not start Task 6 until Task 5 is verified.**

---

## Task 6: Extract `safe_preview` to `util.rs` and add `warn!` macro

### What

Two small hygiene items that belong together because they both create `src/util.rs`.

**6a: `safe_preview` deduplication.** Identical UTF-8-safe string preview helpers exist in both `llm.rs` and `schema.rs`. Extract to a shared `util.rs` module.

**6b: `warn!` macro.** Retry warnings and other runtime messages use `eprintln!` directly. Replace with a thin `warn!()` wrapper that still prints to stderr but allows future swap to a logging framework without touching call sites.

### New file: `src/util.rs`

```rust
use std::fmt;

/// Emit a warning to stderr with a `warning: ` prefix.
pub fn warn(args: fmt::Arguments) {
    eprintln!("warning: {}", args);
}

/// Emit a warning to stderr. Wrapper for future structured logging.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::util::warn(format_args!($($arg)*))
    };
}

/// Truncate a string for error messages without panicking on UTF-8 boundaries.
pub fn safe_preview(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
```

### Changes

**`src/main.rs`** — add module declaration:

```rust
mod util;
```

Place it with the other `mod` declarations (after `stats`, before `#[cfg(test)]`).

**`src/llm.rs`**:

- Add `use crate::util::safe_preview;` to imports.
- Delete the local `safe_preview` function.
- Replace all `eprintln!("warning: ...")` calls in the retry loop with `crate::warn!(...)`. Remove the `"warning: "` prefix from the format string (the macro adds it).

**`src/schema.rs`**:

- Add `use crate::util::safe_preview;` to imports.
- Delete the local `safe_preview` function.

**`src/loop.rs`** (if any `eprintln!("warning: ...")` calls exist for checkpoint or fingerprint warnings):

- Replace with `crate::warn!(...)`.

### Tests

Add to `src/util.rs`:

| Test | Assertion |
|------|-----------|
| `safe_preview_within_limit` | String shorter than limit is returned unchanged |
| `safe_preview_truncates` | String longer than limit is truncated at a char boundary |
| `safe_preview_multibyte` | Truncation at a multi-byte char boundary doesn't panic and returns valid UTF-8 |

Existing tests in `llm.rs` and `schema.rs` that exercise `safe_preview` indirectly should continue to pass.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **151+ passing, 1 ignored.**

**Do not start Task 7 until Task 6 is verified.**

---

## Task 7: Structured errors for `LoopError`, `ContextError`, and `StatsError`

### What

Replace freeform string fields in error variants with typed data. Currently `LoopError::Io { path: String, cause: String }` and similar variants carry unstructured text. The same pattern exists in `ContextError` and `StatsError`. Typed errors enable stats to classify failure modes, CLI to format them consistently, and future features to branch on error kind programmatically.

This is the largest task in Phase 10. Take care to preserve `Display` output quality — the user sees these messages.

### Design decision: `io::ErrorKind` + message string

Store `PathBuf` for paths and `io::ErrorKind` for error classification. Keep a `message: String` from `e.to_string()` so the CLI can still show human-readable OS-level messages (e.g., "permission denied" rather than just `PermissionDenied`).

This avoids `Box<dyn Error>` (which complicates `Clone` and `PartialEq` on error types) while preserving both programmatic branching and human readability.

### 7a: `LoopError` changes in `src/loop.rs`

```rust
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum LoopError {
    Plan(PlanError),
    Edit {
        step_index: usize,
        iteration: usize,
        source: EditError,
    },
    Apply {
        step_index: usize,
        iteration: usize,
        source: ApplyError,
    },
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
        message: String,
    },
    InvalidPlanPath {
        step_index: usize,
        path: PathBuf,
        reason: String,
    },
    Aborted {
        step_index: usize,
        reason: String,
    },
    TotalIterationCap {
        max_total_iterations: usize,
    },
    TokenCapExceeded {
        used: u64,
        cap: u64,
    },
    NoCheckpoint,
    FingerprintMismatch {
        expected_hash: String,
        actual_hash: String,
    },
}
```

Update the `Display` impl:

```rust
Self::Io { path, message, .. } => write!(f, "I/O error for {}: {message}", path.display()),
Self::InvalidPlanPath { step_index, path, reason } => write!(
    f, "invalid plan path at step {} ({}): {reason}", step_index + 1, path.display()
),
```

Update all `LoopError::Io` construction sites in `loop.rs`:

```rust
.map_err(|e| LoopError::Io {
    path: some_path.to_path_buf(),
    kind: e.kind(),
    message: e.to_string(),
})
```

Update the `From<ContextError> for LoopError` impl to map the new typed fields directly:

```rust
impl From<ContextError> for LoopError {
    fn from(value: ContextError) -> Self {
        match value {
            ContextError::Io { path, kind, message } => Self::Io { path, kind, message },
            ContextError::InvalidPath { step_index, path, reason } => Self::InvalidPlanPath {
                step_index,
                path,
                reason,
            },
        }
    }
}
```

### 7b: `ContextError` changes in `src/context.rs`

```rust
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ContextError {
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
        message: String,
    },
    InvalidPath {
        step_index: usize,
        path: PathBuf,
        reason: String,
    },
}
```

Update `Display` to use `path.display()`.

Update all `ContextError::Io` construction sites in `context.rs`:

```rust
.map_err(|e| ContextError::Io {
    path: dir.to_path_buf(),
    kind: e.kind(),
    message: e.to_string(),
})
```

Update `ContextError::InvalidPath` construction sites to use `PathBuf`:

```rust
ContextError::InvalidPath {
    step_index,
    path: path_str.into(),
    reason: "...".to_string(),
}
```

### 7c: `StatsError` changes in `src/stats.rs`

```rust
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatsError {
    NoData,
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
        message: String,
    },
    InvalidLog {
        path: PathBuf,
        reason: String,
    },
}
```

Note: `io::ErrorKind` implements `Clone`, `PartialEq`, and `Eq`, so the existing derives are preserved.

Update `Display` and all construction sites in `stats.rs` (primarily in `read_json()` and `summarize_runs()`).

### Tests

Add to `src/loop.rs` tests:

| Test | Assertion |
|------|-----------|
| `loop_error_io_display` | `LoopError::Io` with a typed `PathBuf` and `ErrorKind::NotFound` displays correctly |
| `context_error_converts_to_loop_error` | `ContextError::Io` converts to `LoopError::Io` preserving path, kind, and message |

Add to `src/context.rs` tests:

| Test | Assertion |
|------|-----------|
| `context_error_io_display` | `ContextError::Io` displays with path and message |

Existing tests that construct or match on error variants must be updated to use the new field types. Specifically, any test that constructs `LoopError::Io { path: "...".to_string(), cause: "...".to_string() }` must change to `LoopError::Io { path: PathBuf::from("..."), kind: io::ErrorKind::Other, message: "...".to_string() }`.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **153+ passing, 1 ignored.**

**Do not start Task 8 until Task 7 is verified.**

---

## Task 8: AGENTS.md final update and LICENSE

### What

Two housekeeping items that close out the prototype.

**8a: AGENTS.md** — Update to reflect post-Phase 10 reality:

- Phase 9 status: `✅ Done`
- Phase 10 status: `✅ Done`
- Phase 10 description: "External usability — naming consistency, --project flag, shared utilities, structured errors, LICENSE"
- "Done" baseline: update test count to final number (≥ 153 passing, 1 ignored)
- Project map: add `util.rs` ("shared helpers: safe_preview, warn macro")
- Golden path: ensure all example commands say `tod`, not `agent`
- Remove or update any remaining Phase 9 aspirational language

**8b: LICENSE file** — Add the standard MIT license text in a `LICENSE` file at the repo root. `Cargo.toml` already declares `license = "MIT"`.

Use exactly:
- Year: `2026`
- Copyright holder: `Ted Karlsson`

The template is the standard MIT text from https://opensource.org/licenses/MIT with those two values filled in. No other modifications.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

No test count change. Just documentation and a license file.

---

## Implementation order summary

| Task | Scope | Files touched |
|------|-------|---------------|
| 1. Verify sort order | Terminal check | None (or `context.rs` if missing) |
| 2. Verify binary name | Terminal check | None (findings feed Task 4) |
| 3. Verify llm_requests | Terminal check | None (or `loop.rs` if wrong) |
| 4. Rename agent → tod | CLI + tests + docs | `cli.rs`, `docs/live-run-log.md`, possibly `README.md` |
| 5. --project flag | CLI + wiring | `cli.rs`, `main.rs` |
| 6. util.rs + warn! | New module + dedup | `util.rs` (new), `main.rs`, `llm.rs`, `schema.rs`, `loop.rs` |
| 7. Structured errors | Error types | `loop.rs`, `context.rs`, `stats.rs` |
| 8. AGENTS.md + LICENSE | Documentation | `AGENTS.md`, `LICENSE` |

**Do not start a later task until the preceding task is verified passing.**

---

## Phase 10 "done" criteria

- All three Tier-1 verifications confirmed (sort order, binary name, llm_requests timing).
- CLI binary name is `tod` everywhere — code, tests, docs.
- `tod status --project <path>` and `tod stats --project <path>` work from any directory.
- `safe_preview` exists in one place (`util.rs`), imported by `llm.rs` and `schema.rs`.
- `warn!` macro replaces all `eprintln!("warning: ...")` calls.
- `LoopError`, `ContextError`, and `StatsError` use `PathBuf` for paths and `io::ErrorKind` for error classification.
- `LICENSE` file exists and matches `Cargo.toml` declaration.
- `AGENTS.md` reflects current reality (Phase 10 done, correct project map, correct baselines).
- `cargo test` passes with ≥ 153 tests, 0 failing, 1 ignored.
- `cargo clippy -- -D warnings` clean.

## Definition of "externally usable"

After Phase 10, Tod can be cloned, built, and used by someone reading only the README. Commands are consistently named. Errors carry typed data suitable for programmatic handling. The project is licensed. No stale references to the old binary name remain.
