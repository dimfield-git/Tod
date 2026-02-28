# Tod — Living List (post Phase 10)

Updated: 2026-02-28

154 passing tests, 1 ignored, clippy clean. Phases 1–10 complete.

---

## Tier 1: Opportunistic polish (do when touching nearby code)

### 1. `loop.rs` decomposition

The big extraction (`context.rs`) is done. Remaining responsibility accumulation (budget logic, fingerprint, logging) should be split opportunistically — e.g., if budget logic is modified, extract `budget.rs`. Do not pre-emptively reorganize.

### 2. Stricter JSON fence stripping

Current `extract_json` handles common cases (triple-backtick fences, preamble text). Edge cases remain: multiple code blocks in one response, JSON preceded by a short explanation, JSON embedded in markdown. Upgrade if real failures are observed, otherwise defer.

### 3. Public constants for magic numbers

Some magic numbers (depths, sizes, caps) are scattered as local `const` values. Making them `pub const` risks API surface creep. Prefer module-scoped `const` unless cross-module sharing is genuinely needed.

---

## Tier 2: Feature candidates

### 4. Patch/diff mode

Diff-based edits instead of full file rewrites. Reduces token cost and improves edit precision for large files. Big behavioral change — affects schema, editor prompts, and runner apply logic.

### 5. Git branch isolation

Run agent work on a throwaway branch, merge on success. Provides clean rollback and makes Tod safer on real projects. Requires git integration in runner or a new module.

### 6. Local model support

Ollama/llama.cpp provider behind existing `LlmProvider` trait. Valuable for offline use and cost control. Risk: scope explosion from model-specific quirks (context sizes, output format differences).

### 7. `--reflect` flag

Self-critique pass on planner output before execution. Cheap to implement (one extra LLM call), but affects prompt design and loop semantics. Useful for catching bad plans early.

### 8. Stronger fingerprint

Content hashing (instead of just file-size hashing) for resume correctness. Current fingerprint can miss same-size file changes. Correctness hardening, not required for prototype.
