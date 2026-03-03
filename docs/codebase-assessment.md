# Tod Codebase Assessment (Post-Phase 16)

Date: 2026-03-03
Scope reviewed: full `src/` surface, runtime behavior, tests, and operator docs.

Validation baseline on current tree:
- `cargo test`: **203 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Executive Assessment

Tod is in a strong engineering state for a terminal-only Rust coding agent focused on deterministic execution and compatibility safety.

Post-Phase-16 status:
- Operator guidance exists and is actionable (`docs/runbook.md`).
- `run()` now emits a non-blocking dirty-workspace warning (safety-forward default without preventing execution).
- Iteration/token cap decisions are now explicit pure helpers in `loop.rs` (maintainability gain, behavior preserved).
- `status`/`stats` now support machine-readable `--json` output while preserving existing human format.

Bottom line:
- **Correctness and safety are strong.**
- **Operator usability materially improved in Phase 16.**
- **Primary residual risk is orchestration concentration in `src/loop.rs` and scaling precision on larger repos.**

---

## 2. Engineering Quality Snapshot

### Determinism and safety

Strong:
- Strict path validation and schema checks before apply.
- Transactional apply model with rollback-on-failure semantics.
- Stable artifact contract under `.tod/`.
- Resume fingerprinting with legacy compatibility behavior explicitly preserved.
- No async/global mutable runtime complexity.

Residual risk:
- `--force` resume is intentionally dangerous when workspace drift exists; this is documented and should remain explicit in UX.
- TOCTOU is still theoretically possible under concurrent external file mutation.

### Test posture

Strong:
- High module-level unit coverage in high-risk components (`loop`, `schema`, `runner`, `stats`).
- Phase-16 additions introduced targeted regression tests (dirty workspace checks, cap helper logic, JSON formatters, CLI parsing).

Residual gap:
- No golden-contract integration fixture set for end-to-end CLI output contracts across human + JSON modes.

---

## 3. Product Utility Assessment (Today)

Tod is practically usable now for:
1. Small/medium Rust maintenance tasks in terminal workflows.
2. Strict-gated maintenance loops (`fmt`/`clippy`/`test`) when quality gates matter.
3. Dry-run planning and failure triage using persisted artifacts.
4. Interrupted run continuation with deterministic profile reuse.

Tod is not yet fully productized for broader adoption due to:
1. Central orchestration complexity still concentrated in a single large module.
2. Edit precision limitations for very large files/repos (contract is still `write_file` / `replace_range`).
3. Single-provider dependency and limited backend flexibility.

---

## 4. Architecture Health

### Boundary quality

- `log_schema.rs`: clean data/serde-only boundary (healthy).
- `loop_io.rs`: persistence + run identity ownership is clear (healthy).
- `loop.rs`: orchestration ownership is correct but heavy (watch area).
- `stats.rs`: compatibility-aware summarization with dual output modes (healthy, growing).

### Hotspot

`src/loop.rs` remains the largest change-risk surface.

Reason:
- It still owns substantial orchestration flow, checkpoint timing, and terminal-path behavior.
- Even with extractions from Phases 15–16, change blast radius remains higher than other modules.

---

## 5. Priority Findings (Ordered)

1. **Maintainability hotspot in `loop.rs`**
- Impact: high (future regressions and change cost)
- Recommended direction: continue behavior-preserving pure helper extraction with targeted table tests.

2. **Observability contract depth is improving but still shallow for automation consumers**
- Impact: medium-high
- Recommended direction: add explicit contract tests for JSON output and richer machine-usable counters while preserving compatibility defaults.

3. **Large-repo effectiveness remains constrained by context/edit contract limits**
- Impact: medium-high
- Recommended direction: postpone major feature expansion, but prepare extraction and measurement groundwork before patch-mode changes.

4. **Provider monoculture**
- Impact: medium
- Recommended direction: defer until orchestration surface is further reduced and telemetry clarity improves.

---

## 6. AI-Research Perspective

From an applied-agent research standpoint, Tod currently demonstrates a strong constrained-agent architecture:
- LLM intent generation is separated from deterministic execution.
- Safety is enforced by hard validation and typed errors.
- Artifacts provide a basis for reproducibility and postmortem analysis.

High-value next research/engineering crossover:
1. Better measurement surfaces (request-level + stage-level observability).
2. Repeatable benchmark fixtures for "goal -> plan -> edits -> outcome" quality drift tracking.
3. Structured failure taxonomy that can be consumed by automation and future adaptive policies.

---

## 7. Recommended Next-Phase Direction

Phase 17 should prioritize **operational depth over feature breadth**:
1. Expand observability fidelity and contract testing.
2. Continue safe orchestration decomposition.
3. Preserve all compatibility and safety invariants.
4. Keep explicit room for pending UX requirements to be integrated without re-scoping core safety work.

This keeps momentum practical and defensible while avoiding premature feature-surface expansion.
