# Tod Codebase Assessment (Post-Phase 17)

Date: 2026-03-03  
Scope reviewed: full `src/` surface, runtime behavior, tests, and operator docs.

Validation baseline on current tree:
- `cargo test`: **215 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Executive Assessment

Tod remains in a strong engineering state for a deterministic, terminal-first Rust coding agent.

Post-Phase-17 status:
- Observability contracts are materially stronger (JSON and human-output contract tests are broader).
- Operator experience improved (startup/step/attempt/review/resume lifecycle messaging).
- Error surfaces are more actionable (guidance appended directly in `LoopError::Display`).
- Completion output is richer and operator-useful (`tokens`, `requests`, `logs` path).
- Compatibility and safety invariants remain intact.

Bottom line:
- **Correctness and safety are strong.**
- **Operator trust/clarity is significantly improved.**
- **Primary residual risks are concentrated in orchestration complexity and observability/accounting precision edges.**

---

## 2. Engineering Quality Snapshot

## Determinism and safety

Strong:
- Strict path/schema validation before apply (`schema.rs`).
- Transactional apply with rollback-on-failure semantics (`runner.rs`).
- Versioned fingerprinting + legacy compatibility guardrails (`loop.rs`).
- Best-effort persistence boundary with atomic checkpoint write pattern (`loop_io.rs`).
- No async runtime, no global mutable runtime state.

Residual risk:
- `--force` resume remains intentionally dangerous under workspace drift (correctly documented, still operator-risky by design).
- TOCTOU remains theoretically possible under external concurrent mutation.

## Test posture

Strong:
- High unit coverage in highest-risk modules (`loop`, `schema`, `runner`, `stats`).
- Phase 17 added contract tests for output shape and compatibility behaviors.

Residual gap:
- No dedicated CLI-level integration harness for stdout/stderr contract behavior across all command variants.
- No benchmark fixture set for repeated large-repo runs and observability trend validation.

---

## 3. Product Utility Assessment (Today)

Tod is production-useful for:
1. Small/medium Rust maintenance loops where deterministic safety is required.
2. Strict-gated maintenance (`fmt`/`clippy`/`test`) with bounded retry/token caps.
3. Dry-run and postmortem workflows via persistent `.tod` artifacts.
4. Interrupted-run continuation with profile/fingerprint compatibility protections.

Tod still has constraints:
1. `loop.rs` remains a large, high-blast-radius module.
2. Large-repo effectiveness is bounded by context construction/edit granularity.
3. Single-provider execution path persists (strategic deferral, not an immediate correctness issue).

---

## 4. Architecture Health

## Boundary quality

Healthy boundaries:
- `log_schema.rs`: pure data + serde defaults.
- `loop_io.rs`: persistence + run identity allocation.
- `stats.rs`: read-only summarization/formatting with compatibility-first behavior.

Watch boundary:
- `loop.rs`: still carries substantial orchestration, terminal pathing, and runtime accounting responsibilities.

## Hotspots

1. `src/loop.rs` (`2854 LOC`, `61 tests`)  
   Strength: highly tested.  
   Risk: broad change surface and dense control flow.

2. `src/stats.rs` (`1530 LOC`, `32 tests`)  
   Strength: strong compatibility logic and output formatting coverage.  
   Risk: continued output-contract growth can drift without strict contract discipline.

---

## 5. Priority Findings (Ordered)

1. **Orchestration concentration in `loop.rs` (high)**  
   Continue extracting pure decision helpers and keeping side effects locally explicit.

2. **Request/usage observability precision edge (medium-high)**  
   Current request-count implementation should be hardened for all pre/post-LLM failure boundaries to fully enforce the documented request semantics under every terminal path.

3. **CLI stdout/stderr contract hardening depth (medium)**  
   Lifecycle messaging and JSON/human output are improved, but command-level contract integration coverage should be expanded.

4. **Large-repo context relevance and edit precision (medium)**  
   Improve signal-to-noise/context targeting before broad edit-contract expansion.

5. **Provider monoculture (medium, strategic)**  
   Defer until orchestration and observability surfaces are further stabilized.

---

## 6. Recommended Phase 18 Direction

Phase 18 should prioritize **observability integrity and operator control**, not broad feature expansion:
1. Harden request-count/usage accounting invariants under all error paths.
2. Provide precise per-run failure log pointers without weakening typed errors.
3. Add explicit output-control ergonomics (quiet/progress policy) while preserving stdout contracts.
4. Expand contract tests for command-level stdout/stderr behavior and machine-readable stability.
5. Keep compatibility-first defaults and safety boundaries unchanged.
