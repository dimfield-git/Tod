# PHASE18.md - Observability Integrity, Operator Control, and Output Contract Reliability

Read `AGENTS.md` first. All operating principles and safety rules apply.

**Implementation order: Tasks must be executed in sequence (1, 2, 3, 4, 5, 6). Do not start Task 4 before Task 2 contract tests are in place. Do not start Task 5 before Task 3 data plumbing is complete.**

---

## Goal

Phase 17 improved operator visibility and output quality. Phase 18 should harden the reliability of those new surfaces by making observability/accounting semantics exact, output controls explicit, and CLI output contracts harder to regress.

Primary outcomes:
- Request/token accounting invariants are enforced under all terminal and pre-LLM failure paths.
- Failure surfaces provide precise run-level log pointers.
- Operators can suppress lifecycle chatter without breaking stdout contracts.
- Command-level stdout/stderr contracts are test-protected for automation confidence.
- `loop.rs` orchestration complexity continues to shrink through pure extraction.

---

## Why This Phase Now

Phase 17 added high-value output surfaces; Phase 18 should ensure those surfaces are exact and durable before broadening capability scope.

Technical pressure:
- Accounting semantics are now operator-visible and need stricter invariant enforcement.
- Output-channel behavior (stdout vs stderr) now carries more product contract significance.
- `loop.rs` still concentrates orchestration and accounting logic.

Product pressure:
- Tooling and operators need predictable output and precise failure navigation.
- Lifecycle messaging is useful, but some workflows need a quiet mode.

---

## Design Decisions (Locked)

1. Preserve existing safety/compatibility behavior unless a task explicitly changes user-visible output.
2. Keep `log_schema.rs` pure data+serde and `loop_io.rs` persistence/identity boundary.
3. No patch-mode/provider expansion/git worktree orchestration in this phase.
4. Stdout remains reserved for command output and JSON payloads.
5. `--quiet` only controls cosmetic lifecycle progress messages; it must not suppress errors.
6. Request-count semantics remain: 1 plan call = 1 request, 1 edit call = 1 request; retries in transport layers do not increment request count.

---

## Baseline (Start of Phase 18)

- `cargo test`: **215 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1-17 complete

---

## Task 1: Accounting Invariant Audit + Helper Extraction

### What
Isolate request/token accounting updates into explicit helper pathways so intent is auditable and testable.

### Scope
- Extract pure/helper logic for accounting transitions in `loop.rs`.
- Ensure request increments occur only at logical LLM intent boundaries.
- Keep side effects (checkpoint/final-log writes) in orchestration flow.

### Constraints
- No behavior change to run outcomes, artifact shape, or checkpoint timing.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 2: Accounting Contract Tests (Terminal-Path Complete)

### What
Add tests that prove accounting semantics across success and failure boundaries.

### Scope
- Add tests covering at minimum:
  - plan success + step success
  - plan failure before `RunState`
  - context/path failures before edit call
  - edit generation failure
  - apply failure
  - token-cap and iteration-cap terminal paths
  - resume terminal paths
- Assert request/token values using `contains`/field assertions without fragile full-string matching.

### Constraints
- Keep request-count semantics consistent with `AGENTS.md`.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 3: Precise Failure Log Pointers

### What
Improve operator recovery speed by surfacing precise log directory pointers on failure.

### Scope
- Enrich error-path reporting so `run`/`resume` failures can include run-level log location when available.
- Preserve typed errors and avoid introducing global mutable state.

### Constraints
- Do not change exit codes.
- Maintain compatibility defaults for legacy checkpoints/artifacts.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 4: Output Policy Control (`--quiet`)

### What
Add operator control over lifecycle progress verbosity.

### Scope
- Add `--quiet` for `run` and `resume`.
- Suppress lifecycle progress banners/messages when enabled.
- Keep stderr errors and stdout command output unchanged.

### Constraints
- `--quiet` is cosmetic only; never affects control flow or return semantics.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 5: Command-Level Output Contract Tests

### What
Protect stdout/stderr behavior from regression in both human and JSON command paths.

### Scope
- Add integration-style tests for:
  - `status` human vs `--json`
  - `stats` human vs `--json`
  - lifecycle message suppression under `--quiet`
  - error output behavior (stderr guidance + stdout cleanliness)

### Constraints
- Avoid brittle exact-whitespace snapshots where possible; assert stable keys/fragments.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 6: Documentation and Phase Closure

### What
Align all operator and planning docs with Phase 18 outcomes.

### Scope
- Update `AGENTS.md` phase status and outcomes.
- Update `docs/runbook.md` for `--quiet`, precise failure pointers, and output contract notes.
- Update `README.md` usage examples and output behavior notes.
- Write `docs/phase18-implementation-log-<date>.md` with verification timeline.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

---

## Out of Scope (Phase 19+)

- Patch/diff edit contract.
- Multi-provider expansion.
- Git worktree orchestration engine.
- Async runtime migration.
- Major reviewer-policy redesign.
