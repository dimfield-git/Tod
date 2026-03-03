# Tod Strategic Plan (Post-Phase 17)

Date: 2026-03-03

Baseline validated on current tree:
- `cargo test`: **215 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Strategic Objective

Evolve Tod from a strong prototype into a dependable daily Rust maintenance agent with:
1. constrained deterministic execution,
2. compatibility-first artifact behavior,
3. operator-grade observability and control,
4. predictable automation-facing contracts.

Near-term objective:
- deepen observability integrity and control surfaces,
- continue reducing orchestration change risk,
- defer broad capability expansion until core loop/accounting confidence is higher.

---

## 2. Current Position (After Phase 17)

What is now true:
- Phase 17 outcomes are complete (observability hardening, lifecycle messaging, actionable errors, enriched completion output, CLI help improvements).
- Compatibility and safety invariants remain intact.
- Operator-facing run behavior is significantly more transparent.

Primary residual risk:
- `src/loop.rs` remains a central orchestration hotspot, now also carrying richer lifecycle and accounting responsibilities.

Secondary risk:
- Request/usage observability semantics require stricter enforcement under all terminal and pre-LLM failure paths.

---

## 3. Path Options From Here

### Path A: Observability Integrity + Operator Control (recommended)
- Strengthen request/usage accounting invariants and command-level output contracts.
- Add precise failure-location guidance (`run_id`/log path fidelity).
- Add controlled lifecycle-output ergonomics (`--quiet` or equivalent policy) without breaking stdout contracts.
- Continue behavior-preserving `loop.rs` extraction.

### Path B: Capability expansion first
- Add patch-mode/provider expansion immediately.
- Upside exists, but regression risk is elevated while orchestration and accounting surfaces are still concentrated.

### Path C: Distribution-first
- Prioritize packaging/adoption now.
- Risks exposing avoidable operator and automation sharp edges.

Recommendation:
- Execute Path A for Phase 18, then re-evaluate capability expansion.

---

## 4. Work Inventory

## Must

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Enforce request-count/usage invariants under all run paths | Trustworthy telemetry and budgeting | Medium | M | `loop.rs`, tests, docs |
| Add precise per-run failure log pointers | Faster operator recovery | Low-Med | M | `loop.rs`, `main.rs`, docs |
| Expand stdout/stderr contract tests for CLI flows | Prevent automation regressions | Low-Med | M | tests, `main.rs`, `stats.rs` |
| Continue `loop.rs` pure-helper extraction | Lower maintenance blast radius | Medium | M | `loop.rs`, tests |

## Should

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Add optional lifecycle-output suppression policy (`--quiet`) | Better scripting ergonomics | Low-Med | S-M | `cli.rs`, `main.rs`, `loop.rs` |
| Add deterministic post-run touched-file summary (no VCS dependency) | Better operator outcome visibility | Medium | M | `runner.rs`/`loop.rs`/logs/tests |

## Deferred (Post-Phase 18)

| Item | Reason deferred |
|---|---|
| Patch/diff edit contract | Wider behavioral surface; defer until observability/control hardening completes. |
| Multi-provider expansion | Better after stable telemetry/comparison surfaces exist. |
| Git worktree orchestration engine | Strategic product expansion, not immediate reliability priority. |

---

## 5. Roadmap

## Phase 18 (next): Observability Integrity + Operator Control

Primary outcomes:
1. Request/usage accounting semantics are reliable and test-proven under all error/terminal paths.
2. Failure messages and CLI output provide precise, actionable per-run log guidance.
3. Lifecycle-output behavior is operator-controllable without violating stdout contracts.
4. `loop.rs` decision logic continues to shrink through pure extraction and targeted table tests.
5. Command-level output contracts (stdout JSON/human + stderr lifecycle/error) are protected by integration tests.

Definition of done:
- Quality gates clean.
- Existing compatibility defaults preserved.
- Contract tests cover command-level output behavior and accounting semantics.

## Phase 19 (candidate): Large-Repo Precision and Edit Granularity

Candidate outcomes:
1. Better context relevance selection and reduced broad rewrites.
2. Deterministic file-change summaries and larger-repo benchmark fixtures.
3. Preparation for future patch-mode introduction.

## Phase 20 (candidate): Backend Flexibility

Candidate outcomes:
1. Provider optionality with consistent usage/request telemetry semantics.
2. Backend operational docs and compatibility expectations.

---

## 6. Decision Summary

Tod is ready for another reliability-focused phase.

Recommended immediate direction:
- Prioritize Phase 18 observability/accounting integrity and operator control.
- Defer major feature-surface expansion until those contracts are hardened and stable.
