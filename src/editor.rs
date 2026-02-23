use std::path::Path;

use crate::llm::{LlmError, LlmProvider};
use crate::planner::PlanStep;
use crate::schema::{extract_json, validate_batch, EditBatch, ValidationError};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum EditError {
    /// The LLM call itself failed.
    Llm(LlmError),
    /// Couldn't parse LLM output into an EditBatch.
    Parse(String),
    /// Parsed but failed validation.
    Validation(ValidationError),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Llm(e) => write!(f, "llm error: {e}"),
            Self::Parse(msg) => write!(f, "edit parse failed: {msg}"),
            Self::Validation(e) => write!(f, "edit validation failed: {e}"),
        }
    }
}

impl std::error::Error for EditError {}

impl From<LlmError> for EditError {
    fn from(e: LlmError) -> Self {
        Self::Llm(e)
    }
}

impl From<ValidationError> for EditError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT: &str = r#"You are a coding editor. Given a plan step and file contents, produce the exact file edits needed to implement the step.

You have two actions:

1. WriteFile — create or overwrite an entire file:
   {"action": "write_file", "path": "src/foo.rs", "content": "full file contents here"}

2. ReplaceRange — replace lines in an existing file (1-indexed, inclusive):
   {"action": "replace_range", "path": "src/foo.rs", "start_line": 10, "end_line": 15, "content": "replacement lines"}

Rules:
- All paths must be relative. No absolute paths, no "..".
- ReplaceRange lines are 1-indexed. start must be >= 1 and start <= end.
- Prefer ReplaceRange for small changes to existing files.
- Use WriteFile for new files or complete rewrites.
- File contents are shown with numbered lines: "   N | code". Use these numbers for ReplaceRange.
- Maximum 20 edits per batch, maximum 512 KiB content per edit.
- Respond with ONLY a JSON object matching this schema, no other text:

{"edits": [ ... ]}
"#;

// ---------------------------------------------------------------------------
// File context formatting
// ---------------------------------------------------------------------------

/// Format file contents with numbered lines for the LLM.
///
/// Produces output like:
/// ```text
/// === src/main.rs ===
///    1 | fn main() {
///    2 |     println!("hello");
///    3 | }
/// ```
///
/// The loop calls this for each file before passing context to the editor.
pub fn format_file_context(path: &str, content: &str) -> String {
    let mut out = format!("=== {path} ===\n");
    for (i, line) in content.lines().enumerate() {
        out.push_str(&format!("{:4} | {line}\n", i + 1));
    }
    out
}

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Turn a plan step + file context into a validated EditBatch.
///
/// `file_context` should contain the numbered contents of each relevant file.
/// `sandbox_root` is the project root for path validation.
pub fn create_edits(
    provider: &dyn LlmProvider,
    step: &PlanStep,
    file_context: &str,
    sandbox_root: &Path,
) -> Result<EditBatch, EditError> {
    let user_msg = format!(
        "## Step\n{}\n\n## Files involved\n{}\n\n## Current file contents\n{}",
        step.description,
        step.files.join(", "),
        file_context,
    );

    let raw = provider.complete(SYSTEM_PROMPT, &user_msg)?;

    let batch = extract_json::<EditBatch>(&raw).map_err(EditError::Parse)?;

    validate_batch(&batch, sandbox_root)?;

    Ok(batch)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmProvider;
    use std::path::PathBuf;

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

    fn test_step() -> PlanStep {
        PlanStep {
            description: "create a new module".into(),
            files: vec!["src/foo.rs".into()],
        }
    }

    fn sandbox() -> PathBuf {
        PathBuf::from("/tmp/test-project")
    }

    #[test]
    fn create_edits_write_file() {
        let provider = FakeProvider {
            response: r#"{"edits":[{"action":"write_file","path":"src/foo.rs","content":"fn main() {}"}]}"#.into(),
        };
        let batch = create_edits(&provider, &test_step(), "", &sandbox()).unwrap();
        assert_eq!(batch.edits.len(), 1);
    }

    #[test]
    fn create_edits_replace_range() {
        let provider = FakeProvider {
            response: r#"{"edits":[{"action":"replace_range","path":"src/foo.rs","start_line":1,"end_line":3,"content":"new lines"}]}"#.into(),
        };
        let batch = create_edits(&provider, &test_step(), "", &sandbox()).unwrap();
        assert_eq!(batch.edits.len(), 1);
    }

    #[test]
    fn create_edits_with_markdown_fences() {
        let provider = FakeProvider {
            response: "```json\n{\"edits\":[{\"action\":\"write_file\",\"path\":\"src/foo.rs\",\"content\":\"hello\"}]}\n```".into(),
        };
        let batch = create_edits(&provider, &test_step(), "", &sandbox()).unwrap();
        assert_eq!(batch.edits.len(), 1);
    }

    #[test]
    fn rejects_path_traversal() {
        let provider = FakeProvider {
            response: r#"{"edits":[{"action":"write_file","path":"../etc/passwd","content":"bad"}]}"#.into(),
        };
        let result = create_edits(&provider, &test_step(), "", &sandbox());
        assert!(matches!(result, Err(EditError::Validation(_))));
    }

    #[test]
    fn rejects_absolute_path() {
        let provider = FakeProvider {
            response: r#"{"edits":[{"action":"write_file","path":"/etc/passwd","content":"bad"}]}"#.into(),
        };
        let result = create_edits(&provider, &test_step(), "", &sandbox());
        assert!(matches!(result, Err(EditError::Validation(_))));
    }

    #[test]
    fn parse_failure_propagates() {
        let provider = FakeProvider {
            response: "totally not json".into(),
        };
        let result = create_edits(&provider, &test_step(), "", &sandbox());
        assert!(matches!(result, Err(EditError::Parse(_))));
    }

    #[test]
    fn llm_failure_propagates() {
        let result = create_edits(&FailProvider, &test_step(), "", &sandbox());
        assert!(matches!(result, Err(EditError::Llm(_))));
    }

    #[test]
    fn mixed_edits_batch() {
        let provider = FakeProvider {
            response: r#"{"edits":[
                {"action":"write_file","path":"src/new.rs","content":"mod new;"},
                {"action":"replace_range","path":"src/main.rs","start_line":1,"end_line":1,"content":"mod new;"}
            ]}"#.into(),
        };
        let batch = create_edits(&provider, &test_step(), "", &sandbox()).unwrap();
        assert_eq!(batch.edits.len(), 2);
    }
    #[test]
    fn format_file_context_numbers_lines() {
        let result = format_file_context("src/main.rs", "fn main() {\n    println!(\"hi\");\n}");
        assert!(result.starts_with("=== src/main.rs ===\n"));
        assert!(result.contains("   1 | fn main() {"));
        assert!(result.contains("   2 |     println!(\"hi\");"));
        assert!(result.contains("   3 | }"));
    }
}
