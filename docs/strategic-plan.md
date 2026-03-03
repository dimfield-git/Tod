# Tod Strategic Plan (Post-Phase 16)

Date: 2026-03-03
Baseline validated on current tree:
- `cargo test`: **203 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Strategic Objective

Evolve Tod from a strong prototype into a dependable daily Rust maintenance agent with:
1. constrained deterministic execution,
2. high compatibility confidence,
3. operator-grade observability and workflow safety.

Near-term objective:
- deepen reliability and maintainability,
- improve machine-usable and operator-usable observability,
- defer broad feature-surface expansion until core loop complexity is reduced further.

---

## 2. Current Position (After Phase 16)

What is now true:
- Phase 16 usability/safety deliverables are complete.
- Operator runbook and mode guidance exist.
- Dirty-workspace warning protects real workflow awareness without blocking runs.
- JSON output for `status`/`stats` exists for automation consumers.

What remains the main risk:
- `src/loop.rs` orchestration concentration and long-term change blast radius.

---

## 3. Path Options From Here

### Path A: Reliability + Observability Depth (recommended)
- Continue behavior-preserving orchestration extraction.
- Strengthen stats/output contract tests and operational telemetry quality.
- Keep compatibility invariants strict.

### Path B: Capability expansion first
- Add patch-mode/provider expansion immediately.
- Higher upside, higher regression risk while orchestration complexity remains concentrated.

### Path C: Distribution-first
- Push packaging/adoption before deeper internals hardening.
- Risks exposing rough edges to wider users prematurely.

Recommendation:
- Execute Path A for Phase 17, then reassess capability expansion.

---

## 4. Work Inventory

## Must

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Continue `loop.rs` surface reduction (pure-helper extractions) | Lower regression risk and review complexity | Medium | M | `loop.rs`, tests |
| Strengthen observability contracts (human + JSON stability) | Better operator trust and automation safety | Low-Med | S-M | `stats.rs`, CLI/tests/docs |
| Preserve compatibility constraints with explicit regression checks | Prevent legacy artifact drift | Medium | S-M | `stats.rs`, `loop_io.rs`, tests |
| Define UX integration seam for future input | Enables later UX improvements without rework | Low | S | docs + phase planning |

## Should

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Add richer run-level telemetry summaries | Better root-cause and trend analysis | Medium | M | `stats.rs`, log artifacts |
| Add additional contract tests for terminal outcomes and JSON keys | Reduce accidental output drift | Low | S-M | tests |

## Deferred (Post-Phase 17)

| Item | Reason deferred |
|---|---|
| Patch/diff edit contract | Higher behavior-surface risk; better after loop/stats hardening. |
| Multi-provider expansion | Better after telemetry and orchestration simplification. |
| Git worktree orchestration engine | Product-surface expansion, not immediate reliability priority. |

---

## 5. Proposed Roadmap

## Phase 17 (next): Observability Fidelity + Orchestration Maintainability

Primary outcomes:
1. Improve machine-consumable observability and contract confidence.
2. Continue small, behavior-preserving decomposition of orchestration logic.
3. Keep all safety and compatibility invariants intact.
4. Reserve an explicit UX input slot for requirements to be supplied later.

Definition of done:
- Quality gates clean.
- No compatibility regression for legacy artifacts/checkpoints.
- Output contract behavior (human + JSON) documented and test-protected.

## Phase 18 (candidate): Precision and Scale Improvements

Candidate outcomes:
1. Context relevance improvements for larger repos.
2. Measured reduction in broad rewrite patterns.
3. Potential preparation work for future patch-mode contract.

## Phase 19 (candidate): Backend Flexibility

Candidate outcomes:
1. Provider optionality.
2. Operational docs for backend selection.
3. Telemetry consistency across providers.

---

## 6. Decision Summary

Tod is now robust enough that the highest ROI is disciplined operational depth, not rapid feature breadth.

Recommended immediate direction:
- Phase 17 should reinforce observability and maintainability while preserving strict safety/compatibility guarantees, and should include reserved room for UX requirements to be integrated once supplied.
