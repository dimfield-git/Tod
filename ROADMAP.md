# ROADMAP.md — Codebase Assessment & Strategic Planning

**Read AGENTS.md first.** All operating principles apply.

You are a senior Rust engineer doing a codebase assessment for Tod. This document requests analysis and recommendations, not code changes.

---

## Objective

Produce two documents:

1. **`docs/codebase-assessment.md`** — A detailed, structured assessment of the current codebase state (post-Phase 10).
2. **`docs/strategic-plan.md`** — A concrete plan for next steps, using the living list ([`tod-living-list.md`](tod-living-list.md)), roadmap docs, and implementation logs.

---

## Hard constraints

- Do NOT implement code in this run. This is analysis + planning only.
- Do NOT suggest speculative features outside the roadmap unless explicitly flagged as "optional future."
- Be precise: cite specific modules, files, functions, and line counts when making claims.
- Keep recommendations actionable: each recommendation must include "why," "scope," and "expected risk."
- If you suspect something, say how to verify it (grep, tests, inspection), don't assert.
- Base the assessment on what the code actually does, not what the documentation says it does. If they disagree, flag the discrepancy.
- You may run read-only commands: `grep`, `wc`, `cat`, `find`, `cargo test`, `cargo clippy`. Do not run any commands that modify the codebase.

---

## Document 1: Codebase Assessment (`docs/codebase-assessment.md`)

### A1. Architecture map

Modules and responsibilities. For each module:
- What is its single responsibility?
- Are there functions or types that have drifted into the wrong module?
- Where are the coupling points? Are they narrow (trait/type boundaries) or wide (shared mutable state, implicit conventions)?

### A2. Correctness & invariants

- Iteration caps, token caps, context byte budgets — are they enforced consistently?
- LLM retry — is it correctly contained inside the provider?
- Fingerprint and resume — does the checkpoint round-trip through JSON without data loss?
- Context ordering — is prompt content deterministic across machines?
- Concurrency — any shared mutable state or ordering assumptions that could break?

### A3. Safety model

- Path validation: can any code path bypass the sandbox jail?
- Edit application: is the transactional rollback complete? Are there edge cases?
- Command restrictions: is the `cargo`-only whitelist enforced?
- UTF-8 handling: are all string truncation points boundary-safe?

### A4. Observability & stats

- Can a user diagnose a failed run from logs alone?
- Are error messages actionable (do they say what went wrong and where)?
- Is token usage tracking accurate and complete?
- Are stats semantics correct (billed requests vs. transport retries)?

### A5. Code quality

- Naming conventions: consistent across modules?
- Duplication: any remaining shared logic that should be extracted?
- Error typing: are errors informative, typed, and consistently structured after Phase 10?
- Cohesion: are there functions doing too many things?
- Test coverage: adequate? Obvious gaps?
- Any `unwrap()` calls in non-test code?
- Dead code paths, unused imports, stale comments?

### A6. UX surface

- CLI: are all commands, flags, and help text consistent and correct?
- `--project`: does it work correctly for all commands that accept it?
- Docs: do README, AGENTS.md, and live-run-log.md match the current binary behavior?

### B. Phase implementation quality

Evaluate Phase 9 and Phase 10 implementation quality specifically:
- What landed well?
- What's brittle or could break under future changes?
- Any patterns introduced that should be reinforced or corrected going forward?

---

## Document 2: Strategic Plan (`docs/strategic-plan.md`)

### C. Remaining work inventory

Create a prioritized list grouped by:

| Priority | Meaning |
|----------|---------|
| **Must** | Blocks correctness, safety, or usability |
| **Should** | High value, low risk, do soon |
| **Nice** | Improves quality but not urgent |
| **Future** | Feature work, requires dedicated phase |

Each item must include: payoff, risk, rough effort, and touch points (files/modules).

Reference the current living list in [`tod-living-list.md`](tod-living-list.md) — review each of the 8 items and reassess priority. Add any new items discovered during the codebase assessment.

### D. Roadmap plan

Propose a 2–4 phase roadmap for the next milestones. For each phase:

- **Goals:** What does this phase achieve?
- **Tasks:** Numbered, ordered, with files touched.
- **Definition of done:** Test count expectation, clippy clean, specific behavioral criteria.
- **Validation commands:** `cargo test`, `cargo clippy -- -D warnings`, plus any targeted greps.
- **Codex reasoning level:** Low, medium, high, or xhigh per task.

Keep phases small and shippable. Each phase should be completable in a single Codex session.

### D2. Development path

- Of the three strategic paths (reliability, capability, distribution), which should Tod pursue next and why?
- Is Tod ready for external users beyond the developer, or does more hardening come first?
- What's the biggest risk to the project right now?

---

## Validation commands to reference

```
cargo test
cargo clippy -- -D warnings
grep / wc / cat / find as needed
```

---

## Start now.
