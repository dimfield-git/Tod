# Tod Strategic Plan (Post-Phase 15)

Date: 2026-03-03  
Baseline validated on current tree:
- `cargo test`: **193 passed, 1 ignored**
- `cargo clippy -- -D warnings`: **clean**

---

## 1. Strategic Objective

Evolve Tod from a strong prototype into a practical, trustworthy Rust coding agent for day-to-day terminal workflows.

Near-term objective:
- keep reliability invariants intact,
- improve real-world usability and operator trust,
- add capability only where it directly increases successful Rust task completion.

---

## 2. Agent Usage Model and Options

## How the agent will be used

1. Solo developer workflow
- Run Tod on a local Rust project for scoped tasks (bugs, refactors, focused feature increments).
- Use strict mode when code quality policy matters, dry-run when reviewing intent first.

2. Team-internal assistant workflow
- Use Tod on prepared branches/worktrees for repetitive maintenance tasks.
- Inspect `.tod/logs` artifacts for run auditability and postmortem analysis.

3. Controlled CI-adjacent automation (future-ready)
- Use deterministic caps and strict pipelines for controlled auto-fix experiments.
- Resume support helps handle interrupted sessions without losing context.

## Options we have next

Path A: Reliability-first productization (recommended next)
- Improve isolation, runbook docs, and operator ergonomics before larger capability jumps.

Path B: Capability-first expansion
- Add patch mode and advanced planning/review logic first; higher upside, higher regression risk.

Path C: Distribution-first
- Focus on packaging and adoption ergonomics now; risks exposing rough edges before workflow hardening.

Recommendation:
- Execute Path A now, then blend in selected Path B items once operational safety/usability is stronger.

---

## 3. Remaining Work Inventory

## Must

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Git-safe workflow mode (branch/worktree guardrails) | Makes real repo usage safer and reversible | Med-High | M-L | `loop.rs`, new git integration module, docs |
| Operator runbook and failure-recovery docs | Converts prototype into usable tool for non-authors | Low | S-M | `README.md`, `docs/`, AGENTS phase docs |
| Continue `loop.rs` surface reduction with behavior parity | Lowers regression risk as features grow | Medium | M | `loop.rs`, `loop_io.rs`, tests |
| Context robustness for larger Rust repos | Improves success rate on realistic codebases | Medium | M | `context.rs`, `planner.rs`, `editor.rs` |

## Should

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Patch/diff edit contract (non-destructive preference) | Better edit precision and reviewability | Medium | M-L | `schema.rs`, `editor.rs`, `runner.rs` |
| Enhanced stats output (structured export + richer counters) | Better operational visibility | Low-Med | S-M | `stats.rs`, docs |
| Provider abstraction expansion (second backend) | Cost/reliability flexibility | Medium | M | `llm.rs`, config/CLI/docs |

## Nice

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Optional planner self-check/reflection pass | Better plan quality on ambiguous goals | Medium | M | `planner.rs`, `loop.rs` |
| Configurable runner stage presets | Better adaptation to project conventions | Medium | M | `config.rs`, `runner.rs`, CLI |

## Future

| Item | Payoff | Risk | Effort | Touch points |
|---|---|---|---|---|
| Remote/daemon mode | Enables service workflows | High | L | architecture-wide |
| Multi-language expansion beyond Rust | Larger market scope | High | L | prompts/schema/runner/context |

---

## 4. What Is Left for Tod to Be a Functional, High-Utility Rust Agent

Tod already functions. What remains is scaling practical utility safely:

1. Safe application boundary in real repos
- Add first-class git isolation strategy and explicit approval/review loop options.

2. Higher-confidence edit precision
- Introduce patch/diff-first mode to reduce broad file rewrites.

3. Better large-project context handling
- Improve context selection and summarization for bigger trees and noisy diagnostics.

4. Better operator experience
- Clear runbooks, troubleshooting, and decision guidance (strict vs default vs dry-run, when to resume with `--force`, token cap tuning).

5. Flexibility in model backend
- Add at least one additional provider to reduce single-vendor dependency.

---

## 5. Proposed Phase Roadmap (Next 3 Phases)

## Phase 16: Operator-Grade Usability and Workflow Safety

Goals:
- make Tod safer/easier to use in real Rust repo workflows without widening core feature surface too much.

Tasks:
1. Add explicit operator workflow documentation and decision matrix (strict/default/dry-run/resume/force).
2. Add a minimal git-aware safety mode plan and scaffolding (non-destructive by default).
3. Continue small extraction from `loop.rs` where behavior parity is straightforward.
4. Add regression tests for any new workflow-level contracts.

Definition of done:
- `cargo test` and `cargo clippy -- -D warnings` clean.
- No behavior regressions to current artifact compatibility.
- Docs and CLI behavior alignment verified.

Reasoning level by task:
- 1: Medium
- 2: High
- 3: Medium
- 4: Medium

## Phase 17: Edit Precision and Large-Repo Effectiveness

Goals:
- improve successful code modification quality on larger, realistic projects.

Tasks:
1. Introduce patch/diff edit schema path behind explicit contract.
2. Improve context assembly heuristics for large trees and long diagnostics.
3. Strengthen extraction/parsing resilience for noisy model outputs.

Definition of done:
- quality gates pass,
- no regression in legacy compatibility,
- measured decrease in broad write-file operations on test fixtures.

Reasoning level by task:
- 1: XHigh
- 2: High
- 3: High

## Phase 18: Provider Flexibility and Operational Telemetry

Goals:
- reduce vendor dependency and improve operational visibility.

Tasks:
1. Add second provider implementation behind `LlmProvider`.
2. Expand telemetry summaries (request timing/retry surfaces/structured output mode).
3. Add operator-facing docs for backend selection and tradeoffs.

Definition of done:
- quality gates pass,
- provider swap path documented and tested,
- stats remain backward compatible.

Reasoning level by task:
- 1: High
- 2: Medium
- 3: Low-Medium

---

## 6. Development Path Decision

Reliability vs capability vs distribution:
- Next: reliability/usability hybrid (operator-grade workflows), then capability.
- Reason: Tod now has strong core correctness; highest ROI is reducing adoption friction and workflow risk.

Is Tod ready for broader external users?
- Limited yes: for technical users comfortable with Rust + terminal + environment setup.
- Not fully: still needs stronger workflow isolation and product-level operator guidance.

Biggest current project risk:
- Practical adoption risk, not core correctness risk.
- If workflow safety and operator ergonomics lag, strong internals will still be underused.
