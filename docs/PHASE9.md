# PHASE9.md — Working Prototype

**Read AGENTS.md first.** All operating principles, coding standards, and safety rules apply.

---

## Goal

Make Tod usable end-to-end: prove the loop works on a real project, add context safety, handle transient LLM failures, provide a scaffolding command, and package for first use. Five tasks in order.

---

## Task 1: End-to-end live run

### What

Validate Tod against a real project with real LLM calls. This is not a code task — it's a validation exercise that will surface problems to fix before moving on.

### Procedure

1. Create a toy target project:

```bash
mkdir -p ~/Agents/tod-test-target
cd ~/Agents/tod-test-target
cargo init .
```

2. Run Tod in dry-run mode first to verify planning + edit generation:

```bash
cd ~/Agents/Tod
cargo run -- run --dry-run --project ~/Agents/tod-test-target "Add a CLI flag --name that takes a string and prints hello, <name>"
```

3. Inspect `.tod/logs/` in the target project. Verify:
   - `plan.json` has sensible steps
   - Attempt logs have valid `EditBatch` JSON
   - `state.json` has nonzero usage fields

4. Run Tod for real (no `--dry-run`):

```bash
cargo run -- run --project ~/Agents/tod-test-target "Add a CLI flag --name that takes a string and prints hello, <name>"
```

5. Verify:
   - Target project compiles (`cd ~/Agents/tod-test-target && cargo build`)
   - The flag works (`cargo run -- --name world` prints `hello, world`)
   - `tod status` and `tod stats` report correctly

### Handling findings

- **Blocks a successful round-trip** → fix inline as part of Phase 9. Document the fix.
- **Quality-of-life improvement** → log it for a future phase. Do not fix now.
- **Prompt issues** → adjust `SYSTEM_PROMPT` in `planner.rs` or `editor.rs` only if the current prompt causes structural failures (wrong JSON, missing fields). Do not tune for quality.

### Deliverable

A written record of the live run: commands executed, outcome, any fixes applied, any issues deferred. Save as `docs/live-run-log.md`.

### Verify

Tod successfully completes at least one real run against the toy project. `tod stats` shows the run.

**Do not start Task 2 until Task 1 has at least one successful round-trip.**

---

## Task 2: Context window management (`context.rs`)

### What

Extract context-building logic from `loop.rs` into a new `context.rs` module. Add byte budgets so context cannot grow unbounded and overflow the model's context window.

### New file: `src/context.rs`

### Constants

```rust
/// Max bytes for planner context (project tree + Cargo.toml).
const MAX_PLANNER_CONTEXT_BYTES: usize = 128 * 1024; // 128 KiB

/// Max bytes for step file context (file contents sent to editor).
const MAX_STEP_CONTEXT_BYTES: usize = 64 * 1024; // 64 KiB

/// Max bytes for retry context (previous failure output appended to step context).
const MAX_RETRY_CONTEXT_BYTES: usize = 8 * 1024; // 8 KiB

/// Max directory depth to recurse when building project context.
const MAX_TREE_DEPTH: usize = 12;

/// Max files to list in planner context.
const MAX_LISTED_FILES: usize = 200;
```

### Functions to move from `loop.rs`

These functions move to `context.rs` with their existing logic, plus byte-budget enforcement:

**`build_planner_context(project_root: &Path) -> Result<String, ContextError>`**

- Same logic as current `build_project_context()` in `loop.rs`.
- After building the string, truncate to `MAX_PLANNER_CONTEXT_BYTES` using a UTF-8-safe truncation that snaps to the last complete file entry.
- If truncated, append `\n\n... [context truncated, {N} bytes omitted] ...`.

**`build_step_context(project_root: &Path, files: &[String], step_index: usize) -> Result<String, ContextError>`**

- Same logic as current `build_step_file_context()` in `loop.rs`.
- After building file context, truncate to `MAX_STEP_CONTEXT_BYTES`.
- If total exceeds budget: include files in order, truncating the last file that fits, and append a note listing omitted files.

**`build_retry_context(error_output: &str) -> String`**

- Truncate `error_output` to `MAX_RETRY_CONTEXT_BYTES` using UTF-8-safe line-snapped truncation.
- Prefix with `## Previous runner failure\n`.

**`truncate_context(s: &str, max_bytes: usize) -> String`**

- Move from `loop.rs`. Same logic.

Helper functions that also move: `collect_paths()`, `format_file_context()` (currently in `editor.rs` — move here, update `editor.rs` to import from `context.rs`).

### Error type

```rust
#[derive(Debug)]
pub enum ContextError {
    Io { path: String, cause: String },
    InvalidPath { step_index: usize, path: String, reason: String },
}
```

This replaces the `LoopError::Io` and `LoopError::InvalidPlanPath` variants that are currently used by the context helpers in `loop.rs`. Add `From<ContextError> for LoopError`.

### Changes to `loop.rs`

- Remove `build_project_context()`, `build_step_file_context()`, `collect_paths()`, `truncate_context()`, `MAX_TREE_DEPTH`, `MAX_LISTED_FILES`.
- Import from `context.rs` instead.
- In `run()`: replace `build_project_context(&config.project_root)?` with `context::build_planner_context(&config.project_root)?`.
- In `run_from_state()`: replace `build_step_file_context(...)` with `context::build_step_context(...)`.
- In `run_from_state()`: replace the inline retry context appending with `context::build_retry_context(ctx)`.

### Changes to `editor.rs`

- `format_file_context()` moves to `context.rs`.
- `editor.rs` imports it: `use crate::context::format_file_context;`
- Update any tests in `editor.rs` that reference the function.

### Changes to `main.rs`

- Add `mod context;`.

### Tests

Add to `context.rs`:

| Test | Setup | Assertion |
|------|-------|-----------|
| `planner_context_within_budget` | Small project tree | Returns context, length ≤ `MAX_PLANNER_CONTEXT_BYTES` |
| `planner_context_truncates_large_tree` | Create 300+ files | Output truncated, contains truncation note |
| `step_context_within_budget` | Two small files | Returns context with both files |
| `step_context_truncates_large_files` | Write a 100 KiB file | Output truncated to budget, omitted files noted |
| `retry_context_truncates` | 16 KiB error string | Output ≤ `MAX_RETRY_CONTEXT_BYTES`, line-snapped |
| `retry_context_prefixes_header` | Any error string | Output starts with `## Previous runner failure` |
| `format_file_context_numbers_lines` | (moved from editor.rs) | Existing assertion preserved |
| `collect_paths_excludes_hidden_dirs` | Create `.git/`, `target/`, `.tod/` | None appear in output |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **140+ passing, 1 ignored.**

**Do not start Task 3 until Task 2 is verified.**

---

## Task 3: Retry on LLM transient failure

### What

Add retry with exponential backoff inside `AnthropicProvider::complete()` for transient HTTP errors. The loop never sees transient failures.

### In `llm.rs`

#### Retry policy constants

```rust
const MAX_RETRIES: usize = 3;
const INITIAL_BACKOFF_MS: u64 = 1000;
```

#### Retryable condition

```rust
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503)
}
```

Network errors (from `ureq::Error`) are always retryable.

HTTP 400, 401, 403, 404 are **not** retryable — they indicate a request or auth problem.

#### Implementation in `AnthropicProvider::complete()`

Wrap the existing HTTP call in a retry loop:

```rust
for attempt in 0..=MAX_RETRIES {
    // ... existing request logic ...

    match result {
        // Network error → retryable
        Err(e) if attempt < MAX_RETRIES => {
            eprintln!("warning: LLM request failed (attempt {}), retrying: {e}", attempt + 1);
            sleep_with_jitter(attempt);
            continue;
        }
        Err(e) => return Err(LlmError::RequestFailed(e.to_string())),

        Ok(response) => {
            let status = response.status().as_u16();
            if status >= 400 && is_retryable_status(status) && attempt < MAX_RETRIES {
                let body_preview = // read + preview body
                eprintln!("warning: LLM API error {status} (attempt {}), retrying", attempt + 1);
                sleep_with_jitter(attempt);
                continue;
            }
            // ... existing response handling ...
        }
    }
}
```

#### Backoff helper

```rust
fn sleep_with_jitter(attempt: usize) {
    let base_ms = INITIAL_BACKOFF_MS * 2u64.pow(attempt as u32);
    let jitter_ms = base_ms / 4; // ±25%
    let actual_ms = base_ms + (pseudo_random_offset(jitter_ms));
    std::thread::sleep(std::time::Duration::from_millis(actual_ms));
}
```

For jitter, use a simple deterministic source (e.g., `std::time::SystemTime::now()` nanos modulo range). No need for a `rand` dependency.

#### Token budget interaction

Retried requests that ultimately fail at the HTTP level return no usage data — they don't count against the token budget. Only successful responses that include a `usage` field contribute to the token accumulator. This is already the case because usage extraction only happens on a successful response parse.

### Tests

Add to `llm.rs`:

| Test | Assertion |
|------|-----------|
| `is_retryable_429` | `is_retryable_status(429)` returns true |
| `is_retryable_500` | `is_retryable_status(500)` returns true |
| `not_retryable_400` | `is_retryable_status(400)` returns false |
| `not_retryable_401` | `is_retryable_status(401)` returns false |

The retry behavior itself is best validated during the live run (Task 1), not in unit tests, because it involves real HTTP and real `sleep()` calls. The unit tests verify the classification logic.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **144+ passing, 1 ignored.**

**Do not start Task 4 until Task 3 is verified.**

---

## Task 4: `init` command

### What

Implement the `init` subcommand by delegating to `cargo init` and appending `.tod/` to `.gitignore`.

### In `main.rs`

Replace the stub:

```rust
Command::Init { name } => {
    match init_project(&name) {
        Ok(()) => println!("initialized project: {name}"),
        Err(e) => {
            eprintln!("init failed: {e}");
            std::process::exit(1);
        }
    }
}
```

### New function in `main.rs` (or a small `init.rs` if preferred)

```rust
fn init_project(name: &str) -> Result<(), String> {
    // Run cargo init
    let output = std::process::Command::new("cargo")
        .args(["init", name])
        .output()
        .map_err(|e| format!("failed to run cargo init: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo init failed: {stderr}"));
    }

    // Append .tod/ to .gitignore
    let gitignore_path = std::path::Path::new(name).join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    if !existing.contains(".tod/") {
        let append = if existing.ends_with('\n') || existing.is_empty() {
            ".tod/\n"
        } else {
            "\n.tod/\n"
        };
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&gitignore_path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, append.as_bytes()))
            .map_err(|e| format!("failed to update .gitignore: {e}"))?;
    }

    Ok(())
}
```

### Tests

Testing `init` requires `cargo` to be installed (true in any Rust dev environment). Tests use `TempSandbox` as a working directory.

| Test | Setup | Assertion |
|------|-------|-----------|
| `init_creates_project` | Run `init_project("test_proj")` in a temp dir | `test_proj/Cargo.toml` exists, `test_proj/src/main.rs` exists |
| `init_adds_tod_to_gitignore` | Run `init_project` | `.gitignore` contains `.tod/` |
| `init_does_not_duplicate_gitignore_entry` | Run `init_project`, manually add `.tod/` again, verify only one entry | `.tod/` appears exactly once |
| `init_fails_on_existing_dir` | Create the directory first with a Cargo.toml | Returns error (cargo init's own behavior) |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **148+ passing, 1 ignored.**

**Do not start Task 5 until Task 4 is verified.**

---

## Task 5: README + Cargo.toml metadata

### What

Update documentation and crate metadata for a presentable prototype.

### Cargo.toml

Add or update these fields:

```toml
[package]
name = "tod"
version = "0.1.0"
edition = "2021"
description = "A minimal Rust coding agent that edits projects via LLM-generated JSON"
license = "MIT"
repository = "https://github.com/<your-username>/Tod"
```

### README.md

Replace the current README with a concise prototype-level document covering:

- **What Tod is** (one paragraph)
- **Quick start** (`cargo build`, set `ANTHROPIC_API_KEY`, `tod init myproject`, `tod run`)
- **Commands** (init, run, resume, status, stats — one line each)
- **Configuration** (env vars: `ANTHROPIC_API_KEY`, `TOD_MODEL`, `TOD_RESPONSE_MAX_TOKENS`)
- **Run flags** (`--strict`, `--max-iters`, `--max-tokens`, `--dry-run`, `--project`)
- **Project structure** (the module list from AGENTS.md, abbreviated)
- **Status** ("prototype — Phases 1–9 complete")

Keep it under 100 lines. No badges, no screenshots, no feature roadmap.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

No test count change. Just documentation.

---

## Implementation order summary

| Task | Scope | Files touched |
|------|-------|---------------|
| 1. Live run | Validation, manual | None (or small prompt fixes in planner.rs/editor.rs) |
| 2. Context management | New module + extraction | `context.rs` (new), `loop.rs`, `editor.rs`, `main.rs` |
| 3. LLM retry | Provider-internal | `llm.rs` |
| 4. Init command | New feature | `main.rs` (or `init.rs`) |
| 5. README + metadata | Documentation | `README.md`, `Cargo.toml` |

**Do not start a later task until the preceding task is verified passing.**

---

## Phase 9 "done" criteria

- Tod has completed at least one successful real LLM-driven run against a toy project, documented in `docs/live-run-log.md`.
- Context building lives in `context.rs` with byte budgets. No context can grow unbounded.
- LLM transient failures (429, 500, network) are retried inside the provider with backoff.
- `tod init` scaffolds a usable project via `cargo init` + `.gitignore` entry.
- README and Cargo.toml are presentable for a first-time user.
- `cargo test` passes with ≥ 148 tests, 0 failing, 1 ignored.
- `cargo clippy -- -D warnings` clean.

## Definition of "working prototype"

After Phase 9, Tod can:
- Scaffold or target a project
- Complete at least one real LLM-driven round trip
- Handle transient LLM failures without dying
- Stay within context and token budgets
- Produce inspectable logs and stats
- Be tried by someone else without handholding
