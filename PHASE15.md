# PHASE15.md — Loop Surface Reduction & Compatibility Hardening

Read `AGENTS.md` first. All operating principles and safety rules apply.

---

## Goal

Phase 14 closed key observability and metrics-fidelity gaps. Phase 15 reduces orchestration maintenance risk while preserving runtime behavior and legacy compatibility.

Primary outcomes:
- Clean three-module boundary: `log_schema.rs` (types + serde), `loop_io.rs` (persistence + identity), `loop.rs` (orchestration).
- `loop.rs` no longer owns duplicated helper logic that can drift across call sites.
- Resume compatibility rules are isolated, explicit, and table-tested.
- Runtime artifact contracts remain stable (`plan.json`, `final.json`, `step_<n>_attempt_<m>.json`).
- No feature-surface expansion; this phase is reliability-through-structure.

---

## Why This Phase Now (Evidence)

Current module snapshot (2026-03-03):
- `src/loop.rs`: **~2157 LOC** (primary concentration hotspot).
- `log_schema.rs` contains `write_plan_error_artifact` — a Phase 14 expedient that violates the types-only boundary.
- Run-id/log-dir allocation is duplicated between `RunState::new` and the plan-error path in `run()`.

---

## Design Decisions (Locked)

1. **Behavior preservation first**: no CLI, policy, or artifact semantics changes unless explicitly listed.
2. **Three-module boundary**: `log_schema.rs` = types + serde defaults. `loop_io.rs` = persistence + identity allocation. `loop.rs` = orchestration.
3. **Run identity as struct**: the run-id allocator returns `RunIdentity { run_id, log_dir }`, not bare strings. All call sites use this one helper. No duplication.
4. **All persistence is best-effort**: all writes (checkpoint, plan, final, attempt logs) warn on failure and never propagate errors. Checkpoint writes additionally use atomic tmp+rename to prevent corruption. Extraction must not change these semantics.
5. **Artifact compatibility is non-negotiable**: legacy checkpoints/logs remain deserializable and summarizable.
6. **No major features**: patch mode, git isolation, local providers, and reviewer redesign stay out of scope.

---

## Baseline (Current Tree)

Validated on 2026-03-03:
- `cargo test`: **185 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1-14 complete

---

## Task 1: Create `loop_io.rs` and Extract Persistence Primitives

### What
Create `src/loop_io.rs` as the persistence and IO module for orchestration. Move write helpers out of both `loop.rs` and `log_schema.rs`.

### Items to move into `loop_io.rs`

**From `log_schema.rs`:**
- `write_plan_error_artifact(log_dir, run_id, message)` — move entirely. `log_schema.rs` becomes pure types + serde after this.

**From `loop.rs` — `impl RunState` methods:**
- `write_plan_log(...)` — extract the JSON serialization + file write into a free function in `loop_io.rs`. The `RunState` method becomes a thin wrapper that calls it.
- `write_final_log(...)` — same pattern: free function in `loop_io.rs`, thin wrapper on `RunState`.
- `write_attempt_log(...)` — same pattern.
- `write_checkpoint(...)` — extract the atomic temp-file-then-rename logic into a free function in `loop_io.rs`. `RunState` wrapper calls it.

### Semantic preservation rules

Each extracted function must preserve the original error handling exactly:
- **Checkpoint write**: best-effort with atomic pattern — writes to `state.json.tmp` then renames to `state.json`. On serialization or rename failure, `warn!` and return without propagating the error. The atomicity pattern prevents corruption from interrupted writes but is not a hard failure.
- **Plan/final/attempt log writes**: best-effort — `create_dir_all`, serialize, write, silently return on any failure (current behavior).
- **`write_plan_error_artifact`**: returns `Result<(), io::Error>` — caller decides how to handle (currently best-effort with `warn!`).

All three categories are best-effort (no panics, no propagated errors). The only difference is that checkpoint uses an atomic rename pattern to prevent partial writes.

### Imports needed in `loop_io.rs`
```rust
use std::fs;
use std::io;
use std::path::Path;
use serde::Serialize;
use crate::log_schema::{AttemptLog, FinalLog, PlanLog, RunnerLog};
use crate::llm::Usage;
use crate::planner::Plan;
use crate::schema::EditBatch;
use crate::runner::RunResult;
```

Adjust as needed — the exact import set depends on how thin the `RunState` wrappers are.

### Touch points
- `src/loop_io.rs` (new)
- `src/main.rs` (add `mod loop_io;`)
- `src/loop.rs` (thin wrappers, import from `loop_io`)
- `src/log_schema.rs` (remove `write_plan_error_artifact` and its `chrono` import if no longer needed)

### Tests
- Move or duplicate the `write_plan_error_artifact_creates_final_json` test to `loop_io.rs`.
- Add one test for atomic checkpoint write behavior: write succeeds and `state.json.tmp` is cleaned up.
- Add one test for best-effort log write: confirm no panic when directory doesn't exist and can't be created (e.g., path under a file).
- Existing tests in `loop.rs` and `stats.rs` must continue to pass unchanged.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 2 until Task 1 is verified.

---

## Task 2: Unify Run Identity Allocation

### What
Eliminate duplicated run-id/log-dir allocation logic. Introduce a single helper that all call sites use.

### Implementation

Add to `src/loop_io.rs`:

```rust
/// Allocated identity for a new run.
#[derive(Debug, Clone)]
pub struct RunIdentity {
    pub run_id: String,
    pub log_dir: String,
}

/// Allocate a unique run identity. Handles timestamp generation and
/// suffix collision avoidance (`_2`, `_3`, ...).
pub fn allocate_run_identity(project_root: &Path) -> RunIdentity {
    // Move the existing run-id generation logic here.
    // Must preserve: timestamp format, lexical ordering, suffix policy.
    // Return RunIdentity { run_id, log_dir } where log_dir = ".tod/logs/<run_id>"
}
```

### Call sites to update

**`RunState::new(...)` in `loop.rs`:**
- Currently generates `run_id` and `log_dir` inline.
- Replace with `let identity = crate::loop_io::allocate_run_identity(project_root);`
- Use `identity.run_id` and `identity.log_dir`.

**Plan-error path in `run()` in `loop.rs`:**
- Currently duplicates run-id generation.
- Replace with the same `allocate_run_identity` call.

### Constraints
- Preserve current timestamp format and lexical ordering behavior exactly.
- Preserve collision suffix policy (`_2`, `_3`, ...) exactly.
- After this task, there is exactly one place that generates run IDs.

### Tests
- Existing `run_state_new_generates_unique_run_ids` must still pass.
- Add: `plan_error_run_id_uses_same_allocation_policy` — assert that a plan-error artifact's run-id in `final.json` follows the same format and suffix policy as a normal run.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 3 until Task 2 is verified.

---

## Task 3: Extract Resume Fingerprint Compatibility Logic

### What
Move fingerprint mismatch decision logic out of `resume()` into a pure helper function.

### Implementation

Add a helper (in `loop.rs` or `loop_io.rs`, whichever is cleaner):

```rust
#[derive(Debug, PartialEq)]
pub enum FingerprintDecision {
    /// Fingerprints match — safe to resume.
    Match,
    /// Fingerprints mismatch — abort resume.
    Mismatch { expected_hash: String, actual_hash: String },
    /// Legacy v1 match by size only — resume with warning.
    LegacyMatch { warning: String },
}

/// Pure decision function: compare stored vs current fingerprint.
pub fn check_fingerprint_compatibility(
    stored: &Fingerprint,
    current: &Fingerprint,
) -> FingerprintDecision {
    // Move the version-compatibility matrix currently embedded in resume() here.
    // Preserve behavior for: v1 vs v1, v1 vs v2, v2 vs v2, unknown version.
    // Preserve warning text for legacy v1 same-size caveat.
}
```

Update `resume()` to call this helper and act on the returned decision (match -> continue, mismatch -> error, legacy -> warn + continue).

### Constraints
- No change to `LoopError::FingerprintMismatch` semantics.
- No change to resume call order or behavior.
- The helper must be a pure function — no IO, no side effects.

### Tests

Add table-driven tests:

```rust
#[test]
fn fingerprint_compatibility_matrix() {
    // v2 vs v2, same hash -> Match
    // v2 vs v2, different hash -> Mismatch
    // v1 vs v1, same hash -> LegacyMatch with warning
    // v1 vs v1, different hash -> Mismatch
    // v1 vs v2 -> Mismatch (version incompatible)
    // v2 vs v1 -> Mismatch (version incompatible)
    // unknown version -> Mismatch
}
```

Cover each branch. Existing resume tests must continue to pass.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 4 until Task 3 is verified.

---

## Task 4: Compatibility Regression Matrix

### What
Add focused regression coverage that protects compatibility guarantees across all the refactoring in this phase.

### Test cases to add

All tests should use `TempSandbox` and be deterministic.

1. **Legacy checkpoint without `profile` field** — deserialize a checkpoint JSON missing the `profile` key, confirm `RunState` loads with default profile.
2. **Legacy fingerprint without `fingerprint_version` field** — deserialize, confirm v1 assumed.
3. **Legacy attempt log without `runner_output.stage`** — deserialize, confirm `stage` defaults to `"review"`.
4. **Plan-error-only artifact** — write only `final.json` (no `plan.json`), confirm `summarize_run` returns a valid `RunSummary` with `outcome: PlanError`.
5. **Artifact contract parity** — run a complete mock cycle, confirm all three artifact types exist in expected paths: `plan.json`, `final.json`, `step_0_attempt_1.json`.

### Constraints
- Do not weaken any existing assertions.
- Do not change runtime behavior.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 5 until Task 4 is verified.

---

## Task 5: Documentation and Phase Closure

### What
Align docs with Phase 15 structural outcomes.

### Scope
- Update `AGENTS.md`: phase status to Done, baseline test count, any invariant wording changes.
- Update `README.md` if any operator-visible wording needs adjustment (unlikely this phase).
- Write `docs/phase15-implementation-log-<date>.md`.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Expected after Phase 15: tests >= 185 baseline, clippy clean.

---

## Implementation Order Summary

| Task | Scope | Reasoning level |
|---|---|---|
| 1. Create `loop_io.rs` + extract persistence | Establish IO boundary, clean `log_schema.rs` | High |
| 2. Unify run identity allocation | Remove duplication, return struct | Medium-High |
| 3. Extract fingerprint compatibility logic | Make resume safety pure + table-testable | High |
| 4. Compatibility regression matrix | Protect legacy and artifact contracts | Medium |
| 5. Documentation closure | Keep operator docs and phase state trustworthy | Low-Medium |

---

## Out of Scope (Phase 16+)

- Patch/diff edit mode.
- Git branch/worktree isolation.
- Local/offline provider expansion.
- Major reviewer-policy redesign.
- Large stats UX redesign beyond compatibility-driven fixes.
