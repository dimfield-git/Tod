# ROADMAP.md — Codebase Assessment & Strategic Planning

**Read AGENTS.md first.** All operating principles apply. This document requests analysis and recommendations, not code changes.

---

## Objective

Produce two documents:

1. **`docs/codebase-assessment.md`** — A professional review of Tod's current state.
2. **`docs/strategic-plan.md`** — Recommendations for what to build next and how.

---

## Document 1: Codebase Assessment (`docs/codebase-assessment.md`)

Write a thorough assessment of the codebase as it exists right now. Be specific — cite files, functions, line counts, and patterns. Flag anything that contradicts the architectural invariants in AGENTS.md.

### Architecture & module boundaries

- Does each module have a single, clear responsibility?
- Are there functions or types that have drifted into the wrong module?
- Where are the coupling points between modules? Are they narrow (trait/type boundaries) or wide (shared mutable state, implicit conventions)?

### Code quality & consistency

- Are naming conventions consistent across modules?
- Are there dead code paths, unused imports, or stale comments?
- How is error handling — are errors informative, typed, and consistently structured?
- Are there any `unwrap()` calls in non-test code?
- Is the test coverage adequate? Are there obvious gaps?

### Safety & correctness

- Path validation: can any code path bypass the sandbox jail?
- Edit application: is the transactional rollback complete? Are there edge cases?
- State serialization: can `RunState` round-trip through JSON without data loss?
- UTF-8 handling: are all string truncation points boundary-safe?
- Concurrency: any shared mutable state or ordering assumptions that could break?

### Observability & debuggability

- Can a user diagnose a failed run from logs alone?
- Are error messages actionable (do they say what went wrong and where)?
- Is token usage tracking accurate and complete?

### Technical debt inventory

- List every known shortcut, TODO, or deferred decision in the codebase.
- For each item, assess: is it blocking, annoying, or harmless?
- Highlight any debt that would compound if features are added on top of it.

---

## Document 2: Strategic Plan (`docs/strategic-plan.md`)

Based on the codebase assessment and the living list in [`tod-living-list.md`](tod-living-list.md), write a strategic plan for Tod's next development phase.

### Living list review

For each of the 8 items on the current living list:

**Tier 1 items (opportunistic polish, items 1–3):**
- Is the item still relevant?
- Should any be promoted to "do before next feature work"?

**Tier 2 items (feature candidates, items 4–8):**

| # | Feature | Summary |
|---|---------|---------|
| 4 | Patch/diff mode | Diff-based edits instead of full file rewrites |
| 5 | Git branch isolation | Throwaway branch per run, merge on success |
| 6 | Local model support | Ollama/llama.cpp behind LlmProvider trait |
| 7 | `--reflect` flag | Self-critique pass on planner output before execution |
| 8 | Stronger fingerprint | Content hashing for resume correctness |

For each candidate, assess:
- **Feasibility:** How much code change? Which modules affected? Risk of destabilizing existing functionality?
- **Value:** What does this unlock for the user? For the developer? For learning?
- **Dependencies:** Does it require other changes first? Does it conflict with other candidates?
- **Recommended scope:** If this were a phase, how many tasks? What reasoning level for Codex?

### Phase 11 recommendation

- Which feature(s) should go into Phase 11?
- What's the recommended task breakdown?
- What's the expected test count increase?
- Are there preparatory changes needed before the main feature work?

### Development path

- Of the three strategic paths (reliability, capability, distribution), which should Tod pursue next and why?
- Is Tod ready for external users beyond the developer, or does more hardening come first?
- What's the biggest risk to the project right now?

### Living list updates

- Based on the assessment, are there new issues or opportunities to add?
- Are there items to remove as no longer relevant?

### Tone

Be direct — recommend, don't hedge. Where there are genuine tradeoffs, state them and pick a side with reasoning.

---

## Constraints

- Do not modify any source code.
- Do not run any commands that change the codebase.
- You may run read-only commands: `grep`, `wc`, `cat`, `find`, `cargo test` (to verify current state), `cargo clippy`.
- Base the assessment on what the code actually does, not what the documentation says it does. If they disagree, flag the discrepancy.
