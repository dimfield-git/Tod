# Tod Codebase Assessment (Post-Phase 15)

Date: 2026-03-03  
Scope reviewed: `src/` modules, CLI/runtime behavior, and current docs surface.

Validation baseline on current tree:
- `cargo test`: **193 passed, 1 ignored, 0 failed**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Overall Assessment

Tod is now a reliable prototype-grade terminal Rust coding agent with strong determinism and safety discipline.

Key state after Phase 15:
- Module boundary cleanup landed: `log_schema.rs` (types), `loop_io.rs` (persistence + run identity), `loop.rs` (orchestration).
- Run identity allocation is centralized and consistent across normal and plan-error flows.
- Resume fingerprint compatibility logic is now isolated and table-tested.
- Legacy artifact compatibility remains intact.

Current quality profile:
- Correctness: strong for loop, resume, checkpointing, and artifact semantics.
- Safety: strong path-jailing + transactional edit apply model.
- Observability: strong terminal-path coverage, including plan-error `final.json`.
- Maintainability: materially improved, but `loop.rs` is still the largest concentration hotspot.
- Operator usefulness: viable for controlled terminal workflows; still missing some productization features for broader adoption.

---

## 2. How Tod Can Be Used Today

### Current practical usage modes

1. Assisted local Rust maintenance in a sandboxed project
- Use `tod run --project <path> "<goal>"` for targeted bugfix/refactor tasks.
- Best fit: medium-scoped tasks where `cargo` diagnostics drive retry loops.

2. CI-like strict quality enforcement during autonomous edits
- Use `--strict` to run `fmt --check`, `clippy -D warnings`, and tests each attempt.
- Best fit: teams requiring style/lint/test gates on every iteration.

3. Safe planning and dry-run review mode
- Use `--dry-run` to generate/validate/log edits without filesystem mutation.
- Best fit: prompt tuning, trust-building, and failure forensics.

4. Interrupted-run continuity
- Use `resume` from `.tod/state.json` with deterministic profile reuse.
- Best fit: long-running sessions, terminal interruptions, process restarts.

### Operational options available now

- Reliability-oriented operation:
  - strict mode + token cap + lower `--max-iters`
- Cost-oriented operation:
  - default mode + explicit `--max-tokens`
- Diagnostic operation:
  - dry-run + status/stats inspection of artifacts
- Recovery operation:
  - `resume` and optional `--force` for drift override

---

## 3. What Is Still Needed for Tod to Be Broadly Useful as a Rust Agent

Tod is already useful for controlled local workflows, but broad practical adoption needs several additions:

1. Safer workspace isolation and review workflow
- Missing: built-in git branch/worktree isolation and patch preview/approval loop.
- Why it matters: operators need reversible, auditable edit boundaries beyond transactional file rollback.

2. Better edit precision for larger codebases
- Missing: patch/diff-oriented edit mode (today edits are `write_file` / `replace_range`).
- Why it matters: large-file rewrites are costlier and riskier; diff-first workflows scale better.

3. Provider flexibility
- Missing: alternate provider support (OpenAI/local/offline adapters).
- Why it matters: operational cost, reliability, and deployment flexibility.

4. Stronger context and extraction resilience
- Missing: more robust JSON extraction for noisy/multi-block model output and richer large-repo context strategy.
- Why it matters: real-world prompts and larger repos increase response variance and context pressure.

5. Distribution and operator ergonomics
- Missing: packaged release path, setup docs for non-developer users, and clearer runbook-style troubleshooting docs.
- Why it matters: current usage assumes developer-level comfort with Rust + environment setup.

---

## 4. Architecture and Boundary Review

### Boundary quality (current)

| Area | State | Notes |
|---|---|---|
| CLI/config boundary | Good | `cli.rs` cleanly maps command surface to immutable `RunConfig`. |
| Planner/editor/runner/reviewer separation | Good | Responsibilities are explicit and mostly decoupled. |
| Safety validation boundary | Strong | `schema.rs` remains canonical pre-apply guardrail. |
| Persistence boundary | Improved | `loop_io.rs` now owns run-id allocation and write helpers. |
| Log-schema boundary | Improved | `log_schema.rs` is data+serde only. |
| Orchestration boundary | Medium | `loop.rs` still carries heavy logic and large test concentration. |

### Main concentration point

- `src/loop.rs` remains the largest module and central change-risk surface.
- Decomposition improved in Phase 15 but future refactors should continue reducing orchestration blast radius.

---

## 5. Correctness and Determinism

### Strong invariants now

- Checkpoint writes are best-effort with atomic tmp+rename.
- Plan/final/attempt logs are best-effort and stable in path contract.
- Planner-stage failures produce terminal `final.json` artifacts.
- Run identity allocation is single-source and collision-safe.
- Fingerprint compatibility logic is pure and explicitly tested by matrix.
- Resume profile reuse preserves originating execution semantics.

### Residual risks

- `--force` can override fingerprint mismatch by design; this is operationally necessary but should stay clearly documented.
- Context budget truncation can omit useful files/lines in large repos; behavior is safe but may reduce fix quality.

---

## 6. Safety Model

### Strengths

- Path validation rejects absolute paths, traversal, and symlink-escape patterns.
- Edits apply transactionally with rollback attempts.
- Runner executes static cargo stages (no model-driven shell command execution).
- UTF-8 truncation/preview behavior is defensive and tested.

### Residual risk

- TOCTOU remains theoretically possible between validation and apply under concurrent filesystem mutation.
- Rollback failures are typed and surfaced but can still leave partial state in worst-case filesystem errors.

---

## 7. Observability and Stats

### Strong areas

- `final.json` is source of truth when present.
- Plan-error-only artifact runs are summarizable.
- Legacy compatibility defaults are preserved (attempt stage defaults, fingerprint/profile defaults).
- Request counting semantics (plan=1 + edits=attempt count) are stable in stats.

### Gaps still worth improving

- Richer operator telemetry is limited (no per-stage latency distribution, retry timing, or model response quality signals).
- Single-run and multi-run summaries are functional but still text-oriented; no structured export format yet.

---

## 8. Module-by-Module Findings

- `main.rs`: stable dispatcher; init helper still embedded but acceptable.
- `cli.rs`: clear, deterministic parsing; good tests.
- `config.rs`: clean immutable runtime settings.
- `context.rs`: robust byte-budget handling; non-UTF8 files still a practical limitation.
- `planner.rs`: strict semantic validation is a strength.
- `editor.rs`: strong typed error bridge from model output to schema validation.
- `schema.rs`: strongest safety backbone in the codebase.
- `runner.rs`: transactional apply + deterministic pipeline staging are solid.
- `reviewer.rs`: simple and predictable decision policy.
- `llm.rs`: retry containment is correct; provider surface intentionally minimal.
- `log_schema.rs`: now correctly scoped to pure data and serde defaults.
- `loop_io.rs`: clean persistence boundary and run-id single source.
- `loop.rs`: robust orchestration behavior, still the major complexity center.
- `stats.rs`: compatibility-aware summarization is strong; room for richer analytics.
- `util.rs`, `test_util.rs`: small and appropriate.

---

## 9. Priority Findings (Ordered)

1. Productization gap (distribution + workflow isolation)
- Impact: high on real-world adoption.
- Scope: docs/runbooks + git isolation strategy + release ergonomics.

2. Edit precision/scalability gap for larger repos
- Impact: medium-high on correctness/cost in real projects.
- Scope: patch/diff workflow and richer context strategy.

3. Orchestration complexity concentration in `loop.rs`
- Impact: medium-high on long-term maintenance risk.
- Scope: continued phase-scoped extractions and targeted regression tests.

4. Provider monoculture
- Impact: medium on reliability/cost flexibility.
- Scope: pluggable provider adapters behind existing trait boundary.

---

## 10. Recommended Direction

Tod should now shift from structural hardening to practical operator value while preserving safety invariants.

Recommended next-phase posture:
- Reliability + usability first (not broad feature explosion).
- Prioritize workflows that make Tod safely usable on real Rust repos by more than the primary developer.
- Keep each phase behavior-preserving unless explicitly targeted for new capability.
