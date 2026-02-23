use serde::Deserialize;

use crate::llm::{LlmError, LlmProvider};
use crate::schema::extract_json;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PlanStep {
    /// What this step accomplishes.
    pub description: String,
    /// Files this step will create or modify (relative paths).
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}

#[derive(Debug)]
pub enum PlanError {
    /// The LLM call itself failed.
    Llm(LlmError),
    /// Couldn't parse LLM output into a Plan.
    Parse(String),
    /// Plan came back empty.
    Empty,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llm(e) => write!(f, "llm error: {e}"),
            Self::Parse(msg) => write!(f, "plan parse failed: {msg}"),
            Self::Empty => write!(f, "plan has no steps"),
        }
    }
}

impl std::error::Error for PlanError {}

impl From<LlmError> for PlanError {
    fn from(e: LlmError) -> Self {
        Self::Llm(e)
    }
}

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT: &str = r#"You are a coding planner. Given a goal and project context, produce an ordered list of implementation steps.

Rules:
- Each step is a single, concrete change (create a file, modify a function, add a dependency).
- Each step lists the relative file paths it will create or modify.
- Steps are ordered so earlier steps never depend on later ones.
- Do NOT write code. Describe what to do, not how to do it in code.
- Respond with ONLY a JSON object matching this schema, no other text:

{
  "steps": [
    {
      "description": "what this step does",
      "files": ["src/foo.rs", "Cargo.toml"]
    }
  ]
}
"#;

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Turn a goal + project context into an ordered plan.
///
/// `context` should contain the file tree and any relevant file contents.
/// This function does NOT read the filesystem — the caller provides context.
pub fn create_plan(
    provider: &dyn LlmProvider,
    goal: &str,
    context: &str,
) -> Result<Plan, PlanError> {
    let user_msg = format!("## Goal\n{goal}\n\n## Project context\n{context}");
    let raw = provider.complete(SYSTEM_PROMPT, &user_msg)?;
    let plan = extract_json::<Plan>(&raw).map_err(PlanError::Parse)?;

    if plan.steps.is_empty() {
        return Err(PlanError::Empty);
    }

    Ok(plan)
}





// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmProvider;
    use crate::schema::extract_json;

    /// Fake provider that returns a canned response.
    struct FakeProvider {
        response: String,
    }

    impl LlmProvider for FakeProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<String, LlmError> {
            Ok(self.response.clone())
        }
    }

    struct FailProvider;

    impl LlmProvider for FailProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<String, LlmError> {
            Err(LlmError::RequestFailed("fake failure".into()))
        }
    }

    #[test]
    fn parse_clean_json() {
        let json = r#"{"steps":[{"description":"create module","files":["src/foo.rs"]}]}"#;
        let plan: Plan = extract_json(json).unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].files, vec!["src/foo.rs"]);
    }

    #[test]
    fn parse_with_markdown_fences() {
        let raw = "```json\n{\"steps\":[{\"description\":\"do thing\",\"files\":[\"a.rs\"]}]}\n```";
        let plan: Plan = extract_json(raw).unwrap();
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn parse_with_preamble_garbage() {
        let raw = "Sure! Here's the plan:\n{\"steps\":[{\"description\":\"step one\",\"files\":[\"b.rs\"]}]}";
        let plan: Plan = extract_json(raw).unwrap();
        assert_eq!(plan.steps[0].description, "step one");
    }

    #[test]
    fn parse_garbage_fails() {
        let result = extract_json::<Plan>("not json at all");
        assert!(result.is_err());
    }

    // ... (create_plan_with_fake_provider, empty_plan_is_rejected, llm_failure_propagates unchanged)

    #[test]
    fn multi_step_plan() {
        let json = r#"{
            "steps": [
                {"description": "create schema", "files": ["src/schema.rs"]},
                {"description": "add tests", "files": ["src/schema.rs"]},
                {"description": "wire into main", "files": ["src/main.rs"]}
            ]
        }"#;
        let plan: Plan = extract_json(json).unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[2].files, vec!["src/main.rs"]);
    }
}
