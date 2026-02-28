# PHASE8.md — Hardening + Budget Enforcement

**Read AGENTS.md first.** All operating principles, coding standards, and safety rules apply.

---

## Goal

Five bounded changes that harden Tod's internals and add token-level budget tracking. Each change is independently testable. Complete them in order.

---

## Task 1: TempSandbox extraction

### What

Extract the duplicated `TempSandbox` test helper from `runner.rs`, `loop.rs`, and `stats.rs` into a single shared module.

### New file: `src/test_util.rs`

```rust
#![cfg(test)]

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// RAII temp directory — cleaned up on drop (even on panic).
pub struct TempSandbox(PathBuf);

impl TempSandbox {
    pub fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("tod_test_{}_{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// Create the sandbox with a `src/main.rs` already present.
    pub fn with_main_rs() -> Self {
        let sb = Self::new();
        fs::create_dir_all(sb.join("src")).unwrap();
        fs::write(sb.join("src/main.rs"), "fn main() {}\n").unwrap();
        sb
    }
}

impl Deref for TempSandbox {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
```

### In `main.rs`

Add at the bottom, alongside the other `mod` declarations:

```rust
#[cfg(test)]
mod test_util;
```

### Changes to existing modules

In `runner.rs`, `loop.rs`, and `stats.rs`:

- Delete the local `TempSandbox` struct, `TEST_ID` static, `Deref` impl, and `Drop` impl.
- Delete any local `with_main_rs()` implementations.
- Add to each test module's imports: `use crate::test_util::TempSandbox;`
- Remove now-unused imports (`AtomicUsize`, `Ordering`, `Deref`) if they become dead.

### Tests

No new tests. All existing tests must still pass. The only change is import paths.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **121 passing, 1 ignored.**

**Do not start Task 2 until Task 1 is verified.**

---

## Task 2: Atomic checkpoint writes

### What

Prevent corrupted `state.json` if the process is killed mid-write. Write to a temp file, then atomically rename.

### In `loop.rs` — `RunState::checkpoint()`

Replace the direct `fs::write(tod_dir.join("state.json"), json)` with:

```rust
let tmp_path = tod_dir.join("state.json.tmp");
let final_path = tod_dir.join("state.json");
if let Err(e) = fs::write(&tmp_path, json) {
    eprintln!("warning: failed to write checkpoint: {e}");
    return;
}
if let Err(e) = fs::rename(&tmp_path, &final_path) {
    eprintln!("warning: failed to finalize checkpoint: {e}");
}
```

### Tests

Add one test in `loop.rs`:

| Test | Setup | Assertion |
|------|-------|-----------|
| `checkpoint_is_atomic` | Create RunState, call `checkpoint()`, verify `state.json` exists and `state.json.tmp` does not | File contents deserialize to valid RunState |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **122 passing, 1 ignored.**

**Do not start Task 3 until Task 2 is verified.**

---

## Task 3: Explicit truncation flag

### What

Stop detecting truncation via string search (`output.contains("[truncated")`). Make it a boolean returned from `truncate_output` and carried through `RunResult`.

### In `runner.rs`

Change `truncate_output` signature:

```rust
fn truncate_output(raw: &str, max_bytes: usize) -> (String, bool)
```

Returns `(output, was_truncated)`. Update `run_pipeline` to destructure and pass the bool.

Add `truncated: bool` to `RunResult::Failure`:

```rust
pub enum RunResult {
    Success,
    Failure {
        stage: String,
        output: String,
        truncated: bool,
    },
}
```

### Ripple changes

**`reviewer.rs`** — `review()` matches on `RunResult::Failure`. Add `..` to the pattern (reviewer doesn't use `truncated`).

**`loop.rs`** — `write_attempt_log()` currently infers truncation via string search:

```rust
let truncated = output.contains("[truncated");
```

Replace this. Extract `truncated` from the `RunResult::Failure` variant directly:

```rust
let (stage, ok, output, truncated) = match run_result {
    RunResult::Success => ("success".to_string(), true, String::new(), false),
    RunResult::Failure { stage, output, truncated } => {
        (stage.clone(), false, output.clone(), *truncated)
    }
};
```

**`loop.rs`** — `run_from_state()` matches on `RunResult::Failure` for the dry-run branch and apply branch. Update patterns to include `truncated` or use `..`.

### Tests

Update existing truncation tests in `runner.rs` to assert on the tuple:

| Test | Assertion |
|------|-----------|
| `no_truncation_under_limit` | Returns `(original, false)` |
| `truncation_snaps_to_line_boundary` | Returns `(truncated_output, true)` |
| `exact_limit_no_truncation` | Returns `(original, false)` |

Update any `RunResult::Failure { stage, output }` pattern matches in existing tests across `runner.rs`, `reviewer.rs`, and `loop.rs` to include `truncated` or `..`.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **122 passing, 1 ignored** (no new tests, existing ones updated).

**Do not start Task 4 until Task 3 is verified.**

---

## Task 4: Provider config via env

### What

Move model name and response max tokens out of hardcoded values in `AnthropicProvider`. Read from environment variables with sensible defaults.

### In `llm.rs` — `AnthropicProvider::from_env()`

Replace the hardcoded values:

```rust
pub fn from_env() -> Result<Self, LlmError> {
    let api_key = env::var("ANTHROPIC_API_KEY")
        .map_err(|_| LlmError::MissingApiKey)?;

    let model = env::var("TOD_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-5-20250929".to_string());

    let max_tokens: u32 = env::var("TOD_RESPONSE_MAX_TOKENS")
        .unwrap_or_else(|_| "4096".to_string())
        .parse()
        .map_err(|_| LlmError::RequestFailed(
            "TOD_RESPONSE_MAX_TOKENS must be a valid u32".to_string()
        ))?;

    Ok(Self {
        api_key,
        model,
        max_tokens,
    })
}
```

### Tests

Add to `llm.rs` tests:

| Test | Setup | Assertion |
|------|-------|-----------|
| `provider_uses_default_model` | Set `ANTHROPIC_API_KEY`, unset `TOD_MODEL` | Provider builds, model field is default string |
| `provider_reads_custom_model` | Set `TOD_MODEL=claude-haiku-4-5-20251001` | Provider builds, model matches env |
| `provider_reads_custom_max_tokens` | Set `TOD_RESPONSE_MAX_TOKENS=8192` | Provider builds, max_tokens is 8192 |
| `provider_rejects_invalid_max_tokens` | Set `TOD_RESPONSE_MAX_TOKENS=banana` | Returns error |

Use the existing `env_lock()` and `EnvGuard` pattern from `llm.rs` tests to avoid test interference.

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **126 passing, 1 ignored.**

**Do not start Task 5 until Task 4 is verified.**

---

## Task 5: Budget enforcement

### What

Track token usage across all LLM calls in a run. Accumulate in `RunState` so it checkpoints and survives resume. Optionally abort when a configurable token cap is exceeded. Show usage in stats output.

### 5a: New types in `llm.rs`

```rust
/// Token usage reported by an LLM provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    pub fn accumulate(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

/// Full response from an LLM provider call.
pub struct LlmResponse {
    pub text: String,
    pub usage: Option<Usage>,
}
```

### 5b: Change `LlmProvider` trait

```rust
pub trait LlmProvider {
    fn complete(&self, system: &str, user: &str) -> Result<LlmResponse, LlmError>;
}
```

### 5c: Update `AnthropicProvider`

Parse usage from the API response:

```rust
let usage = response_body.get("usage").and_then(|u| {
    Some(Usage {
        input_tokens: u.get("input_tokens")?.as_u64()?,
        output_tokens: u.get("output_tokens")?.as_u64()?,
    })
});

Ok(LlmResponse {
    text: text.to_string(),
    usage,
})
```

### 5d: Update callers

**`planner.rs`** — `create_plan()`:
- `provider.complete()` now returns `LlmResponse`.
- Extract `raw = response.text`.
- Return `(Plan, Option<Usage>)` — change signature to `Result<(Plan, Option<Usage>), PlanError>`.

**`editor.rs`** — `create_edits()`:
- Same pattern: extract `.text`, return `(EditBatch, Option<Usage>)`.
- Change signature to `Result<(EditBatch, Option<Usage>), EditError>`.

**`loop.rs`** — `run_from_state()`:
- After `create_plan()`: accumulate usage into `state.usage`.
- After `create_edits()`: accumulate usage into `state.usage`, then check cap.

### 5e: Add usage and cap to `RunState`

```rust
pub struct RunState {
    // ... existing fields ...

    /// Accumulated token usage across all LLM calls in this run.
    #[serde(default)]
    pub usage: Usage,

    /// Total token requests made in this run.
    #[serde(default)]
    pub llm_requests: u64,

    /// Optional token cap (input + output combined). 0 = no cap.
    #[serde(default)]
    pub max_tokens: u64,
}
```

`#[serde(default)]` on the new fields ensures old `state.json` files from Phase 7 still deserialize.

### 5f: Add cap to config and CLI

**`config.rs`** — add to `RunConfig`:

```rust
/// Max total tokens (input + output) across all LLM calls. 0 = no cap.
pub max_tokens: u64,
```

Default: `0` (no cap).

**`cli.rs`** — add to `Command::Run`:

```rust
/// Max total tokens (input + output) for the entire run. 0 = no limit.
#[arg(long, default_value_t = 0)]
max_tokens: u64,
```

Wire into `RunConfig` in `into_run_config()`. Copy into `RunState` in `RunState::new()`.

### 5g: Budget check in loop

In `run_from_state()`, after each LLM call (both `create_plan` and `create_edits`):

```rust
if let Some(usage) = &call_usage {
    state.usage.accumulate(usage);
    state.llm_requests += 1;
}

if state.max_tokens > 0 && state.usage.total() > state.max_tokens {
    state.checkpoint(config);
    return Err(LoopError::TokenCapExceeded {
        used: state.usage.total(),
        cap: state.max_tokens,
    });
}
```

### 5h: New `LoopError` variant

```rust
TokenCapExceeded { used: u64, cap: u64 },
```

Display: `"token budget exceeded: used {used} tokens, cap was {cap}"`

### 5i: Update stats output

**`format_run_summary`** — append a line:

```
Tokens:     1,234 in / 567 out (3 requests)
```

Only show if `usage.total() > 0`.

**`format_multi_run_summary`** — append:

```
  Avg tokens: 1,801
```

Add `avg_tokens: f64` to `MultiRunSummary`. Compute from per-run token totals.

This requires `summarize_run` to also return token data. Add `total_tokens: u64` and `llm_requests: u64` to `RunSummary`.

For Phase 7 runs that don't have usage in their logs: the fields will be 0 via `#[serde(default)]`. Stats will show 0 tokens for old runs, which is correct.

### 5j: Update attempt logs

In `write_attempt_log()`, add to the `AttemptLog` struct:

```rust
pub usage_this_call: Option<Usage>,
pub usage_cumulative: Usage,
```

`usage_this_call` comes from the editor response for this iteration. `usage_cumulative` is a snapshot of `state.usage` at log time.

Use `#[serde(default)]` on both so old log files still parse.

### Tests

Add to `llm.rs`:

| Test | Assertion |
|------|-----------|
| `usage_accumulate` | Two `Usage` values accumulate correctly |
| `usage_total` | `total()` returns sum of input + output |
| `usage_default_is_zero` | `Usage::default()` has all zeros |

Add to `loop.rs`:

| Test | Assertion |
|------|-----------|
| `token_cap_aborts_run` | Run with `max_tokens=1` (impossibly low) aborts with `TokenCapExceeded` |
| `usage_survives_checkpoint` | After run, deserialize `state.json`, verify `usage` fields are non-default |

Add to `stats.rs`:

| Test | Assertion |
|------|-----------|
| `format_run_summary_shows_tokens` | Summary with nonzero usage includes "Tokens:" line |
| `format_run_summary_hides_zero_tokens` | Summary with zero usage omits "Tokens:" line |

### Verify

```
cargo test
cargo clippy -- -D warnings
```

Baseline after this task: **131+ passing, 1 ignored.**

---

## Implementation order summary

| Task | Scope | Files touched |
|------|-------|---------------|
| 1. TempSandbox extraction | Test infra only | `test_util.rs` (new), `main.rs`, `runner.rs`, `loop.rs`, `stats.rs` |
| 2. Atomic checkpoint | `loop.rs` only | `loop.rs` |
| 3. Truncation flag | Runner → loop ripple | `runner.rs`, `loop.rs`, `reviewer.rs` |
| 4. Provider config | `llm.rs` only | `llm.rs` |
| 5. Budget enforcement | Cross-cutting | `llm.rs`, `planner.rs`, `editor.rs`, `loop.rs`, `config.rs`, `cli.rs`, `stats.rs` |

**Do not start a later task until the preceding task is verified passing.**

---

## Phase 8 "done" criteria

- All five tasks complete and verified.
- `cargo test` passes with ≥ 131 tests, 0 failing, 1 ignored.
- `cargo clippy -- -D warnings` clean.
- No duplicated `TempSandbox` definitions remain.
- `state.json` writes are atomic (tmp + rename).
- Truncation is a boolean carried through `RunResult`, not inferred from strings.
- Model name and response max tokens are configurable via env vars.
- Token usage is tracked per run, logged per attempt, shown in stats output.
- `--max-tokens` cap deterministically aborts with explicit error.
- Resume continues from prior token totals.
