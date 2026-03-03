# Phase 13 Implementation Log (2026-03-03)

Date: 2026-03-03 (UTC)
Scope: Resume Determinism & Drift Hardening (`PHASE13.md`)

## Baseline Before Phase 13

- `cargo test`: 169 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

## Task 1: Make Fingerprint Truly Checkpoint-Scoped

### Goal
Ensure persisted checkpoint fingerprints reflect the workspace state at checkpoint write time.

### Implementation
Updated [`src/loop.rs`](../src/loop.rs):

- Added `RunState::refresh_fingerprint(&mut self, project_root: &Path)`.
- Called `state.refresh_fingerprint(&config.project_root)` immediately before all `state.checkpoint(config)` calls inside `run_from_state()`:
  - total iteration cap exit
  - edit-generation error exit
  - token-cap-after-edit exit
  - apply error exit
  - proceed checkpoint
  - retry checkpoint
  - abort checkpoint
  - per-step cap exhaustion
  - step-advance checkpoint
- Did not change `checkpoint(&self, ...)` signature/contract.
- Did not add refresh in `run()` initial checkpoint.
- Did not add refresh in `resume()` beyond existing recomputation path.

### Tests Added
In [`src/loop.rs`](../src/loop.rs):

- `checkpoint_refreshes_fingerprint_after_edit`
- `resume_after_agent_edit_without_force_succeeds`

### Verification After Task 1

- `cargo test`: 171 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 2: Persist and Reuse Run Execution Profile on Resume

### Goal
Preserve and reuse the originating run profile for deterministic resume semantics.

### Implementation
Updated [`src/loop.rs`](../src/loop.rs):

- Added `RunProfile`:
  - `mode: String` (`"default" | "strict"`)
  - `dry_run: bool`
  - `max_runner_output_bytes: usize`
- Added conversion helpers:
  - `RunProfile::from_config(&RunConfig)`
  - `RunProfile::to_run_mode()` with warning fallback for unknown mode strings
- Extended `RunState` with:
  - `#[serde(default)] pub profile: Option<RunProfile>`
- Initialized `profile` in `RunState::new()` with `Some(RunProfile::from_config(config))`.
- In `resume()`:
  - built `effective_config` from stored profile when present
  - kept legacy compatibility by falling back to caller config when profile missing
  - passed `&effective_config` into `run_from_state()`
  - used `effective_config` for token-cap final log path

Compatibility update in [`src/stats.rs`](../src/stats.rs):

- Added `profile: None` to direct `RunState` fixture literal used in stats tests.

### Tests Added
In [`src/loop.rs`](../src/loop.rs):

- `resume_preserves_strict_mode_in_attempt_log`
- `resume_preserves_dry_run_behavior`
- `legacy_state_without_profile_still_resumes`

### Verification After Task 2

- `cargo test`: 174 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 3: Upgrade Drift Detection to Content-Aware Fingerprint (Versioned)

### Goal
Add versioned fingerprinting with v2 content-aware hashing while keeping legacy checkpoint compatibility.

### Implementation
Updated [`src/loop.rs`](../src/loop.rs):

- Extended `Fingerprint` with:
  - `#[serde(default = "default_fingerprint_version")] pub fingerprint_version: u8`
  - `default_fingerprint_version() -> 1` for legacy deserialization
- Updated `compute_fingerprint()`:
  - now computes v2 fingerprints (`fingerprint_version: 2`)
  - still tracks `file_count` and `total_bytes`
  - hash now incorporates each file path + file content bytes (content-aware)
- Updated `resume()` fingerprint comparison logic:
  - v1 stored vs v2 current: enforce `file_count` / `total_bytes`; skip hash equality; emit warning:
    - `legacy v1 fingerprint — same-size drift not detected until next checkpoint upgrade`
  - v1 vs v1: hash comparison (legacy behavior)
  - v2 vs v2: compare count + bytes + hash
  - fallback path for unknown version combinations uses hash mismatch

Compatibility update in [`src/stats.rs`](../src/stats.rs):

- Added `fingerprint_version: 2` to direct `Fingerprint` fixture literal.

### Tests Added
In [`src/loop.rs`](../src/loop.rs):

- `fingerprint_v2_detects_same_size_change`
- `resume_legacy_fingerprint_version_compatible`

### Verification After Task 3

- `cargo test`: 176 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 4: Harden Run ID Uniqueness

### Goal
Avoid same-second log directory collisions while preserving stats ordering behavior.

### Implementation
Updated [`src/loop.rs`](../src/loop.rs):

- Changed run ID base format in `RunState::new()` to include fractional seconds:
  - `"%Y%m%d_%H%M%S%.6f"`
- Added defensive collision handling:
  - if `.tod/logs/<run_id>` exists, append `_2`, `_3`, ... until unique
- `log_dir` remains `.tod/logs/{run_id}` and lexical recency ordering remains compatible.

Updated [`src/stats.rs`](../src/stats.rs):

- Added stats sorting stability test for fractional + suffixed run IDs.

### Tests Added

- In [`src/loop.rs`](../src/loop.rs):
  - `run_state_new_generates_unique_run_ids`
- In [`src/stats.rs`](../src/stats.rs):
  - `run_id_sorting_remains_stable_for_stats`

### Verification After Task 4

- `cargo test`: 178 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 5: Close Invariant Debt and Documentation

### Goal
Remove non-test `expect` in main run path and align docs with Phase 13 behavior.

### Implementation
Updated [`src/main.rs`](../src/main.rs):

- Replaced non-test `.expect("Run command must produce run config")` with explicit `let Some(...) = ... else` handling.
- On failure, prints `failed to build run configuration` to stderr and exits with code 1.

Updated [`AGENTS.md`](../AGENTS.md):

- Updated baseline test count to 178 passing, 1 ignored.
- Updated phase status text to show phases 1–13 complete.
- Marked Phase 13 row as done.
- Added invariants documenting:
  - v2 versioned content-aware fingerprinting with legacy v1 compatibility path
  - run ID fractional timestamp + collision suffixing

Updated [`README.md`](../README.md):

- Added “Resume determinism notes” for:
  - persisted run profile on resume
  - v2 fingerprint semantics + v1 migration behavior
  - run ID collision hardening
- Updated status line to “Phases 1-13 complete” with current verification baseline.

### Verification After Task 5

- `cargo test`: 178 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Final Result

Phase 13 implementation completed end-to-end with sequential task gating and verification after each task.

Final repository state for this phase:
- deterministic checkpoint-scoped fingerprint persistence
- resume profile determinism across mode/dry-run/runner-output cap
- versioned, content-aware drift detection with legacy compatibility
- hardened run ID generation and collision handling
- invariant cleanup and docs synchronized with implemented behavior
