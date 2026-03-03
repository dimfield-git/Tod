# PHASE14.md — Observability Schema Cohesion & Metrics Fidelity

Read `AGENTS.md` first. All operating principles and safety rules apply.

---

## Goal

Phase 13 made resume behavior deterministic and materially stronger. Phase 14 should improve operator trust and maintainability by addressing remaining observability and reporting debt.

Primary outcomes:
- Stats no longer depends on loop-internal log struct ownership.
- Planner-stage failures produce terminal artifacts.
- Multi-run stats report explicit infra-failure outcome buckets.
- LLM request counts reflect call semantics more faithfully than usage-presence heuristics.

---

## Baseline (Current Tree)

Validated on 2026-03-03:
- `cargo test`: **178 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**
- Phases 1–13 complete

---

## Task 1: Extract Shared Log Schema

### What
Create a dedicated log-schema module (recommended: `src/log_schema.rs`) and move shared run-log structs out of `loop.rs`:
- `RunnerLog`
- `AttemptLog`
- `PlanLog`
- `FinalLog`

Keep serde compatibility behaviors (`#[serde(default)]`) intact.

### Why
`stats.rs` currently imports these types from `loop.rs`, coupling read-only reporting to orchestration internals.

### Touch points
- `src/log_schema.rs` (new)
- `src/loop.rs`
- `src/stats.rs`

### Tests
- Update affected tests to import from the new schema module.
- Add one compatibility test in either `stats.rs` or `log_schema.rs` ensuring legacy payloads still deserialize.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Expected after Task 1: tests pass, clippy clean.

Do not start Task 2 until Task 1 is verified.

---

## Task 2: Planner-Stage Terminal Artifact

### What
Add terminal artifact emission for planner-stage failures (when `create_plan` fails before `RunState` exists):
- write a `final.json` with `outcome: "plan_error"` and message,
- ensure a deterministic run directory is still created,
- avoid breaking existing success-path log behavior.

Implementation may use a lightweight pre-run log context helper in `loop.rs`.

### Why
Current observability is strong after planning, but pre-RunState failures leave no run artifact trail.

### Touch points
- `src/loop.rs`
- (optional) `src/log_schema.rs` for helper reuse

### Tests
Add:
- `run_plan_error_writes_terminal_artifact`

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 3 until Task 2 is verified.

---

## Task 3: Expand Multi-Run Outcome Aggregates

### What
Increase fidelity in `stats` multi-run reporting by tracking explicit terminal outcome classes:
- success
- aborted
- cap_reached
- token_cap
- edit_error
- apply_error
- plan_error

Keep backward compatibility for legacy logs (no `final.json`) via existing heuristic fallback.

### Why
Current aggregate buckets collapse distinct operational failure classes, reducing usefulness for incident triage.

### Touch points
- `src/stats.rs`

### Tests
Add/adjust tests:
- `summarize_runs_counts_terminal_outcomes`
- update formatting tests for multi-run summaries as needed

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 4 until Task 3 is verified.

---

## Task 4: Request Count Semantics Hardening

### What
Make request counting semantics explicit and consistent with call behavior, not usage-field presence alone.

Recommended direction:
- treat each plan generation as one planner request when planning succeeds,
- treat each attempt log as one edit request,
- usage fields remain token accounting, not request existence.

Document semantics in `README.md` and/or `AGENTS.md` notes.

### Why
Usage metadata can be missing in some paths; counting only usage-bearing calls underreports request activity.

### Touch points
- `src/stats.rs`
- `src/loop.rs` (if logging fields need adjustment)
- `README.md` and/or `AGENTS.md`

### Tests
Add/adjust tests:
- `summarize_run_request_count_independent_of_usage_fields`

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Do not start Task 5 until Task 4 is verified.

---

## Task 5: Light Loop Surface Reduction (Optional but Recommended)

### What
Perform one behavior-preserving extraction from `loop.rs` introduced by Phase 14 scope:
- either logging write helpers,
- or pre-run artifact helper(s).

No semantic changes beyond relocation.

### Why
`loop.rs` remains the primary maintainability hotspot.

### Touch points
- `src/loop.rs`
- new helper module(s) only if extraction is clean and reviewable

### Tests
- Ensure all affected tests still pass.
- Add one regression test for extracted helper behavior if practical.

### Verify
```bash
cargo test
cargo clippy -- -D warnings
```

Expected after Phase 14: tests still pass (target >= baseline), clippy clean.

---

## Implementation Order Summary

| Task | Scope | Reasoning level |
|---|---|---|
| 1. Shared log schema extraction | Coupling reduction | High |
| 2. Planner-stage terminal artifact | Observability closure | High |
| 3. Stats outcome bucket expansion | Reporting fidelity | Medium |
| 4. Request count semantics hardening | Metrics trust | Medium-High |
| 5. Optional light loop extraction | Maintainability | Medium |

---

## Out of Scope (Phase 15+)

- Patch/diff edit mode.
- Git branch isolation workflow.
- New provider classes (local/offline models).
- Major reviewer policy redesign.

