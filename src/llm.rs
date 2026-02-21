use serde_json::json;
use std::env;

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

/// Any LLM backend the agent can talk to.
///
/// Blocking by design — the agent loop is sequential.
pub trait LlmProvider {
    fn complete(&self, system: &str, user: &str) -> Result<String, LlmError>;
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

        Ok(Self {
            api_key,
            model: "claude-sonnet-4-5-20250929".to_string(),
            max_tokens: 4096,
        })
    }
}

impl LlmProvider for AnthropicProvider {
    fn complete(&self, system: &str, user: &str) -> Result<String, LlmError> {
        let body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": system,
            "messages": [
                { "role": "user", "content": user }
            ]
        });

        let response = ureq::post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .send_json(&body)
            .map_err(|e: ureq::Error| LlmError::RequestFailed(e.to_string()))?;

        let body_str = response.into_body().read_to_string()
            .map_err(|e: ureq::Error| LlmError::UnexpectedResponse(e.to_string()))?;
        let response_body: serde_json::Value = serde_json::from_str(&body_str)
            .map_err(|e| LlmError::UnexpectedResponse(e.to_string()))?;

        // Extract text from first content block
        let text = response_body["content"][0]["text"]
            .as_str()
            .ok_or_else(|| {
                LlmError::UnexpectedResponse(format!(
                    "no text in content block: {}",
                    &response_body.to_string()[..response_body.to_string().len().min(200)]
                ))
            })?;

        Ok(text.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_returns_error() {
        // Temporarily unset the key
        let original = env::var("ANTHROPIC_API_KEY").ok();
        env::remove_var("ANTHROPIC_API_KEY");

        let result = AnthropicProvider::from_env();
        assert!(matches!(result, Err(LlmError::MissingApiKey)));

        // Restore if it existed
        if let Some(key) = original {
            env::set_var("ANTHROPIC_API_KEY", key);
        }
    }

    #[test]
    fn provider_builds_with_key() {
        env::set_var("ANTHROPIC_API_KEY", "test-key-not-real");
        let provider = AnthropicProvider::from_env();
        assert!(provider.is_ok());
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn trait_signature_compiles() {
        // Proves the trait is object-safe and usable as a dynamic dispatch target.
        fn _accepts_provider(_p: &dyn LlmProvider) {}
    }

    #[test]
    #[ignore] // requires live API key
    fn smoke_real_api_call() {
        let provider = AnthropicProvider::from_env().expect("ANTHROPIC_API_KEY must be set");
        let response = provider.complete("Respond with only the word 'hello'.", "Say hello.")
            .expect("API call failed");
        assert!(response.to_lowercase().contains("hello"), "got: {response}");
    }
}