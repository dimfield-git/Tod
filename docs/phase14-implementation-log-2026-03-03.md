# Phase 14 Implementation Log (2026-03-03)

Date: 2026-03-03 (UTC)  
Scope: Observability Schema Cohesion & Metrics Fidelity (`PHASE14.md`)

## Baseline Before Phase 14

- `cargo test`: 178 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

## Task 1: Extract Shared Log Schema

### Goal
Move shared run-log types out of `loop.rs` into a dedicated schema module to reduce coupling and prepare for cross-module log consumption.

### Implementation

Added [`src/log_schema.rs`](../src/log_schema.rs):

- `RunnerLog`
- `AttemptLog`
- `PlanLog` (visibility widened to `pub`)
- `FinalLog` (visibility widened to `pub`)
- `default_runner_stage()` for serde default resolution

Updated module wiring/imports:

- [`src/main.rs`](../src/main.rs): added `mod log_schema;`
- [`src/loop.rs`](../src/loop.rs): removed in-module log structs; imported from `crate::log_schema`
- [`src/stats.rs`](../src/stats.rs): now imports log types from `crate::log_schema`, keeps `RunState` import from `loop`

Added compatibility coverage in [`src/log_schema.rs`](../src/log_schema.rs):

- `legacy_payloads_deserialize_with_defaults`

### Verification After Task 1

- `cargo test`: 179 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 2: Planner-Stage Terminal Artifact

### Goal
Ensure planner failures that occur before `RunState` construction still emit terminal observability artifacts (`final.json`) with `outcome: "plan_error"`.

### Implementation

Added helper in [`src/log_schema.rs`](../src/log_schema.rs):

- `write_plan_error_artifact(log_dir, run_id, message) -> Result<(), std::io::Error>`

Updated [`run()` in `src/loop.rs`](../src/loop.rs):

- Wrapped `create_plan(...)` in explicit error handling.
- On plan error:
  - generated run-id/log-dir using existing run-id logic shape,
  - wrote `.tod/logs/<run_id>/final.json` via `write_plan_error_artifact(...)`,
  - treated artifact write as best-effort (`warn!` on write failure),
  - preserved original return semantics: `Err(LoopError::Plan(error))`.

Added tests:

- [`src/log_schema.rs`](../src/log_schema.rs): `write_plan_error_artifact_creates_final_json`
- [`src/loop.rs`](../src/loop.rs): `run_plan_error_writes_terminal_artifact`

### Verification After Task 2

- `cargo test`: 181 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 3: Expand Multi-Run Outcome Aggregates

### Goal
Replace coarse aggregate counters with explicit terminal-outcome buckets across all `RunOutcome` variants.

### Implementation

Updated [`MultiRunSummary` in `src/stats.rs`](../src/stats.rs):

- retained: `runs_total`, `runs_succeeded`, `runs_aborted`, `runs_cap_reached`
- added:
  - `runs_token_cap`
  - `runs_edit_error`
  - `runs_apply_error`
  - `runs_plan_error`

Updated [`summarize_runs()` in `src/stats.rs`](../src/stats.rs):

- explicit per-variant matching for all seven outcomes.

Updated [`format_multi_run_summary()` in `src/stats.rs`](../src/stats.rs):

- dynamically renders outcome breakdown
- always includes `Succeeded`
- includes other outcome buckets only when non-zero

Added/updated tests in [`src/stats.rs`](../src/stats.rs):

- `summarize_runs_counts_terminal_outcomes`
- updated aggregate/empty assertions for new counters

### Verification After Task 3

- `cargo test`: 182 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 4: Request Count Semantics Hardening

### Goal
Make request counting reflect logical LLM intents, independent of usage field presence.

### Implementation

Updated [`summarize_run()` in `src/stats.rs`](../src/stats.rs):

- `llm_requests_plan = 1`
- `llm_requests_edit = attempt_logs.len() as u64`
- `llm_requests_total = plan + edit`

Updated [`format_run_summary()` in `src/stats.rs`](../src/stats.rs):

- removed legacy planner-usage suffix branch and related formatting path

Added/updated tests in [`src/stats.rs`](../src/stats.rs):

- `summarize_run_request_count_independent_of_usage_fields`
- updated legacy-plan test expectations to logical request semantics
- replaced legacy annotation assertion with `format_run_summary_does_not_show_legacy_annotation`

### Phase-Integrity Follow-up

Because Task 2 introduces planner-failure runs that may have `final.json` without `plan.json`, stats needed a compatible read path to avoid dropping those runs from multi-run summaries.

Updated [`summarize_run()` in `src/stats.rs`](../src/stats.rs):

- if `plan.json` is missing but `final.json.outcome == "plan_error"`, synthesize a minimal `RunSummary` with:
  - `goal = "(plan unavailable)"`
  - `outcome = PlanError`
  - zero attempts/tokens
  - logical request counts (`1 plan`, `0 edit`)

Added tests:

- `summarize_run_plan_error_without_plan_log`
- `summarize_runs_includes_plan_error_without_plan_log`

### Verification After Task 4 (+ follow-up)

- `cargo test`: 185 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 5: Light Loop Surface Reduction (Conditional)

Decision: **Skipped** per Phase 14 rule.

Rationale:

- Tasks 1-2 removed shared log-schema ownership from `loop.rs` and moved pre-run artifact helper out of orchestration.
- Additional extraction was not necessary for this phase’s maintainability objective and would have added low-value churn.

Verification after skip decision:

- `cargo test`: 185 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Files Changed in Phase 14

- [`src/main.rs`](../src/main.rs)
- [`src/log_schema.rs`](../src/log_schema.rs) (new)
- [`src/loop.rs`](../src/loop.rs)
- [`src/stats.rs`](../src/stats.rs)

## Final Outcome

Phase 14 completed with all primary outcomes delivered:

- shared log schema decoupled from orchestration internals,
- planner-stage failures now emit terminal artifacts,
- multi-run stats expose explicit terminal outcome buckets,
- request counts now follow logical-call semantics,
- plan-error artifacts without `plan.json` are represented in stats.

Final validation state:

- `cargo test`: 185 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean
