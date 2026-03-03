# PHASE16.md - Operator Usability and Workflow Safety

Read `AGENTS.md` first. All operating principles and safety rules apply.

---

## Goal

Phase 15 reduced structural maintenance risk and compatibility drift. Phase 16 focuses on making Tod more practical for real Rust development workflows while preserving the existing safety model.

Primary outcomes:
- Stronger operator guidance for command/mode/resume decisions.
- Safer workflow defaults for real repositories.
- Incremental maintainability improvements without feature-surface explosion.
- Preserved artifact and resume compatibility.

---

## Why This Phase Now

Current status is strong on core correctness (`193` tests passing, clippy clean), but adoption risk remains:
- operational workflow guidance is still implicit,
- safe real-repo usage patterns are not yet first-class,
- orchestration complexity remains concentrated in `loop.rs`.

This phase prioritizes practical utility and trust over broad new capability.

---

## Design Decisions (Locked)

1. Preserve behavior unless a task explicitly introduces a user-visible change.
2. Do not weaken path safety, transactional apply semantics, or compatibility defaults.
3. Keep all writes best-effort where already defined (`loop_io` semantics unchanged).
4. Keep `log_schema.rs` as pure data+serde.
5. Prefer small, reviewable changes and phase-scoped extraction over large rewrites.
6. Do not introduce major features this phase (no patch mode, no provider expansion, no git worktree orchestration engine).

---

## Baseline (Start of Phase 16)

- `cargo test`: **193 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1-15 complete

---

## Task 1: Operator Guidance Alignment

### What
Update operator-facing docs so command usage and operational tradeoffs are explicit.

### Scope
- Add `docs/runbook.md` containing:
  - **Mode decision matrix**: when to use default vs strict vs dry-run. Include a concise table mapping scenario → recommended flags.
  - **Cap tuning guidance**: explain `--max-iters` (per-step) and its derived total cap (`max_iters * 5`), plus `--max-tokens`. Give concrete examples (small bugfix: 3 iters; complex refactor: 8 iters + token cap).
  - **Resume and `--force` guidance**: when resume works, what fingerprint mismatch means, what `--force` overrides, and the risk of forcing past drift.
  - **Failure recovery decision tree**: given an error outcome (plan_error, cap_reached, token_cap, aborted, edit_error, apply_error), what operator action to take next.
- Update `README.md` to reference `docs/runbook.md` in the usage section.
- Ensure all CLI `--help` text is consistent with runbook guidance (no behavior changes, just verify alignment).

### Constraints
- Documentation only; no code changes.
- Keep guidance concrete and concise — operator runbook, not tutorial.

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 2: Pre-Run Dirty-Workspace Warning

### What
Add an informational warning when `tod run` targets a project with uncommitted git changes. Non-blocking — the run proceeds regardless.

### Design

Add a helper function in `loop.rs`:

```rust
/// Check if project root is inside a git repo with uncommitted changes.
/// Returns a warning string if dirty, None if clean or not a git repo.
fn check_workspace_dirty(project_root: &Path) -> Option<String>
```

Put it in `loop.rs` near the other pre-run helpers. Do not create a new module for it.

Implementation:
1. Run `git -C <project_root> status --porcelain` (blocking, via `std::process::Command`).
2. If the command fails (not a git repo, git not installed), return `None` silently. This is informational only.
3. If stdout is non-empty, return `Some("warning: workspace has uncommitted changes — consider committing or stashing before running Tod")`.

Call site: at the top of `pub fn run()`, before `build_planner_context`. If `Some(warning)`, print via `eprintln!`.

### Constraints
- **Non-blocking**: the warning prints but does not prevent the run.
- **No new dependencies**: uses `std::process::Command` (already used in `runner.rs`).
- Does not apply to `resume` (resume already has fingerprint checks).
- Does not apply to `--dry-run` (dry-run doesn't mutate).

### Tests
- `check_workspace_dirty` returns `None` for a non-git directory (TempSandbox).
- `check_workspace_dirty` returns `None` for a clean git repo (init + commit in TempSandbox).
- `check_workspace_dirty` returns `Some(...)` for a dirty git repo (init + commit + modify in TempSandbox).
- Git-dependent tests (clean repo, dirty repo) must skip gracefully if `git` is not available in the test environment. Check for git at test start; if missing, the test passes trivially (the "git unavailable returns None" path is already covered by the non-git-directory test).
- Verify `run()` behavior is unchanged (warning is informational only — existing run tests must still pass).

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 3: Extract Cap-Check Helpers from `loop.rs`

### What
Extract the iteration-cap and token-cap guard logic from `run_from_state` into pure helper functions. This reduces inline complexity in the main loop body.

### Design

Add two pure functions in `loop.rs` (above `run_from_state`):

```rust
/// Check whether the total iteration cap has been reached.
/// Returns Some(LoopError::TotalIterationCap { .. }) if exceeded, None otherwise.
fn check_iteration_cap(state: &RunState) -> Option<LoopError>
```

```rust
/// Check whether the token budget has been exceeded.
/// Returns Some(LoopError::TokenCapExceeded { .. }) if exceeded, None otherwise.
/// Returns None if max_tokens is 0 (no cap).
fn check_token_cap(state: &RunState) -> Option<LoopError>
```

Then replace the inline cap checks in `run_from_state` and `run` with calls to these helpers. The surrounding checkpoint/final-log/return-Err pattern stays inline (it depends on mutable state and config), but the decision logic becomes a one-line `if let Some(err) = check_…(state)`.

### Constraints
- **Behavior-preserving**: identical outcomes, checkpoint timing, and artifact paths.
- The helpers are pure functions of `&RunState` — no side effects.
- Existing tests must pass unchanged.

### Tests
Add focused unit tests for each helper:
- `check_iteration_cap` returns `None` when `total_iterations < max_total_iterations`.
- `check_iteration_cap` returns `Some(TotalIterationCap)` when `total_iterations >= max_total_iterations`.
- `check_token_cap` returns `None` when `max_tokens == 0` (no cap).
- `check_token_cap` returns `None` when `usage.total() <= max_tokens`.
- `check_token_cap` returns `Some(TokenCapExceeded)` when `usage.total() > max_tokens`.

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 4: Structured JSON Output for Stats

### What
Add `--json` flag to `tod stats` and `tod status` commands for machine-readable output.

### Design

**CLI changes** (`cli.rs`):
- Add `#[arg(long)] json: bool` to both `Status` and `Stats` variants.

**Stats changes** (`stats.rs`):
- Add `pub fn format_run_summary_json(summary: &RunSummary) -> String` that serializes a JSON object with fields: `run_id`, `goal`, `outcome`, `terminal_message`, `steps_completed`, `steps_aborted`, `total_attempts`, `attempts_per_step`, `failure_stages`, `input_tokens`, `output_tokens`, `total_tokens`, `llm_requests_total`, `llm_requests_plan`, `llm_requests_edit`.
- Add `pub fn format_multi_run_summary_json(summary: &MultiRunSummary) -> String` that serializes a JSON object with all `MultiRunSummary` fields.
- Use `serde_json::json!` macro for construction (serde_json is already a dependency).

**Dispatch changes** (`main.rs`):
- In `Status` branch: if `json`, call `format_run_summary_json`, else existing `format_run_summary`.
- In `Stats` branch: if `json`, call `format_multi_run_summary_json`, else existing `format_multi_run_summary`.

### Constraints
- Default output (no `--json`) is unchanged.
- JSON output is a single line (compact, not pretty-printed) for easy piping.
- No changes to `RunSummary` or `MultiRunSummary` struct definitions.
- All JSON fields are derived directly from existing struct fields — no ad-hoc computation. The field list in the JSON formatters must map 1:1 to `RunSummary` / `MultiRunSummary` fields.
- Legacy log deserialization behavior unchanged.

### Tests
- CLI parsing: `tod stats --json` and `tod status --json` parse correctly.
- `format_run_summary_json` produces valid JSON that round-trips through `serde_json::from_str::<serde_json::Value>`.
- `format_multi_run_summary_json` produces valid JSON with expected keys.
- Verify existing human-readable format tests still pass.

### Reasoning level
Medium

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 5: Documentation and Phase Closure

### What
Align docs and phase status after implementation.

### Scope
- Update `AGENTS.md`:
  - Phase 16 status to `Done`.
  - Baseline to final test count.
  - Add Phase 16 Outcomes section (replace handoff wording with outcomes wording).
  - Add Phase 17 Priority handoff section.
- Update `README.md`:
  - Status line to `Phases 1-16`.
  - Reference `docs/runbook.md` if not already done in Task 1.
  - Note `--json` flag in stats/status usage.
- Write `docs/phase16-implementation-log-<date>.md` with task-by-task log and verification timeline.

### Reasoning level
Low

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Out of Scope (Phase 17+)

- Patch/diff edit mode.
- Multi-provider expansion.
- Full git branch/worktree orchestration.
- Async runtime migration.
- Large reviewer-policy redesign.
