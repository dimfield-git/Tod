use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::llm::{LlmError, LlmProvider, Usage};
use crate::schema::extract_json;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// What this step accomplishes.
    pub description: String,
    /// Files this step will create or modify (relative paths).
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
}

#[derive(Debug)]
pub enum PlanError {
    /// The LLM call itself failed.
    Llm(LlmError),
    /// Couldn't parse LLM output into a Plan.
    Parse { message: String, usage: Option<Usage> },
    /// Plan came back empty.
    Empty { usage: Option<Usage> },
    InvalidStep {
        index: usize,
        reason: String,
        usage: Option<Usage>,
    },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llm(e) => write!(f, "llm error: {e}"),
            Self::Parse { message, .. } => write!(f, "plan parse failed: {message}"),
            Self::Empty { .. } => write!(f, "plan has no steps"),
            Self::InvalidStep { index, reason, .. } => {
                write!(f, "invalid plan step {}: {reason}", index + 1)
            }
        }
    }
}

impl std::error::Error for PlanError {}

impl From<LlmError> for PlanError {
    fn from(e: LlmError) -> Self {
        Self::Llm(e)
    }
}

impl PlanError {
    /// Usage from the observed provider response, if available.
    pub fn observed_usage(&self) -> Option<&Usage> {
        match self {
            Self::Parse { usage, .. } | Self::Empty { usage } | Self::InvalidStep { usage, .. } => {
                usage.as_ref()
            }
            Self::Llm(_) => None,
        }
    }

    /// Whether this error path observed a provider response.
    pub fn response_observed(&self) -> bool {
        match self {
            Self::Llm(err) => err.response_observed(),
            Self::Parse { .. } | Self::Empty { .. } | Self::InvalidStep { .. } => true,
        }
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
) -> Result<(Plan, Option<Usage>), PlanError> {
    let user_msg = format!("## Goal\n{goal}\n\n## Project context\n{context}");
    let response = provider.complete(SYSTEM_PROMPT, &user_msg)?;
    let usage = response.usage.clone();
    let plan = extract_json::<Plan>(&response.text).map_err(|message| PlanError::Parse {
        message,
        usage: usage.clone(),
    })?;

    if plan.steps.is_empty() {
        return Err(PlanError::Empty { usage });
    }
    validate_plan(&plan, usage.clone())?;

    Ok((plan, response.usage))
}

fn validate_plan(plan: &Plan, usage: Option<Usage>) -> Result<(), PlanError> {
    for (index, step) in plan.steps.iter().enumerate() {
        if step.description.trim().is_empty() {
            return Err(PlanError::InvalidStep {
                index,
                reason: "empty description".to_string(),
                usage: usage.clone(),
            });
        }
        if step.files.is_empty() {
            return Err(PlanError::InvalidStep {
                index,
                reason: "step has no files".to_string(),
                usage: usage.clone(),
            });
        }
        for path in &step.files {
            if path.trim().is_empty() {
                return Err(PlanError::InvalidStep {
                    index,
                    reason: "contains empty file path".to_string(),
                    usage: usage.clone(),
                });
            }
            let p = Path::new(path);
            if p.is_absolute() {
                return Err(PlanError::InvalidStep {
                    index,
                    reason: format!("absolute path not allowed: {path}"),
                    usage: usage.clone(),
                });
            }
            if p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(PlanError::InvalidStep {
                    index,
                    reason: format!("path traversal not allowed: {path}"),
                    usage: usage.clone(),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmProvider, LlmResponse};
    use crate::schema::extract_json;

    /// Fake provider that returns a canned response.
    struct FakeProvider {
        response: String,
    }

    impl LlmProvider for FakeProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<LlmResponse, LlmError> {
            Ok(LlmResponse {
                text: self.response.clone(),
                usage: None,
            })
        }
    }

    struct FailProvider;

    impl LlmProvider for FailProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<LlmResponse, LlmError> {
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

    #[test]
    fn create_plan_success() {
        let provider = FakeProvider {
            response: r#"{"steps":[{"description":"create module","files":["src/foo.rs"]}]}"#
                .to_string(),
        };
        let (plan, usage) = create_plan(&provider, "goal", "ctx").unwrap();
        assert!(usage.is_none());
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn create_plan_rejects_empty_step_description() {
        let provider = FakeProvider {
            response: r#"{"steps":[{"description":"   ","files":["src/foo.rs"]}]}"#.to_string(),
        };
        let result = create_plan(&provider, "goal", "ctx");
        assert!(matches!(result, Err(PlanError::InvalidStep { .. })));
    }

    #[test]
    fn create_plan_rejects_empty_files() {
        let provider = FakeProvider {
            response: r#"{"steps":[{"description":"ok","files":[]}]}"#.to_string(),
        };
        let result = create_plan(&provider, "goal", "ctx");
        assert!(matches!(result, Err(PlanError::InvalidStep { .. })));
    }

    #[test]
    fn create_plan_rejects_path_traversal() {
        let provider = FakeProvider {
            response: r#"{"steps":[{"description":"ok","files":["../escape.rs"]}]}"#.to_string(),
        };
        let result = create_plan(&provider, "goal", "ctx");
        assert!(matches!(result, Err(PlanError::InvalidStep { .. })));
    }

    #[test]
    fn empty_plan_is_rejected() {
        let provider = FakeProvider {
            response: r#"{"steps":[]}"#.to_string(),
        };
        let result = create_plan(&provider, "goal", "ctx");
        assert!(matches!(result, Err(PlanError::Empty { .. })));
    }

    #[test]
    fn llm_failure_propagates_unchanged() {
        let result = create_plan(&FailProvider, "goal", "ctx");
        assert!(matches!(result, Err(PlanError::Llm(_))));
    }

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
