# Tod Codebase Assessment (Post-Phase 18)

Date: 2026-03-12  
Scope reviewed: full `src/` surface, tests, CLI/runtime contracts, operator docs, and current phase artifacts.

Validation baseline on current tree:
- `cargo test`: **229 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Executive Assessment

Tod is now in a credible alpha state for controlled Rust-project usage.

Post-Phase-18 status:
- Safety-critical execution boundaries remain strong: path validation, transactional apply, rollback, checkpointing, and resume compatibility are intact.
- Operator trust improved materially: request accounting semantics are now aligned with documented behavior, failure surfaces point to precise run logs when available, and stdout/stderr contracts are protected by integration tests.
- The codebase has clear architectural intent: LLM-generated intent is constrained by deterministic Rust logic, and that principle is consistently visible across `schema.rs`, `runner.rs`, `loop.rs`, and the persistence boundary.

Bottom line:
- **Tod is ready for alpha with disciplined operator usage.**
- **The next constraint is not correctness of the core safety model; it is precision and maintainability at larger repo scale.**
- **The two engineering hotspots remain `loop.rs` and `stats.rs`, both because of size and because they are now contract-critical surfaces.**

---

## 2. System Assessment By Layer

## CLI and configuration surface

Strengths:
- `cli.rs` exposes a small, comprehensible command model: `run`, `resume`, `status`, `stats`, and `init`.
- `config.rs` is clean and stable. `RunConfig` is immutable after construction and now carries shared output-policy state (`quiet`) without leaking presentation concerns across the rest of the codebase.
- `run_mode_label()` is centralized, removing a small but real source of drift.

Risks:
- `main.rs` is still responsible for command dispatch and some presentation policy. That is acceptable, but it should stay thin. New output shaping should not accumulate there without clear extraction boundaries.

Assessment:
- Healthy. This layer is not a current blocker.

## Context building and prompting

Strengths:
- `context.rs` is deterministic, budget-aware, and tested against truncation behavior.
- Hidden directories are excluded, and file sampling stays bounded.
- `planner.rs` and `editor.rs` are relatively thin wrappers around prompt construction, parsing, and validation, which makes them easier to reason about.

Risks:
- Context selection is still coarse relative to likely alpha workloads. It is breadth-oriented and budget-limited, but not yet relevance-ranked.
- On larger repos, the limiting factor will be context precision, not safety.
- Prompt logic remains product logic. That is appropriate, but it means seemingly small prompt changes can have outsized behavioral impact and need contract-minded testing.

Assessment:
- Functionally solid for small and medium repos. Large-repo precision is the next meaningful gap.

## Schema validation and edit application

Strengths:
- `schema.rs` remains one of the strongest modules in the system. It enforces relative-only paths, rejects traversal, rejects conflicting edit batches, and protects range semantics.
- `runner.rs` remains the strongest execution module. Transactional apply plus rollback-on-failure is the right design, and the strict-mode pipeline is explicit and predictable.
- Runner output truncation is carefully handled, including UTF-8 and line-boundary behavior.

Risks:
- The edit contract is intentionally narrow: `write_file` and `replace_range`. That keeps safety high, but it also limits precision on larger or noisier edits.
- There is still a theoretical TOCTOU risk under external concurrent mutation, but the current design is sensible for a terminal-first, single-run agent.

Assessment:
- Strong. This subsystem is the foundation to preserve while other capabilities evolve.

## Orchestration, checkpointing, and resume

Strengths:
- `loop.rs` preserves clear runtime invariants around final-log writing, checkpoint behavior, and fingerprint compatibility.
- Phase 18 fixed the main accounting ambiguity: `llm_requests` now reflects observed provider responses rather than attempted calls.
- Error-path teardown is more maintainable after consolidation.
- Resume behavior remains compatibility-conscious and profile-aware.

Risks:
- `loop.rs` is still the largest blast-radius module in the codebase at roughly 3k LOC.
- The file now carries orchestration flow, runtime accounting, lifecycle messaging, report shaping, and much of the run-state mutation logic.
- It is heavily tested, which mitigates risk, but that does not remove the maintenance cost of its size.

Assessment:
- Correctness is good; maintainability remains the main concern.

## Persistence, logs, and reporting

Strengths:
- The `log_schema.rs` / `loop_io.rs` split is clean and worth preserving.
- Best-effort writes and atomic checkpoint replacement are the right operational compromise.
- `final.json` is now the preferred truth source, which simplifies stats reasoning for terminal runs and edge outcomes like `plan_error`.
- Command-level output contract tests now protect stdout/stderr behavior at the actual CLI boundary.

Risks:
- `stats.rs` is now a second large hotspot. It mixes compatibility handling, summarization logic, and human/JSON formatting in one module.
- Every additional reporting feature now has a real chance of making `stats.rs` harder to evolve safely unless extractions remain disciplined.

Assessment:
- Operationally strong, structurally at risk of gradual accumulation.

## LLM provider boundary

Strengths:
- `llm.rs` has a minimal trait boundary and clear usage accounting types.
- Retryability classification is tested and explicit.
- Provider-specific configuration remains constrained.

Risks:
- Single-provider execution is still a product limitation, though not the right next investment.
- There is no richer retry telemetry yet; this is acceptable for now because Phase 18 correctly treated request counting and retry observability as separate concerns.

Assessment:
- Adequate for alpha. Provider expansion should remain deferred.

---

## 3. Module-By-Module Health

| Module | Role | Assessment | Recommended follow-up |
|---|---|---|---|
| `main.rs` | Entry point and command dispatch | Healthy, but should stay presentation-thin. | Do not let new reporting or policy logic accumulate here. |
| `cli.rs` | Clap command model and flag parsing | Healthy and readable. | Keep future flags consistent with existing `RunConfig` conversion patterns. |
| `config.rs` | Stable runtime configuration | Healthy and improving. | Continue centralizing shared labels/defaults here. |
| `context.rs` | Planner/step/retry context assembly | Adequate, deterministic, and tested. | Next improvement should be relevance ranking, not broader collection. |
| `planner.rs` | Plan prompt and validation | Healthy and intentionally narrow. | Preserve prompt/validation symmetry and avoid ad hoc prompt drift. |
| `editor.rs` | Edit prompt and batch parsing | Healthy and aligned with planner flow. | Keep error semantics and provider-contact boundaries explicit. |
| `schema.rs` | Edit contract and path safety | Strongest safety boundary in the repo. | Preserve invariants; avoid broadening the contract casually. |
| `runner.rs` | Transactional apply and cargo pipeline | Strong and well-scoped. | Future work should add evidence surfaces, not weaken rollback guarantees. |
| `reviewer.rs` | Retry/proceed/abort policy | Small, pure, and low risk. | Keep it policy-only. |
| `llm.rs` | Provider trait and Anthropic integration | Adequate and contained. | Defer provider expansion until post-alpha precision work lands. |
| `log_schema.rs` | Pure data and serde defaults | Strong boundary. | Keep it free of IO and formatting. |
| `loop_io.rs` | Persistence and run identity | Strong boundary with good operational semantics. | Preserve best-effort writes and atomic checkpoint behavior. |
| `loop.rs` | Core orchestration state machine | Correct but still the main maintainability hotspot. | Continue extracting pure helpers and keep side effects explicit. |
| `stats.rs` | Read-only summaries and formatting | Useful, heavily tested, but now oversized. | Treat any new stats surface as contract work and extract carefully. |
| `util.rs` / `test_util.rs` | Shared helpers and test sandboxing | Healthy and high leverage. | Reuse rather than duplicating helper logic in future tests. |

---

## 4. Test And Contract Posture

Current strengths:
- The highest-risk runtime modules are also the most tested: `loop`, `schema`, `runner`, and `stats`.
- Compatibility tests cover legacy/defaulted artifacts and edge outcomes.
- Integration coverage now reaches actual command dispatch boundaries in `tests/command_output_contract.rs`.

Remaining gaps:
- There is still no larger-repo fixture matrix that exercises context quality and edit precision under realistic tree shapes.
- The test suite is excellent at correctness and compatibility; it is not yet strong at measuring precision, scope control, or operator usefulness on medium/large repos.
- The alpha process itself needs structured reporting discipline so product gaps can be categorized instead of rediscovered ad hoc.

Assessment:
- This is a high-confidence correctness suite, not yet a high-confidence product-fit suite.

---

## 5. Priority Findings

1. **`loop.rs` remains the primary maintenance risk.**  
   The module is heavily tested and functionally sound, but it still concentrates too much orchestration and report-shaping responsibility in one place.

2. **`stats.rs` is becoming a second contract-heavy hotspot.**  
   It is accurate and well-tested, but additional reporting requirements could make it harder to evolve safely without extraction discipline.

3. **Large-repo precision is the next real product constraint.**  
   The system is safe, but safe broad edits or noisy context windows will still lose operator trust on non-trivial repos.

4. **Tod still lacks deterministic changed-file evidence in its final operator surface.**  
   Logs exist, but an operator should not need to diff the repo or inspect multiple attempt logs to answer "what files did this run actually touch?"

5. **Release and packaging hygiene is slightly behind engineering quality.**  
   `Cargo.toml` still carries a placeholder repository URL. This is small, but it is exactly the kind of metadata mismatch that undermines alpha polish.

---

## 6. Alpha Readiness

Ready now for:
1. Controlled alpha runs on disposable branches or worktrees.
2. Small and medium Rust maintenance tasks with bounded iteration and token caps.
3. Resume, recovery, and postmortem workflows where `.tod` artifacts are preserved.

Not ready yet for:
1. Unattended or high-volume automation across heterogeneous repos.
2. Large-repo claims where context precision and edit scope have not been validated.
3. Broad capability expansion such as patch mode, provider expansion, or git orchestration.

Operational recommendation:
- Run alpha through a structured route and reporting discipline. `docs/alpha-user-test.md` should be treated as part of the product-validation process, not optional process overhead.

---

## 7. Recommended Next Step

The next step should be **Phase 19: Alpha Validation and Precision Scaling**.

Recommended focus:
1. Add deterministic changed-file evidence to run-level reporting and logs.
2. Improve context precision on larger repos without weakening deterministic behavior.
3. Add a repeatable alpha fixture/reporting loop that turns operator runs into actionable engineering input.
4. Keep deferring patch mode, multi-provider support, and git worktree orchestration until the alpha evidence says the current core loop is precise enough.
