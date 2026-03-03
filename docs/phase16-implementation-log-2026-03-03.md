# Phase 16 Implementation Log (2026-03-03)

## Scope

Phase 16 focused on operator usability and workflow safety:
1. Operator runbook
2. Dirty-workspace warning
3. Cap-check helper extraction
4. JSON output for `status`/`stats`
5. Documentation and phase closure

## Task-by-Task Log

### Task 1: Operator Guidance Alignment

Changes:
- Added `docs/runbook.md` with:
  - mode decision matrix,
  - cap tuning guidance (`--max-iters`, derived `max_iters * 5`, `--max-tokens`),
  - resume and `--force` guidance,
  - failure recovery decision tree.
- Updated `README.md` usage section to reference the runbook.
- Verified CLI help text alignment for `run`, `resume`, `status`, and `stats`.

Verification:
- `cargo test` passed (`193 passed, 1 ignored` at this step).
- `cargo clippy -- -D warnings` passed.

### Task 2: Pre-Run Dirty-Workspace Warning

Changes:
- Added `check_workspace_dirty(project_root: &Path) -> Option<String>` in `src/loop.rs`.
- Implemented check with `git -C <project_root> status --porcelain`.
- Added informational `eprintln!` warning in `run()` only when:
  - not dry-run,
  - git indicates uncommitted changes.
- Preserved non-blocking behavior and silent fallback when git is unavailable/not a repo.

Tests added:
- non-git directory returns `None`,
- clean git repo returns `None`,
- dirty git repo returns warning string,
- git-dependent tests skip trivially when git is unavailable.

Verification:
- `cargo test` passed (`196 passed, 1 ignored` at this step).
- `cargo clippy -- -D warnings` passed.

### Task 3: Cap-Check Extraction

Changes:
- Added pure helpers in `src/loop.rs`:
  - `check_iteration_cap(&RunState) -> Option<LoopError>`
  - `check_token_cap(&RunState) -> Option<LoopError>`
- Replaced inline cap decision logic in `run()` and `run_from_state()` with helper calls.
- Kept checkpointing, final log writing, and return behavior inline and unchanged.

Tests added:
- iteration cap: under limit -> `None`,
- iteration cap: at/over limit -> `Some(TotalIterationCap)`,
- token cap: disabled (`max_tokens == 0`) -> `None`,
- token cap: `usage <= cap` -> `None`,
- token cap: `usage > cap` -> `Some(TokenCapExceeded)`.

Verification:
- `cargo test` passed (`201 passed, 1 ignored` at this step).
- `cargo clippy -- -D warnings` passed.

### Task 4: Structured JSON Output for Stats

Changes:
- CLI:
  - added `--json` to `status` and `stats` in `src/cli.rs`.
- Dispatch:
  - updated `src/main.rs` to select human vs JSON formatting based on `--json`.
- Stats formatters:
  - added `format_run_summary_json(&RunSummary) -> String`,
  - added `format_multi_run_summary_json(&MultiRunSummary) -> String`,
  - implemented with `serde_json::json!` and compact single-line output.

Tests added:
- CLI parse coverage for `tod status --json` and `tod stats --json`,
- JSON round-trip validity test for run-summary formatter,
- expected-key coverage test for multi-run-summary formatter.

Verification:
- `cargo test` passed (`203 passed, 1 ignored` at this step).
- `cargo clippy -- -D warnings` passed.

### Task 5: Documentation and Phase Closure

Changes:
- Updated `AGENTS.md`:
  - phase status set to Done,
  - baseline updated to current test count,
  - added Phase 16 Outcomes,
  - added Phase 17 Priority Handoff.
- Updated `README.md`:
  - status updated to Phases 1-16 complete,
  - baseline updated,
  - `status`/`stats` command examples and command list updated for `--json`,
  - runbook reference retained.
- Added this implementation log file.

Verification:
- Final gate run completed after Task 5:
  - `cargo test` passed (`203 passed, 1 ignored`)
  - `cargo clippy -- -D warnings` passed

## Final Result

Phase 16 deliverables are implemented, validated, and documented with compatibility/safety invariants preserved.
