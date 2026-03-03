# PHASE13.md — Resume Determinism & Drift Hardening

**Read `AGENTS.md` first.** All operating principles and safety rules apply.

---

## Goal

Make resume behavior deterministic and trustworthy under real edit history, not only dry-run paths.  
After Phase 12, terminal outcome observability is strong, but core resume integrity still has correctness gaps:

1. Checkpoint fingerprint data can become stale relative to the actual workspace.
2. Resume can silently run with a different execution profile than the original run.
3. Drift detection is still vulnerable to same-size file changes.

Phase 13 should ensure:
- Resume does not require `--force` after agent-generated edits when no external drift occurred.
- Resumed execution uses the same operational profile (mode/dry-run/output policy) as the originating run.
- Drift checks are materially stronger while remaining backward-compatible with legacy checkpoints.

---

## Baseline (Current Tree)

Validated on 2026-03-03:
- `cargo test`: **169 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1–12 complete

---

## Deep Assessment (Post-Phase-12)

### What Is Working Well

1. **Safety/observability fundamentals are strong.**
   - Path and traversal safety is strict, including symlink-aware checks (`src/schema.rs`).
   - Edit application is transactional with rollback (`src/runner.rs`).
   - Every post-plan terminal path now writes `final.json` and pre-runner failures are logged (`src/loop.rs`).

2. **Operational diagnostics are materially improved.**
   - Stats prefers terminal outcome from `final.json` while retaining legacy fallback (`src/stats.rs`).
   - Error decisions (`review_decision: "error"`) are no longer conflated with aborts.

3. **Test coverage is broad and behavior-focused.**
   - Extensive unit tests across orchestration, schema hardening, runner behavior, and stats compatibility.

### Material Gaps

1. **Checkpoint fingerprint does not currently track checkpoint reality.**
   - `fingerprint` is initialized in `RunState::new` (`src/loop.rs:215-219`) and only refreshed in `resume` (`src/loop.rs:789-797`).
   - `checkpoint()` is called many times (`src/loop.rs`) without refreshing fingerprint first.
   - Consequence: resume can report drift even when only agent-applied changes occurred after earlier checkpoints.

2. **Resume execution profile can drift from originating run.**
   - `Resume` CLI has only `--project` and `--force` (`src/cli.rs:57-66`).
   - `main` constructs default `RunConfig` for resume (`src/main.rs:70-73`).
   - Attempt logs and planner logs derive `run_mode` from `config.mode` (`src/loop.rs:281`, `src/loop.rs:344`), and pipeline execution uses `config` (`src/loop.rs:685`).
   - Consequence: a run started in strict mode (or dry-run) can resume under different semantics.

3. **Fingerprint model is still size/path-only.**
   - Hash input uses `(relative_path, file_size)` only (`src/loop.rs:50-52`, `src/loop.rs:96-99`).
   - Same-size content edits can evade drift detection.

4. **Run identity is second-resolution and collision-prone.**
   - `run_id` uses `%Y%m%d_%H%M%S` (`src/loop.rs:216-217`).
   - Multiple runs created in the same second can collide on `.tod/logs/<run_id>/`.

5. **Minor invariant debt remains in CLI entrypoint.**
   - Non-test `expect` still present in main run branch (`src/main.rs:31`).

6. **Stats layering remains coupled to loop internals (tracked, but deferred).**
   - `stats.rs` imports log/state structs directly from `loop` (`src/stats.rs:8`).
   - This increases schema-change friction but is not the highest reliability risk for this phase.

---

## Proposed Phase 13 Scope

Theme: **Resume Correctness First**

Five tasks, in order. Tasks 1–3 are core reliability fixes. Task 4 hardens log identity. Task 5 closes invariants/docs.

---

## Task 1: Make Fingerprint Truly Checkpoint-Scoped

### What

Ensure each persisted checkpoint stores a fingerprint computed from the workspace state at that checkpoint write, not an earlier run snapshot.

Implementation direction:
- Refresh fingerprint immediately before checkpoint persistence on run/resume progress paths.
- Keep behavior best-effort for write failures, but do not persist stale fingerprint after successful recompute.

### Why

This restores the intended semantic of `fingerprint` as “workspace at last checkpoint,” eliminating false-positive drift failures on resume.

### Touch points

- `src/loop.rs`

### Risk

Medium (more frequent fingerprint computation; verify no noticeable regression in normal test runs).

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `checkpoint_refreshes_fingerprint_after_edit` | Fingerprint in `state.json` changes after workspace file content changes and checkpoint write. |
| `resume_after_agent_edit_without_force_succeeds` | Resume does not raise `FingerprintMismatch` when workspace matches last checkpoint produced by the same run. |

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

Expected: **171 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 2 until Task 1 is verified.**

---

## Task 2: Persist and Reuse Run Execution Profile on Resume

### What

Store immutable execution profile in checkpoint state and use it for resumed attempts:
- run mode (`default` / `strict`)
- dry-run behavior
- runner output cap policy

Implementation direction:
1. Add a serialized run profile to `RunState` with serde defaults for legacy checkpoints.
2. On new run, initialize profile from `RunConfig`.
3. In `resume`, prefer checkpoint profile as execution source of truth.
4. Keep log `run_mode` consistent with effective profile.

### Why

Resume must continue the same run semantics. Switching modes mid-run is a behavioral regression and can invalidate observability data.

### Touch points

- `src/loop.rs`
- `src/main.rs`
- (tests likely in `src/loop.rs` and/or `src/main.rs`)

### Risk

Medium-high (state schema extension and resume behavior migration).

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `resume_preserves_strict_mode_in_attempt_log` | Resumed attempt log still reports `run_mode: "strict"` for a strict-origin run. |
| `resume_preserves_dry_run_behavior` | Resuming a dry-run does not apply filesystem edits. |
| `legacy_state_without_profile_still_resumes` | Legacy checkpoint without profile deserializes and resumes using compatibility defaults. |

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

Expected: **174 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 3 until Task 2 is verified.**

---

## Task 3: Upgrade Drift Detection to Content-Aware Fingerprint (Versioned)

### What

Introduce fingerprint versioning and a content-aware algorithm (v2) while keeping legacy checkpoint compatibility.

Implementation direction:
- Add `fingerprint_version` (serde default for old checkpoints).
- Preserve v1 comparison behavior for legacy checkpoints as needed.
- Use v2 for new checkpoints/runs, with hash material that includes content bytes (not only size/path).

### Why

Size-only hashing misses important drift classes (same-size edits). Versioning avoids breaking historical resume data.

### Touch points

- `src/loop.rs`

### Risk

Medium (migration and compatibility logic).

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `fingerprint_v2_detects_same_size_change` | Same-size content mutation changes fingerprint hash. |
| `resume_legacy_fingerprint_version_compatible` | Legacy checkpoint fingerprint format still resumes correctly under compatibility path. |

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

Expected: **176 passing, 1 ignored.**

**Codex reasoning level: high**

**Do not start Task 4 until Task 3 is verified.**

---

## Task 4: Harden Run ID Uniqueness

### What

Eliminate same-second log directory collisions.

Implementation direction:
- Generate run IDs with higher resolution and/or deterministic collision suffixing when target log dir already exists.
- Preserve existing lexical ordering expectations for recent-run sorting.

### Why

Log identity collisions can overwrite artifacts or merge unrelated runs, undermining trust in run history.

### Touch points

- `src/loop.rs`
- potentially `src/stats.rs` tests for sort assumptions

### Risk

Low-medium.

### Tests

Add:

| Test | Assertion |
|------|-----------|
| `run_state_new_generates_unique_run_ids` | Two immediate run states generate distinct `run_id`/`log_dir`. |
| `run_id_sorting_remains_stable_for_stats` | Recent-run ordering behavior remains deterministic for `stats --last N`. |

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

Expected: **178 passing, 1 ignored.**

**Codex reasoning level: medium**

**Do not start Task 5 until Task 4 is verified.**

---

## Task 5: Close Invariant Debt and Documentation

### What

1. Replace non-test `expect` in `main` run branch with explicit error handling path.
2. Document Phase 13 resume semantics updates and fingerprint version behavior.
3. Update phase status metadata after implementation completion.

### Touch points

- `src/main.rs`
- `AGENTS.md`
- `README.md`

### Why

Closes a known invariant exception and keeps operator-facing docs aligned with runtime behavior.

### Risk

Low.

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

No required test count increase in this task.

**Codex reasoning level: low**

---

## Implementation Order Summary

| Task | Scope | Files touched | Reasoning level |
|------|-------|---------------|-----------------|
| 1. Checkpoint fingerprint freshness | Resume correctness | `loop.rs` | High |
| 2. Resume execution profile persistence | Behavioral determinism | `loop.rs`, `main.rs` | High |
| 3. Fingerprint v2 + versioning | Drift detection quality | `loop.rs` | High |
| 4. Run ID uniqueness hardening | Log identity safety | `loop.rs`, `stats.rs` tests | Medium |
| 5. Invariant/doc closure | Consistency | `main.rs`, docs | Low |

**Do not start a later task until the preceding task is verified passing.**

---

## Verification Plan

After each task:

```bash
cargo test
cargo clippy -- -D warnings
```

Target after Phase 13:
- `cargo test`: **178+ passed, 1 ignored**
- `cargo clippy -- -D warnings`: clean

Suggested targeted checks:

```bash
rg -n "fingerprint|checkpoint|resume|run_mode|dry_run" src/loop.rs src/main.rs
rg -n "run_id|log_dir|summarize_runs" src/loop.rs src/stats.rs
```

---

## Out of Scope (Track Next)

Keep these for Phase 14 unless needed for bugfix fallout:

1. Decouple stats log schema from loop internals (`src/stats.rs:8`).
2. Expand multi-run aggregates to report infra failure outcome buckets explicitly.
3. Optional plan-stage terminal artifact for pre-RunState planner failures.

---

## Recommended Decision

Proceed with Phase 13 as scoped above: **resume determinism and drift hardening first**.  
This sequence addresses the highest remaining correctness risks before structural cleanup work.
