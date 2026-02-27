# Phase 8 Implementation Log (2026-02-27)

This document records all code and test changes made to implement Phase 8 (`PHASE8.md`) in Tod.

## Scope

Implemented all five Phase 8 tasks in order:
1. TempSandbox extraction
2. Atomic checkpoint writes
3. Explicit truncation flag
4. Provider config via env
5. Budget enforcement

Verification was run after each task boundary with:

```bash
cargo test
cargo clippy -- -D warnings
```

Final verification state:
- `cargo test`: 133 passed, 1 ignored, 0 failed
- `cargo clippy -- -D warnings`: clean

---

## Task 1: TempSandbox extraction

### Added
- `src/test_util.rs` (new, `#[cfg(test)]`): shared `TempSandbox` helper with:
  - `TempSandbox::new()`
  - `TempSandbox::with_main_rs()`
  - `Deref<Target = Path>`
  - `Drop` cleanup

### Updated
- `src/main.rs`
  - Added `#[cfg(test)] mod test_util;`

- `src/runner.rs` tests
  - Removed local TempSandbox definition and local static test id
  - Imported `use crate::test_util::TempSandbox;`
  - Removed now-unused local imports

- `src/loop.rs` tests
  - Removed local TempSandbox definition and local static test id
  - Imported `use crate::test_util::TempSandbox;`
  - Removed now-unused local imports

- `src/stats.rs` tests
  - Removed local TempSandbox definition and local static test id
  - Imported `use crate::test_util::TempSandbox;`
  - Removed now-unused local imports

### Result
- No behavioral runtime changes, test-only infrastructure deduplicated.

---

## Task 2: Atomic checkpoint writes

### Updated
- `src/loop.rs`
  - `RunState::checkpoint()` now writes to `.tod/state.json.tmp` first, then `fs::rename` to `.tod/state.json`.
  - Added warning paths for temp-write and rename failure.

### Added tests
- `src/loop.rs`
  - `checkpoint_is_atomic`
    - Verifies final `state.json` exists
    - Verifies temporary `state.json.tmp` does not remain
    - Verifies `state.json` deserializes to `RunState`

### Result
- Checkpoint corruption risk from interrupted direct-write reduced via atomic rename flow.

---

## Task 3: Explicit truncation flag

### Updated
- `src/runner.rs`
  - `RunResult::Failure` now includes `truncated: bool`
  - `truncate_output` signature changed from:
    - `fn truncate_output(...) -> String`
    - to `fn truncate_output(...) -> (String, bool)`
  - `run_pipeline` now passes explicit truncation bool from truncation helper
  - command-execution errors set `truncated: false`

- `src/reviewer.rs`
  - Failure pattern match updated to ignore extra fields with `..`
  - Test fixtures updated to construct `RunResult::Failure { ..., truncated: false }`

- `src/loop.rs`
  - `write_attempt_log` now reads `truncated` from `RunResult::Failure` directly
  - Removed previous string-based truncation inference (`output.contains("[truncated")`)

### Updated tests
- `src/runner.rs`
  - Truncation tests now assert tuple behavior (`(output, was_truncated)`)

### Result
- Truncation handling is now explicit and type-safe; no log behavior depends on string parsing.

---

## Task 4: Provider config via env

### Updated
- `src/llm.rs`
  - `AnthropicProvider::from_env()` now reads:
    - `ANTHROPIC_API_KEY` (required)
    - `TOD_MODEL` (default: `claude-sonnet-4-5-20250929`)
    - `TOD_RESPONSE_MAX_TOKENS` (default: `4096`, must parse as `u32`)
  - Invalid `TOD_RESPONSE_MAX_TOKENS` returns `LlmError::RequestFailed` with explicit message.

### Added tests (`src/llm.rs`)
- `provider_uses_default_model`
- `provider_reads_custom_model`
- `provider_reads_custom_max_tokens`
- `provider_rejects_invalid_max_tokens`

### Test harness updates
- Extended `EnvGuard` and restore logic to include:
  - `TOD_MODEL`
  - `TOD_RESPONSE_MAX_TOKENS`

### Result
- Provider model and response-token limit are runtime-configurable via env vars.

---

## Task 5: Budget enforcement

### 5a/5b LLM usage types and trait

#### Updated
- `src/llm.rs`
  - Added `Usage` struct:
    - `input_tokens: u64`
    - `output_tokens: u64`
    - helpers: `total()`, `accumulate(...)`
  - Added `LlmResponse` struct:
    - `text: String`
    - `usage: Option<Usage>`
  - `LlmProvider::complete` changed to return `Result<LlmResponse, LlmError>`

#### Added tests
- `usage_accumulate`
- `usage_total`
- `usage_default_is_zero`

---

### 5c Anthropic usage parsing

#### Updated
- `src/llm.rs`
  - Anthropic response parsing now extracts optional usage from `response_body["usage"]`:
    - `input_tokens`
    - `output_tokens`
  - Returns `LlmResponse { text, usage }`

---

### 5d Caller signature propagation

#### Updated
- `src/planner.rs`
  - `create_plan(...)` return type changed to `Result<(Plan, Option<Usage>), PlanError>`
  - Extracts text via `response.text`
  - Returns parsed plan with usage payload

- `src/editor.rs`
  - `create_edits(...)` return type changed to `Result<(EditBatch, Option<Usage>), EditError>`
  - Extracts text via `response.text`
  - Returns validated edit batch with usage payload

- Test fake providers in both modules migrated to `LlmResponse`

---

### 5e/5f Run state, config, CLI cap

#### Updated
- `src/config.rs`
  - Added `RunConfig.max_tokens: u64` (default `0`)

- `src/cli.rs`
  - Added `run` flag:
    - `--max-tokens <u64>` (default `0`)
  - Wired into `RunConfig` in `into_run_config()`
  - Tests updated for new flag default behavior

- `src/loop.rs` (`RunState`)
  - Added with `#[serde(default)]`:
    - `usage: Usage`
    - `llm_requests: u64`
    - `max_tokens: u64`
  - Initialized in `RunState::new` from config

---

### 5g/5h Budget check and error

#### Updated
- `src/loop.rs`
  - After planning call: accumulates call usage and increments request count
  - After each edit-generation call: accumulates call usage and increments request count
  - Budget guard checks after each accumulation:
    - if `max_tokens > 0 && usage.total() > max_tokens` -> checkpoint and abort

- `src/loop.rs` (`LoopError`)
  - Added `TokenCapExceeded { used: u64, cap: u64 }`
  - Display message:
    - `token budget exceeded: used {used} tokens, cap was {cap}`

---

### 5i Stats token output

#### Updated
- `src/stats.rs`
  - `RunSummary` now includes:
    - `input_tokens`, `output_tokens`, `total_tokens`, `llm_requests`
  - `MultiRunSummary` now includes:
    - `avg_tokens`
  - `summarize_run` reads cumulative usage from latest attempt log and computes token totals
  - `summarize_runs` aggregates token totals and computes average tokens
  - `format_run_summary` conditionally appends:
    - `Tokens: <in> in / <out> out (<requests> requests)` when total > 0
  - `format_multi_run_summary` includes:
    - `Avg tokens: ...`

#### Added tests
- `format_run_summary_shows_tokens`
- `format_run_summary_hides_zero_tokens`

---

### 5j Attempt log usage fields

#### Updated
- `src/loop.rs` (`AttemptLog`)
  - Added with `#[serde(default)]`:
    - `usage_this_call: Option<Usage>`
    - `usage_cumulative: Usage`
  - `write_attempt_log(...)` now accepts per-call usage and snapshots cumulative usage

---

### Task 5 tests in loop

#### Added (`src/loop.rs`)
- `token_cap_aborts_run`
  - Uses fake provider with usage payload
  - Runs with `max_tokens=1`
  - Asserts `LoopError::TokenCapExceeded`

- `usage_survives_checkpoint`
  - Runs with usage-bearing fake responses
  - Reads `.tod/state.json`
  - Verifies `RunState.usage` persisted as non-zero

---

## Additional note

A pre-existing workspace change (`PHASE7.md` deleted) was intentionally left untouched per user instruction.

