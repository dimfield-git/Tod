# PHASE19.md — Alpha Validation and Precision Scaling

Read `AGENTS.md` first. All operating principles and safety rules apply.

**Implementation order: Tasks must be executed in sequence (1 → 2 → 3 → 4 → 5). Stop after each task and continue only if `cargo test` and `cargo clippy -- -D warnings` both pass.**

---

## Goal

Use real alpha runs to improve the next trust boundary: precision and evidence on medium and large Rust repos.

Primary outcomes:
- Operators can see what a run changed without manual repo archaeology.
- Context assembly is more relevant and less noisy on larger repos.
- Alpha runs follow a repeatable route with disciplined reporting.
- Any new reporting surface is contract-tested before it becomes operator-facing.

---

## Why This Phase Now

Phase 18 fixed the trustworthiness problems that would have made alpha feedback noisy: accounting semantics, failure log pointers, lifecycle output control, and command-boundary output contracts.

That shifts the next bottleneck.

Tod is now safe enough and observable enough to use in alpha, but the next product question is not "did it run?" It is:
- did it choose the right files,
- did it keep changes narrow enough,
- can an operator explain what changed from the normal output and artifacts.

Phase 19 should turn alpha usage into structured engineering input and close the precision gaps it exposes.

---

## Design Decisions (Locked)

1. Preserve path safety, transactional apply, rollback, checkpoint compatibility, and best-effort persistence semantics.
2. No patch mode, provider expansion, git worktree orchestration engine, or async runtime this phase.
3. Any changed-file evidence must come from Tod's own applied edit data, not from VCS assumptions.
4. Context selection changes must remain deterministic and table-testable.
5. Stdout remains reserved for command output and machine-readable JSON payloads.
6. New report or stats fields require contract coverage at the module boundary and the command boundary.

---

## Baseline (Start of Phase 19)

- `cargo test`: **229 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1–18 complete
- Alpha operations route documented in `docs/alpha-user-test.md`

---

## Task 1: Deterministic Changed-File Evidence

### Problem

Tod can already tell the operator whether a run succeeded, how many steps and iterations it used, and where the logs live. It still does not answer the most practical alpha question directly:

`What files did this run actually touch?`

Today the operator must inspect diffs or multiple attempt logs to answer that question.

### Fix

- Track run-level touched relative paths from successful applied edit batches.
- Surface that evidence in the final run summary and `final.json`.
- Keep the representation deterministic:
  - stable ordering,
  - no VCS dependency,
  - no best-guess filesystem scans after the fact.

### Tests

- Run-level touched files are stable and deduplicated.
- Dry-run behavior does not report files as changed.
- Final/report surfaces remain backward-compatible where practical.
- Any new JSON keys are protected by contract tests.

### Constraints

- Do not infer touched files from git diff.
- Do not weaken rollback semantics.
- Do not add noisy file lists to stderr lifecycle messages.

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 2: Context Precision Pass For Medium/Large Repos

### Problem

The current context builders are deterministic and budget-aware, but they are still breadth-oriented. On larger repos that becomes the limiting factor: too much generic context and not enough relevance.

### Fix

- Introduce deterministic file-selection heuristics for planner and editor context construction.
- Favor signals that are already available locally:
  - files named in the plan,
  - recently failed files,
  - recently touched files,
  - Rust source over broad tree inventory when budget is tight.
- Extract ranking and selection logic into pure helpers with table tests.

### Tests

- Table tests for ranking and selection rules.
- Regression tests showing bounded context size with more relevant file inclusion on synthetic larger trees.
- Existing context budget and truncation tests remain green.

### Constraints

- No fuzzy or nondeterministic ranking.
- No new dependencies.
- Keep hidden-dir exclusion and path safety behavior unchanged.

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 3: Alpha Triage Summary Surfaces

### Problem

Alpha reports should be faster to file and compare. Right now the operator can inspect logs, but recurring issues still take too much manual synthesis.

### Fix

- Extend run/report surfaces only where they reduce operator ambiguity:
  - touched-file count or preview,
  - clearer failure-stage summaries where useful,
  - report fields that map cleanly into `docs/alpha-user-test.md`.
- Keep derivation in the runtime/log layer and formatting in the presentation layer.

### Tests

- Module tests for new summary fields.
- Command-boundary tests for human and JSON output behavior.
- Backward-compatibility tests for legacy artifacts where applicable.

### Constraints

- Do not let `stats.rs` grow without extraction where it improves maintainability.
- Do not break existing machine-readable keys without explicit intent.

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 4: Large-Repo Fixture And Regression Coverage

### Problem

Tod's current tests are excellent at correctness, but they do not yet provide enough reusable evidence about precision on more realistic repo shapes.

### Fix

- Add reusable fixture trees that simulate medium/large Rust repos.
- Add regression tests for:
  - context precision,
  - touched-file evidence,
  - command/report output when runs touch multiple files.
- Keep tests deterministic and cheap enough for normal `cargo test`.

### Constraints

- Favor small synthetic fixtures over heavyweight real-world vendored repos.
- Avoid brittle whitespace snapshots; assert fields and stable fragments.

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```

---

## Task 5: Documentation And Phase Closure

### What

- Update `docs/codebase-assessment.md`, `docs/strategic-plan.md`, and `docs/runbook.md` to reflect the implemented Phase 19 behavior.
- Record the implementation in a dated phase log.
- Update `AGENTS.md` with Phase 19 outcomes and the next planned phase.

### Constraints

- Treat operator-facing docs as part of done, not follow-up work.
- Keep runtime behavior and docs aligned.

### Verify

```bash
cargo test
cargo clippy -- -D warnings
```
