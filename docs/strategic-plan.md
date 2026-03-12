# Tod Strategic Plan (Post-Phase 18)

Date: 2026-03-12

Baseline validated on current tree:
- `cargo test`: **229 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Strategic Objective

Evolve Tod from a strong prototype into a dependable alpha agent for real Rust maintenance work with:
1. constrained deterministic execution,
2. compatibility-first artifact behavior,
3. operator-grade observability and control,
4. trustworthy evidence about what a run actually changed.

Near-term objective:
- execute alpha with discipline,
- improve precision on medium and large repos,
- keep the safety model intact while resisting premature feature expansion.

---

## 2. Current Position (After Phase 18)

What is now true:
- Phase 18 completed the key trust repairs needed for alpha: accounting semantics, error-path consolidation, precise failure log pointers, quiet-mode lifecycle control, and command-boundary output contract tests.
- The product is stronger operationally than it was in Phase 17. Failed runs are easier to diagnose, and the output surface is more trustworthy.
- Safety invariants remain intact: strict schema validation, transactional apply, rollback, checkpointing, and compatibility defaults all remain in place.

Primary residual risk:
- The next product limit is precision, not raw capability. On larger repos, context selection and change-scope evidence will decide whether operators trust the agent.

Secondary residual risk:
- `loop.rs` and `stats.rs` remain the two main accumulation points. They are both correct today, but future features can make them expensive to change unless extractions stay disciplined.

---

## 3. Path Options From Here

### Path A: Alpha Validation + Precision Scaling (recommended)
- Run a structured alpha route and feed the results back into engineering.
- Add deterministic changed-file evidence to run/report surfaces.
- Improve context selection so medium/large repos receive more relevant, less noisy input.
- Extend tests toward realistic repo shapes rather than only contract correctness.

### Path B: Capability expansion first
- Move directly into patch mode, multi-provider work, or broader execution modes.
- This would add product surface faster, but it would do so before the current alpha loop is proven precise enough on real repos.

### Path C: Packaging and distribution first
- Prioritize release polish, packaging, and public-facing distribution steps.
- Worth doing, but secondary until alpha evidence says the current product loop is precise and predictable.

Recommendation:
- Execute Path A for Phase 19. Treat alpha as an engineering evidence loop, not a marketing milestone.

---

## 4. Work Inventory

## Must

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Add deterministic changed-file evidence to final/report surfaces | Faster operator verification and better trust | Medium | M | `runner.rs`, `loop.rs`, logs, tests |
| Improve context precision for medium/large repos | Better edit relevance and less noisy plans | Medium-High | M-L | `context.rs`, `planner.rs`, `editor.rs`, tests |
| Build an alpha fixture/reporting loop | Turns operator runs into reusable evidence | Low-Med | M | docs, tests, runbook |
| Keep new reporting surfaces under contract tests | Prevent CLI/log regressions | Medium | M | tests, `stats.rs`, `main.rs` |

## Should

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Extract carefully inside `stats.rs` as reporting grows | Lower maintenance cost | Medium | M | `stats.rs`, tests |
| Clean up package metadata and release-facing docs | Better alpha polish | Low | S | `Cargo.toml`, README, docs |

## Deferred (Post-Phase 19)

| Item | Reason deferred |
|---|---|
| Patch/diff edit contract | Still a wider behavioral surface than the current alpha evidence justifies. |
| Multi-provider expansion | Better after the core loop is validated for precision and run-to-run evidence. |
| Git worktree orchestration engine | Product expansion, not the immediate trust bottleneck. |
| Async runtime or broader execution model | Misaligned with current Linux-first, blocking design principles. |

---

## 5. Roadmap

## Phase 19 (next): Alpha Validation and Precision Scaling

Primary outcomes:
1. Operators can answer "what changed?" from Tod output and logs without manual repo archaeology.
2. Context assembly is more relevant on medium/large repos while remaining deterministic and testable.
3. Alpha runs follow a documented route with consistent reporting and triage discipline.
4. New reporting surfaces remain contract-tested at both module and command boundaries.

Definition of done:
- Quality gates clean.
- New run evidence is deterministic and compatible with existing artifacts.
- Alpha reporting route is documented and usable without tribal knowledge.

## Phase 20 (candidate): Edit Precision Expansion

Candidate outcomes:
1. Narrower edit contracts or patch-oriented execution if Phase 19 evidence supports it.
2. Better operator previews of intended changes before apply.

## Phase 21 (candidate): Backend Flexibility and Distribution

Candidate outcomes:
1. Provider optionality with consistent telemetry semantics.
2. Packaging, release, and distribution hardening once the alpha loop is stable.

---

## 6. Decision Summary

Tod should stay on a reliability-first path, but the next reliability question is different from Phase 18.

Recommended immediate direction:
- Use Phase 19 to validate Tod on real repos and improve precision where the alpha route exposes noise or overreach.
- Keep deferring major feature-surface expansion until the product can show deterministic evidence about what it touched and why.
