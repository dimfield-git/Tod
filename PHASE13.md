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

## Task 1: Make Fingerprint Truly Checkpoint-Scoped

### What

Ensure each persisted checkpoint stores a fingerprint computed from the workspace state at that checkpoint write, not an earlier run snapshot.

### Implementation direction

Add a `refresh_fingerprint` method on `RunState`:

```rust
fn refresh_fingerprint(&mut self, project_root: &Path) {
    self.fingerprint = compute_fingerprint(project_root);
}
```

Call `state.refresh_fingerprint(&config.project_root)` immediately before every `state.checkpoint(config)` call inside `run_from_state()`. The call sites to cover (line numbers are approximate — match by the nearby exit-path or checkpoint comment):

- Total iteration cap exit (~line 579)
- Edit error exit (~line 627)
- Token cap after edit (~line 643–644)
- Apply error exit (~line 675)
- Proceed → checkpoint (~line 703)
- Retry → checkpoint (~line 715)
- Abort → checkpoint (~line 725)
- Per-step cap exhaustion (~line 743)
- Step advance → checkpoint (~line 764)

Do **not** modify `checkpoint()` itself — its contract stays `&self`. The refresh is the caller's responsibility.

Do **not** add `refresh_fingerprint` calls inside `run()` before the first `state.checkpoint(config)` at ~line 561 — the fingerprint was just computed in `RunState::new()` and no edits have occurred yet.

Do **not** add `refresh_fingerprint` in `resume()` — it already recomputes fingerprint at ~lines 789–796.

### Why

This restores the intended semantic of `fingerprint` as "workspace at last checkpoint," eliminating false-positive drift failures on resume after agent-applied edits.

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

Store immutable execution profile in checkpoint state and use it for resumed attempts.

### Implementation direction

#### What gets persisted

Add a `RunProfile` struct to `loop.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunProfile {
    pub mode: String,           // "default" or "strict"
    pub dry_run: bool,
    pub max_runner_output_bytes: usize,
}
```

Store `mode` as a string (not `RunMode` enum) to avoid coupling the checkpoint format to the enum's `Debug` representation. Conversion helpers:

```rust
impl RunProfile {
    fn from_config(config: &RunConfig) -> Self {
        Self {
            mode: match config.mode {
                RunMode::Default => "default".to_string(),
                RunMode::Strict => "strict".to_string(),
            },
            dry_run: config.dry_run,
            max_runner_output_bytes: config.max_runner_output_bytes,
        }
    }

    fn to_run_mode(&self) -> RunMode {
        match self.mode.as_str() {
            "strict" => RunMode::Strict,
            other => {
                if other != "default" {
                    eprintln!("warning: unknown run mode '{}' in checkpoint, falling back to default", other);
                }
                RunMode::Default
            }
        }
    }
}
```

Add to `RunState`:

```rust
#[serde(default)]
pub profile: Option<RunProfile>,
```

Use `Option` so legacy checkpoints without `profile` deserialize cleanly via `#[serde(default)]`.

#### What does NOT get persisted

- `project_root` — resolved at runtime by the CLI. The resume caller owns this.
- `max_iterations_per_step` / `max_total_iterations` / `max_tokens` — already persisted directly on `RunState`.

#### Where to wire it

**In `RunState::new()`:** Initialize `profile: Some(RunProfile::from_config(config))`.

**In `resume()`:** After deserializing state, if `state.profile` is `Some`, build an effective config that overrides the caller's defaults. `RunConfig` already derives `Clone`:

```rust
let effective_config = if let Some(ref profile) = state.profile {
    RunConfig {
        project_root: config.project_root.clone(),
        mode: profile.to_run_mode(),
        dry_run: profile.dry_run,
        max_runner_output_bytes: profile.max_runner_output_bytes,
        max_iterations_per_step: state.max_iterations_per_step,
        max_total_iterations: state.max_total_iterations,
        max_tokens: state.max_tokens,
    }
} else {
    // Legacy checkpoint: fall back to caller config (current behavior).
    config.clone()
};
```

Then pass `&effective_config` to `run_from_state` instead of `config`. This means `run_from_state` does not change at all — it still reads from config as before.

**In `main.rs` `Command::Resume`:** No changes needed. The default `RunConfig` constructed there is now just a carrier for `project_root`; the profile override in `resume()` handles everything.

#### Log consistency

`write_attempt_log` and `write_plan_log` currently derive `run_mode` from `config.mode` (~lines 281, 344). Since `resume` now passes an effective config with the correct mode, logged `run_mode` will automatically match the originating run. No additional changes needed in the logging helpers.

### Why

Resume must continue the same run semantics. Switching modes mid-run is a behavioral regression and can invalidate observability data.

### Touch points

- `src/loop.rs`
- `src/main.rs` (no changes expected, but in scope for verification)

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

### Implementation direction

- Add a `fingerprint_version` field to `Fingerprint` with `#[serde(default = "default_fingerprint_version")]` where the default returns `1`.
- New fingerprints are created with version `2`.
- For v2, replace the hash material at ~line 98 (`format!("{path}:{size}\n")`) with content bytes: for each file, feed `path` bytes then file content bytes into the hasher. Keep `file_count` and `total_bytes` summary fields unchanged.
- In `resume()` fingerprint comparison: if stored fingerprint is v1 and current is v2, still enforce `file_count` and `total_bytes` drift checks (these catch file additions, deletions, and size changes). Only the hash comparison is skipped during v1→v2 migration, because the hash algorithms differ. Log a warning to stderr: `"legacy v1 fingerprint — same-size drift not detected until next checkpoint upgrade"`. The next `refresh_fingerprint` + `checkpoint` from Task 1 will write a v2, so this is a one-time migration.
- If both sides are v1 (legacy→legacy resume), compare as before with no behavior change.
- If both sides are v2, compare all fields including hash.
- Preserve all existing `Fingerprint` fields so old checkpoints deserialize without error.

### Why

Size-only hashing misses important drift classes (same-size edits). Versioning avoids breaking historical resume data. Keeping summary field checks during migration prevents obvious drift from being silently ignored.

### Touch points

- `src/loop.rs`

### Risk

Medium (migration and compatibility logic). Content hashing is slower than size-only but acceptable for Tod's expected project sizes (small Rust projects).

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

### Implementation direction

- Generate run IDs with sub-second resolution. Use the existing chrono formatting style already used for `run_id` in `RunState::new()`, extended with fractional seconds (e.g. `%.3f` for milliseconds in chrono). Verify the exact chrono format specifier compiles before committing.
- If the target log directory already exists (defensive), append `_2`, `_3`, etc.
- Update `log_dir` format string to match.
- Lexical sort order must be preserved for stats `--last N`.

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

1. Replace non-test `expect` in `main.rs` run branch (~line 31: `.expect("Run command must produce run config")`) with explicit match or `if let` that prints a message to stderr and exits with code 1.
2. Document Phase 13 resume semantics updates and fingerprint version behavior in `AGENTS.md` and `README.md`.
3. Update phase status metadata and baseline test count after implementation completion.

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
| 2. Resume execution profile persistence | Behavioral determinism | `loop.rs` | High |
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
rg -n "fingerprint|checkpoint|resume|run_mode|dry_run|RunProfile" src/loop.rs src/main.rs
rg -n "run_id|log_dir|summarize_runs" src/loop.rs src/stats.rs
```

---

## Out of Scope (Track Next)

Keep these for Phase 14 unless needed for bugfix fallout:

1. Decouple stats log schema from loop internals (`src/stats.rs:8`).
2. Expand multi-run aggregates to report infra failure outcome buckets explicitly.
3. Optional plan-stage terminal artifact for pre-RunState planner failures.
