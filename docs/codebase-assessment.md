# Tod Codebase Assessment (Post-Phase 13)

Date: 2026-03-03  
Scope reviewed: `src/` (all modules), `README.md`, `AGENTS.md`, `docs/` strategy and phase artifacts.

Validation baseline on current tree:
- `cargo test`: **178 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Overall Assessment

Tod is currently a robust prototype-stage terminal agent with strong safety and good runtime determinism for resumed runs. Phase 13 closed three major correctness gaps (checkpoint fingerprint freshness, resume-profile reuse, and same-size drift blind spots) and hardened run-id uniqueness.

Current quality profile:
- **Correctness:** strong for run-loop and resume semantics.
- **Safety:** strong path-validation and transactional edit model.
- **Observability:** strong after planning; partial blind spot before `RunState` exists.
- **Maintainability:** acceptable, with one major hotspot (`src/loop.rs`).
- **Test confidence:** high (broad and deep unit coverage).

---

## 2. Architecture and Module Boundaries

### Boundary quality (current)

| Area | State | Notes |
|---|---|---|
| CLI/config boundary | Good | `cli.rs` cleanly maps command surface to `RunConfig`. |
| Planner/editor/runner/reviewer separation | Good | Responsibilities are clear and mostly decoupled. |
| Safety validation boundary | Good | `schema.rs` is the authoritative pre-apply gate. |
| Orchestration boundary | Medium | `loop.rs` is functionally coherent but over-concentrated. |
| Stats/log schema boundary | Medium-Low | `stats.rs` imports loop log/state types directly. |

### Key concentration points
- `src/loop.rs` currently carries orchestration, state schema, log schema, checkpointing, fingerprinting, resume policy, and a very large test suite.
- `src/stats.rs` depends on loop-owned structs (`AttemptLog`, `PlanLog`, `FinalLog`, `RunState`), creating cross-module coupling friction for schema changes.

---

## 3. Correctness and Determinism

### What is now strong
- Checkpoint fingerprint is refreshed at runtime checkpoint call-sites in the main step loop.
- Resume execution profile is persisted and reused (`mode`, `dry_run`, `max_runner_output_bytes`) with legacy compatibility fallback.
- Drift fingerprint is versioned; v2 content-aware hash catches same-size edits.
- Legacy v1 checkpoint compatibility is explicitly handled in resume logic.
- Run-id collisions are mitigated with fractional time + suffix fallback while preserving lexical sort semantics.

### Remaining correctness/behavior risks
- Planner-stage failure occurs before `RunState` initialization, so there is no run artifact for those failures.
- LLM request accounting semantics still depend on available usage fields in current logging/stat logic paths.

---

## 4. Safety Model Review

### Strengths
- Path validation blocks absolute paths, traversal, lexical escape, and existing-ancestor symlink escape.
- Edit apply is transactional with snapshot + rollback.
- Runner executes fixed cargo stage commands, not model-provided shell commands.
- UTF-8 truncation and preview helpers are consistently defensive.

### Residual risk
- TOCTOU remains theoretically possible if filesystem structure changes between validation and apply (expected for this threat model; acceptable in prototype, worth documenting).
- Rollback failure can still leave partial filesystem state; this is surfaced clearly as `ApplyError::Rollback`.

---

## 5. Observability and Logging

### Strong areas
- Per-attempt structured logs and terminal `final.json` support good forensic analysis.
- Stats correctly prefers `final.json` as run-outcome source of truth for modern runs.
- Legacy compatibility is preserved via serde defaults and fallback heuristics.

### Gap to address next
- No structured artifact path for failures before planning succeeds and before `RunState` exists.

---

## 6. Testing and Verification Depth

### Coverage profile
- Total tests passing: 178 (+1 ignored integration smoke test).
- Highest test density in critical runtime modules:
  - `loop.rs` (42)
  - `schema.rs` (28)
  - `runner.rs` (19)
  - `stats.rs` (19)
  - `llm.rs` (15)

### Confidence assessment
- High confidence in regression prevention for core runtime behavior.
- Good compatibility testing for legacy logs and checkpoints.

### Worth adding in upcoming phase
- Planner-stage failure artifact behavior tests (once artifact strategy is implemented).
- Additional accounting tests for explicit request-count semantics when usage fields are absent.

---

## 7. Documentation and Operational Parity

### Current state
- Core docs now largely aligned with implemented Phase 13 behavior.
- `AGENTS.md` and `README.md` were updated to reflect v2 fingerprinting, resume profile semantics, and run-id hardening.

### Ongoing risk
- Architecture and strategy docs can drift quickly if not updated as part of every phase definition of done.

Recommendation:
- Keep docs update as an explicit acceptance item in every future phase file.

---

## 8. Priority Findings (Ordered)

1. **Maintainability hotspot in `loop.rs`**  
Impact: medium-high over time (review cost, regression surface).  
Recommendation: phase-scoped extraction by concern, starting with log schema/persistence helpers.

2. **Stats/log schema coupling to loop internals**  
Impact: medium (schema evolution friction).  
Recommendation: introduce neutral log-schema module consumed by both loop and stats.

3. **Pre-RunState observability gap (planner-stage failures)**  
Impact: medium (forensic incompleteness).  
Recommendation: add a minimal terminal artifact path for planner failures.

4. **Metrics semantics clarity (request counts vs usage presence)**  
Impact: medium (operator trust in reporting).  
Recommendation: make counting semantics explicit and deterministic independent of usage payload presence.

---

## 9. Recommended Next-Step Direction

Short-term direction should prioritize **operational integrity and maintainability**, not new user-facing features.

Recommended next phase scope:
- decouple log schema from loop internals,
- close planner-stage observability gap,
- improve aggregate outcome fidelity in stats,
- keep compatibility with legacy logs/checkpoints.

This preserves current correctness momentum and reduces risk in future feature work.

