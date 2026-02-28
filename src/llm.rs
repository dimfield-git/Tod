use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_RETRIES: usize = 3;
const INITIAL_BACKOFF_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LlmError {
    /// API key not found in environment.
    MissingApiKey,
    /// HTTP or network failure.
    RequestFailed(String),
    /// API returned a non-200 status.
    ApiError { status: u16, body: String },
    /// Response JSON didn't have the expected shape.
    UnexpectedResponse(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingApiKey => write!(f, "ANTHROPIC_API_KEY not set"),
            Self::RequestFailed(e) => write!(f, "request failed: {e}"),
            Self::ApiError { status, body } => write!(f, "API error {status}: {body}"),
            Self::UnexpectedResponse(msg) => write!(f, "unexpected response: {msg}"),
        }
    }
}

impl std::error::Error for LlmError {}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Token usage reported by an LLM provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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

/// Any LLM backend the agent can talk to.
///
/// Blocking by design — the agent loop is sequential.
pub trait LlmProvider {
    fn complete(&self, system: &str, user: &str) -> Result<LlmResponse, LlmError>;
}

// ---------------------------------------------------------------------------
// Anthropic implementation
// ---------------------------------------------------------------------------

pub struct AnthropicProvider {
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl AnthropicProvider {
    /// Build from environment.
    /// Reads `ANTHROPIC_API_KEY` — fails immediately if missing.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = env::var("ANTHROPIC_API_KEY").map_err(|_| LlmError::MissingApiKey)?;
        let model =
            env::var("TOD_MODEL").unwrap_or_else(|_| "claude-sonnet-4-5-20250929".to_string());
        let max_tokens: u32 = env::var("TOD_RESPONSE_MAX_TOKENS")
            .unwrap_or_else(|_| "4096".to_string())
            .parse()
            .map_err(|_| {
                LlmError::RequestFailed("TOD_RESPONSE_MAX_TOKENS must be a valid u32".to_string())
            })?;

        Ok(Self {
            api_key,
            model,
            max_tokens,
        })
    }
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503)
}

fn pseudo_random_offset(jitter_ms: u64) -> i64 {
    if jitter_ms == 0 {
        return 0;
    }
    let span = jitter_ms.saturating_mul(2).saturating_add(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let sample = nanos % span;
    sample as i64 - jitter_ms as i64
}

fn sleep_with_jitter(attempt: usize) {
    let base_ms = INITIAL_BACKOFF_MS.saturating_mul(2u64.saturating_pow(attempt as u32));
    let jitter_ms = base_ms / 4; // +-25%
    let offset = pseudo_random_offset(jitter_ms);
    let actual_ms = if offset >= 0 {
        base_ms.saturating_add(offset as u64)
    } else {
        base_ms.saturating_sub((-offset) as u64)
    };
    thread::sleep(Duration::from_millis(actual_ms));
}

impl LlmProvider for AnthropicProvider {
    fn complete(&self, system: &str, user: &str) -> Result<LlmResponse, LlmError> {
        let body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [
                { "role": "user", "content": user }
            ]
        });

        for attempt in 0..=MAX_RETRIES {
            let response = match ureq::post("https://api.anthropic.com/v1/messages")
                .config()
                .http_status_as_error(false)
                .build()
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .send_json(&body)
            {
                Ok(resp) => resp,
                Err(e) if attempt < MAX_RETRIES => {
                    eprintln!(
                        "warning: LLM request failed (attempt {}), retrying: {e}",
                        attempt + 1
                    );
                    sleep_with_jitter(attempt);
                    continue;
                }
                Err(e) => return Err(LlmError::RequestFailed(e.to_string())),
            };

            let status = response.status().as_u16();
            let body_str = match response.into_body().read_to_string() {
                Ok(s) => s,
                Err(e) if attempt < MAX_RETRIES => {
                    eprintln!(
                        "warning: LLM response read failed (attempt {}), retrying: {e}",
                        attempt + 1
                    );
                    sleep_with_jitter(attempt);
                    continue;
                }
                Err(e) => return Err(LlmError::RequestFailed(e.to_string())),
            };

            if status >= 400 {
                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    eprintln!(
                        "warning: LLM API error {status} (attempt {}), retrying: {}",
                        attempt + 1,
                        safe_preview(&body_str, 200)
                    );
                    sleep_with_jitter(attempt);
                    continue;
                }
                return Err(LlmError::ApiError {
                    status,
                    body: safe_preview(&body_str, 500).to_string(),
                });
            }

            let response_body: serde_json::Value = serde_json::from_str(&body_str)
                .map_err(|e| LlmError::UnexpectedResponse(e.to_string()))?;

            let text = response_body["content"][0]["text"]
                .as_str()
                .ok_or_else(|| {
                    let dump = response_body.to_string();
                    let preview = safe_preview(&dump, 200);
                    LlmError::UnexpectedResponse(format!("no text in content block: {preview}"))
                })?;

            let usage = response_body.get("usage").and_then(|u| {
                Some(Usage {
                    input_tokens: u.get("input_tokens")?.as_u64()?,
                    output_tokens: u.get("output_tokens")?.as_u64()?,
                })
            });

            return Ok(LlmResponse {
                text: text.to_string(),
                usage,
            });
        }

        Err(LlmError::RequestFailed(
            "retry loop exhausted without response".to_string(),
        ))
    }
}

/// Truncate a string for error messages without panicking on UTF-8 boundaries.
fn safe_preview(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        original_api_key: Option<String>,
        original_model: Option<String>,
        original_max_tokens: Option<String>,
    }

    impl EnvGuard {
        fn new() -> Self {
            Self {
                original_api_key: env::var("ANTHROPIC_API_KEY").ok(),
                original_model: env::var("TOD_MODEL").ok(),
                original_max_tokens: env::var("TOD_RESPONSE_MAX_TOKENS").ok(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original_api_key {
                Some(v) => env::set_var("ANTHROPIC_API_KEY", v),
                None => env::remove_var("ANTHROPIC_API_KEY"),
            }
            match &self.original_model {
                Some(v) => env::set_var("TOD_MODEL", v),
                None => env::remove_var("TOD_MODEL"),
            }
            match &self.original_max_tokens {
                Some(v) => env::set_var("TOD_RESPONSE_MAX_TOKENS", v),
                None => env::remove_var("TOD_RESPONSE_MAX_TOKENS"),
            }
        }
    }

    #[test]
    fn missing_key_returns_error() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::new();
        env::remove_var("ANTHROPIC_API_KEY");

        let result = AnthropicProvider::from_env();
        assert!(matches!(result, Err(LlmError::MissingApiKey)));
    }

    #[test]
    fn provider_builds_with_key() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::new();
        env::set_var("ANTHROPIC_API_KEY", "test-key-not-real");
        env::remove_var("TOD_MODEL");
        env::remove_var("TOD_RESPONSE_MAX_TOKENS");
        let provider = AnthropicProvider::from_env();
        assert!(provider.is_ok());
    }

    #[test]
    fn provider_uses_default_model() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::new();
        env::set_var("ANTHROPIC_API_KEY", "test-key-not-real");
        env::remove_var("TOD_MODEL");
        env::remove_var("TOD_RESPONSE_MAX_TOKENS");

        let provider = AnthropicProvider::from_env().unwrap();
        assert_eq!(provider.model, "claude-sonnet-4-5-20250929");
    }

    #[test]
    fn provider_reads_custom_model() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::new();
        env::set_var("ANTHROPIC_API_KEY", "test-key-not-real");
        env::set_var("TOD_MODEL", "claude-haiku-4-5-20251001");
        env::remove_var("TOD_RESPONSE_MAX_TOKENS");

        let provider = AnthropicProvider::from_env().unwrap();
        assert_eq!(provider.model, "claude-haiku-4-5-20251001");
    }

    #[test]
    fn provider_reads_custom_max_tokens() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::new();
        env::set_var("ANTHROPIC_API_KEY", "test-key-not-real");
        env::remove_var("TOD_MODEL");
        env::set_var("TOD_RESPONSE_MAX_TOKENS", "8192");

        let provider = AnthropicProvider::from_env().unwrap();
        assert_eq!(provider.max_tokens, 8192);
    }

    #[test]
    fn provider_rejects_invalid_max_tokens() {
        let _lock = env_lock().lock().unwrap();
        let _guard = EnvGuard::new();
        env::set_var("ANTHROPIC_API_KEY", "test-key-not-real");
        env::remove_var("TOD_MODEL");
        env::set_var("TOD_RESPONSE_MAX_TOKENS", "banana");

        let provider = AnthropicProvider::from_env();
        assert!(matches!(provider, Err(LlmError::RequestFailed(_))));
    }

    #[test]
    fn trait_signature_compiles() {
        // Proves the trait is object-safe and usable as a dynamic dispatch target.
        fn _accepts_provider(_p: &dyn LlmProvider) {}
    }

    #[test]
    fn usage_accumulate() {
        let mut base = Usage {
            input_tokens: 10,
            output_tokens: 20,
        };
        let add = Usage {
            input_tokens: 3,
            output_tokens: 7,
        };
        base.accumulate(&add);
        assert_eq!(
            base,
            Usage {
                input_tokens: 13,
                output_tokens: 27
            }
        );
    }

    #[test]
    fn usage_total() {
        let usage = Usage {
            input_tokens: 12,
            output_tokens: 5,
        };
        assert_eq!(usage.total(), 17);
    }

    #[test]
    fn usage_default_is_zero() {
        assert_eq!(
            Usage::default(),
            Usage {
                input_tokens: 0,
                output_tokens: 0
            }
        );
    }

    #[test]
    fn is_retryable_429() {
        assert!(is_retryable_status(429));
    }

    #[test]
    fn is_retryable_500() {
        assert!(is_retryable_status(500));
    }

    #[test]
    fn not_retryable_400() {
        assert!(!is_retryable_status(400));
    }

    #[test]
    fn not_retryable_401() {
        assert!(!is_retryable_status(401));
    }

    #[test]
    #[ignore] // requires live API key
    fn smoke_real_api_call() {
        let provider = AnthropicProvider::from_env().expect("ANTHROPIC_API_KEY must be set");
        let response = provider
            .complete("Respond with only the word 'hello'.", "Say hello.")
            .expect("API call failed");
        assert!(
            response.text.to_lowercase().contains("hello"),
            "got: {}",
            response.text
        );
    }
}
