# Tod Strategic Plan (Post-Phase 13)

Date: 2026-03-03  
Baseline validated on current tree:
- `cargo test`: **178 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Strategic Objective

Move Tod from a strong prototype to a reliable operator-grade terminal agent by prioritizing:
1. observability completeness,
2. schema and module boundary stability,
3. deterministic metrics/reporting semantics,
4. controlled maintainability improvements before major feature expansion.

---

## 2. Current State Snapshot

### What is strong now
- Resume determinism and drift detection materially improved in Phase 13.
- Safety rails are solid (path validation + transactional edit apply).
- Test depth is high and concentrated on critical runtime paths.

### What still limits external trust
- Planner-stage failures have no run artifact trail.
- Stats depends directly on loop-owned log/state structs.
- `loop.rs` remains a concentration hotspot with growing change complexity.

---

## 3. Prioritized Work Inventory

## Priority: Must

| Item | Why it matters | Risk | Effort | Primary files |
|---|---|---|---|---|
| Decouple stats schema from loop internals | Reduces change blast radius and makes log schema evolution safer | Medium | M | `src/loop.rs`, `src/stats.rs`, new shared schema module |
| Add planner-stage terminal artifacts | Closes observability gap for failures before `RunState` exists | Medium | M | `src/loop.rs` (and possibly bootstrap helper) |
| Improve run outcome aggregates in stats | Better operational reporting fidelity | Low-Med | S-M | `src/stats.rs` |
| Clarify/normalize request counting semantics | Prevents operator confusion around billed vs observed calls | Medium | M | `src/loop.rs`, `src/stats.rs`, docs |

## Priority: Should

| Item | Why it matters | Risk | Effort | Primary files |
|---|---|---|---|---|
| Focused decomposition of `loop.rs` helper concerns | Lowers maintenance/regression risk without behavior change | Medium | M | `src/loop.rs` + extracted helper module(s) |
| Harden JSON extraction against multi-block markdown noise | Better resilience to model response variance | Medium | S-M | `src/schema.rs` |
| Add explicit docs parity checks to phase completion workflow | Prevents documentation drift | Low | S | `AGENTS.md`, phase files, docs |

## Priority: Nice / Later

| Item | Why it matters | Risk | Effort | Primary files |
|---|---|---|---|---|
| Patch/diff edit mode | Could cut token cost and improve precision | High | L | `schema.rs`, `editor.rs`, `runner.rs` |
| Git branch isolation | Safer worktree handling for real projects | High | L | new git integration layer + loop/runner |
| Local model provider support | Cost/offline flexibility | High | L | `llm.rs`, config/CLI/docs |
| Optional planner reflection pass | Improves plan quality for complex goals | Medium | M | `planner.rs`, `loop.rs` |

---

## 4. Recommended Phase Sequence

## Phase 14 (next): Observability and Schema Cohesion

Primary target:
- operational integrity over feature expansion.

Proposed scope:
1. shared log schema extraction used by both loop writer and stats reader,
2. planner-stage failure terminal artifact support,
3. stats aggregate outcome expansion (infra failures explicit),
4. request-count semantics hardening and documentation.

Expected impact:
- lower maintenance risk,
- better forensic completeness,
- higher operator trust in metrics.

## Phase 15: Loop Maintainability and Compatibility Hardening

Primary target:
- reduce orchestration change risk while keeping behavior stable.

Proposed scope:
1. extract selected `loop.rs` helper concerns (checkpoint/log write helpers, compatibility checks),
2. retain backward compatibility for existing artifacts,
3. add regression-focused tests for extracted boundaries.

## Phase 16: Model IO Robustness and Input Surface Hardening

Primary target:
- improve resilience to messy LLM responses and mixed-content repositories.

Proposed scope:
1. stronger `extract_json` behavior for multi-block responses,
2. optional handling path for non-UTF8 file context rendering,
3. validation/path edge-case regression tests.

---

## 5. Success Criteria for Next 1-2 Phases

By end of Phase 14/15, Tod should have:
- complete terminal artifact coverage for all major failure classes,
- a shared stable log schema independent of loop internals,
- clearer operator metrics semantics,
- reduced risk of regressions in orchestration refactors.

If these are met, the codebase is in a safer position to pursue larger feature bets (patch mode, git isolation, additional providers).

---

## 6. Decision Guidance

When forced to choose between reliability and new capability in the next cycle:
- choose reliability until observability and schema-coupling debt is retired.

Reason:
- Tod’s core value proposition is trustworthy autonomous iteration in a terminal.
- Trust erosion from ambiguous logs/metrics or brittle orchestration has higher product cost than delayed feature breadth.

