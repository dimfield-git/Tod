# Tod Strategic Plan (Post-Assessment)

Date: 2026-02-28  
Input set used: `tod-living-list.md` (8 items), phase roadmap docs (`docs/PHASE8.md`, `docs/PHASE9.md`, `docs/PHASE10.md`), implementation logs, and current code behavior.

Current validated baseline:
- `cargo test`: 154 passed, 1 ignored
- `cargo clippy -- -D warnings`: clean

---

## C. Remaining Work Inventory

### Priority: Must

| Item | Source | Payoff | Risk | Rough effort | Touch points |
|---|---|---|---|---|---|
| Stronger fingerprint (content-aware) | Living list #8 | Prevents false-safe resume after same-size edits | Medium (state format and compatibility handling) | M | `src/loop.rs` (fingerprint + resume), tests in `loop.rs` |
| Pre-resume token-cap gate | New from assessment | Prevents extra LLM spend when checkpoint already over budget | Low | S | `src/loop.rs`, `loop.rs` tests |
| Log/checkpoint on edit/apply early exits | New from assessment | Makes failures diagnosable and resumable from `.tod` artifacts | Medium | M | `src/loop.rs`, maybe `src/stats.rs`, tests |
| Correct request accounting in stats (`llm_requests`) | New from assessment | Accurate operator metrics; aligns with "billed requests" intent | Medium | M | `src/loop.rs` logging schema, `src/stats.rs`, tests |

### Priority: Should

| Item | Source | Payoff | Risk | Rough effort | Touch points |
|---|---|---|---|---|---|
| `loop.rs` decomposition by concern | Living list #1 | Lowers regression risk and review burden on highest-churn file | Medium | M | `src/loop.rs` plus extracted modules |
| Stricter JSON extraction for multi-block markdown | Living list #2 | Reduces malformed-response failures in planner/editor | Medium | S-M | `src/schema.rs`, `planner.rs`/`editor.rs` tests |
| Documentation parity pass (runtime vs docs) | New from assessment | Prevents user/operator confusion | Low | S | `docs/live-run-log.md`, `docs/index.html`, `docs/tod-architecture.html`, `AGENTS.md` |
| Replace non-test `expect` in `main` | New from assessment | Aligns code with stated invariant and consistent error paths | Low | S | `src/main.rs`, tests |

### Priority: Nice

| Item | Source | Payoff | Risk | Rough effort | Touch points |
|---|---|---|---|---|---|
| Rationalize scattered magic-number constants | Living list #3 | Improves maintainability without API bloat | Low | S | `context.rs`, `runner.rs`, `schema.rs`, `loop.rs` |
| Remove `editor.rs` formatter placeholder hack | New from assessment | Cleans dead/placeholder code and clarifies ownership | Low | S | `src/editor.rs`, potentially `src/context.rs` |

### Priority: Future

| Item | Source | Payoff | Risk | Rough effort | Touch points |
|---|---|---|---|---|---|
| Patch/diff edit mode | Living list #4 | Lower token cost + better precision on large files | High | L | `schema.rs`, `editor.rs` prompts, `runner.rs`, logs/tests |
| Git branch isolation | Living list #5 | Safer real-project operation and rollback UX | High | L | new git module + `runner`/`loop` integration |
| Local model provider support | Living list #6 | Cost/offline flexibility | High | L | `llm.rs` provider layer, config/CLI/docs |
| `--reflect` planning pass | Living list #7 | Better plan quality with modest added LLM cost | Medium | M | `planner.rs`, `loop.rs`, config/docs/tests |

---

## D. 2-4 Phase Roadmap

## Phase 11: Reliability Accounting

### Goals

Close correctness and accounting gaps that can produce misleading run state or metrics.

### Tasks

1. Enforce token cap before resume continues (`src/loop.rs`).  
Codex reasoning level: **medium**
2. Extend run logging to preserve planner request/usage accounting in a way stats can recover (`src/loop.rs`, `src/stats.rs`).  
Codex reasoning level: **high**
3. Fix stats request semantics so request count reflects intended meaning (and document semantics explicitly) (`src/stats.rs`).  
Codex reasoning level: **high**
4. Add/adjust tests for over-cap resume and request accounting (`src/loop.rs`, `src/stats.rs`).  
Codex reasoning level: **medium**

### Definition of done

- Expected test delta: **+3 to +6 tests** (total passing expected >=157, ignored still 1).
- `cargo clippy -- -D warnings` clean.
- Resume returns deterministic `TokenCapExceeded` before any new LLM call when checkpoint usage is already over cap.
- Stats request count semantics are explicitly true to implementation and stable across runs.

### Validation commands

- `cargo test`
- `cargo clippy -- -D warnings`
- `grep -n "TokenCapExceeded|resume\(" src/loop.rs`
- `grep -n "llm_requests|usage" src/stats.rs src/loop.rs`

---

## Phase 12: Failure Observability

### Goals

Guarantee actionable diagnostics for all orchestration failure exits.

### Tasks

1. Add structured attempt logging + checkpointing for `EditError` and `ApplyError` exits (`src/loop.rs`).  
Codex reasoning level: **high**
2. Ensure log schema remains backward-compatible (`#[serde(default)]` strategy) and stats tolerates old/new logs (`src/loop.rs`, `src/stats.rs`).  
Codex reasoning level: **high**
3. Add tests asserting logs/checkpoints exist for edit/apply failure cases (`src/loop.rs`, `src/stats.rs`).  
Codex reasoning level: **medium**

### Definition of done

- Expected test delta: **+3 to +5 tests** (passing expected >=160, ignored still 1).
- `cargo clippy -- -D warnings` clean.
- For every loop exit path after planning, `.tod/state.json` and relevant run logs remain sufficient to diagnose failure stage and reason.

### Validation commands

- `cargo test`
- `cargo clippy -- -D warnings`
- `grep -n "LoopError::Edit|LoopError::Apply|write_attempt_log|checkpoint" src/loop.rs`
- `grep -n "InvalidLog|summarize_run" src/stats.rs`

---

## Phase 13: Resume Drift Hardening

### Goals

Upgrade workspace drift detection from size-only heuristic to content-aware correctness.

### Tasks

1. Replace `(path,size)` fingerprint hashing with content-inclusive hashing strategy (`src/loop.rs`).  
Codex reasoning level: **xhigh**
2. Add compatibility handling so existing checkpoints fail safely (clear mismatch guidance) instead of panicking (`src/loop.rs`).  
Codex reasoning level: **high**
3. Add tests for same-size content changes, forced resume behavior, and deterministic ordering (`src/loop.rs` tests).  
Codex reasoning level: **high**

### Definition of done

- Expected test delta: **+3 to +5 tests** (passing expected >=163, ignored still 1).
- `cargo clippy -- -D warnings` clean.
- Same-size file content changes must trigger fingerprint mismatch under normal resume.

### Validation commands

- `cargo test`
- `cargo clippy -- -D warnings`
- `grep -n "compute_fingerprint|FingerprintMismatch|resume\(" src/loop.rs`

---

## Phase 14: Maintainability + UX Parity

### Goals

Reduce maintenance cost of core orchestrator and eliminate doc/runtime drift for external users.

### Tasks

1. Extract one cohesive concern from `loop.rs` (recommended first: logging/checkpoint helpers) without behavior change (`src/loop.rs`, new module file).  
Codex reasoning level: **high**
2. Harden `extract_json` for multi-code-block responses while preserving current fast paths (`src/schema.rs` + tests).  
Codex reasoning level: **medium**
3. Update docs to reflect current binary behavior and signatures (`docs/live-run-log.md`, `docs/index.html`, `docs/tod-architecture.html`, `AGENTS.md`, optional `README.md` cleanup).  
Codex reasoning level: **low**

### Definition of done

- Expected test delta: **+2 to +4 tests** (passing expected >=165, ignored still 1).
- `cargo clippy -- -D warnings` clean.
- No known stale command/signature claims in first-party docs.

### Validation commands

- `cargo test`
- `cargo clippy -- -D warnings`
- `grep -Rn "Result<String, LlmError>|agent status|agent stats|status.*do not accept --project" docs AGENTS.md README.md`
- `wc -l src/loop.rs` (expect measurable reduction if extraction lands)

---

## D2. Development Path Decision

### Recommended strategic path next: **Reliability**

Why:
- Current highest-risk issues are correctness/diagnostic integrity, not missing headline features.
- The must-fix set directly protects operator trust (resume correctness, token accounting, complete failure logs).

### Is Tod ready for broader external users now?

- **Partially**. Core functionality is strong and tests are healthy.
- **Not fully** for external reliability expectations until must items above are closed (especially fingerprint strength + full failure observability + request accounting clarity).

### Biggest project risk right now

- **State/log trust gap under failure and resume**: size-only fingerprint plus incomplete early-exit logging can make a resumed run appear coherent while key failure context is missing or drift is undetected.
