use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Hard ceiling on content bytes per single edit action.
const MAX_CONTENT_BYTES: usize = 512 * 1024; // 512 KiB

/// Hard ceiling on edit actions per iteration.
const MAX_ACTIONS_PER_BATCH: usize = 20;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single edit the LLM wants to apply to the project.
///
/// Deserialized directly from the JSON the model returns.
/// Every variant carries a `path` relative to the project root.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "action")]
pub enum EditAction {
    /// Overwrite (or create) an entire file.
    #[serde(rename = "write_file")]
    WriteFile { path: String, content: String },

    /// Replace a line range within an existing file.
    /// Lines are 1-indexed, inclusive on both ends.
    #[serde(rename = "replace_range")]
    ReplaceRange {
        path: String,
        start_line: usize,
        end_line: usize,
        content: String,
    },
}

/// Wrapper for a batch of edits — what the LLM returns per iteration.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EditBatch {
    pub edits: Vec<EditAction>,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Everything that can go wrong with an edit before we touch the filesystem.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    AbsolutePath { path: String },
    PathTraversal { path: String },
    PathEscapesSandbox { path: String, resolved: PathBuf },
    EmptyPath,
    ContentTooLarge { path: String, bytes: usize, max: usize },
    InvalidRange { path: String, start: usize, end: usize },
    ZeroLineIndex { path: String },
    BatchTooLarge { count: usize, max: usize },
    EmptyBatch,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AbsolutePath { path } => write!(f, "absolute path not allowed: {path}"),
            Self::PathTraversal { path } => write!(f, "path traversal (..) not allowed: {path}"),
            Self::PathEscapesSandbox { path, resolved } => {
                write!(f, "path escapes sandbox: {path} -> {}", resolved.display())
            }
            Self::EmptyPath => write!(f, "empty path"),
            Self::ContentTooLarge { path, bytes, max } => {
                write!(f, "content too large for {path}: {bytes} bytes (max {max})")
            }
            Self::InvalidRange { path, start, end } => {
                write!(f, "invalid line range for {path}: {start}..{end}")
            }
            Self::ZeroLineIndex { path } => {
                write!(f, "line indices are 1-based, got 0 for {path}")
            }
            Self::BatchTooLarge { count, max } => {
                write!(f, "batch has {count} edits, max is {max}")
            }
            Self::EmptyBatch => write!(f, "edit batch is empty"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate that `path` is safe to use inside `sandbox_root`.
///
/// Rules:
/// - Must not be empty
/// - Must not be absolute
/// - Must not contain `..` components
/// - When joined to `sandbox_root`, must still resolve inside it
pub fn validate_path(raw: &str, sandbox_root: &Path) -> Result<PathBuf, ValidationError> {
    // Empty check
    if raw.is_empty() {
        return Err(ValidationError::EmptyPath);
    }

    let p = Path::new(raw);

    // Absolute path check
    if p.is_absolute() {
        return Err(ValidationError::AbsolutePath {
            path: raw.to_string(),
        });
    }

    // Component-level traversal check
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(ValidationError::PathTraversal {
                path: raw.to_string(),
            });
        }
    }

    // Resolved path must stay inside sandbox
    // We use lexical joining here — the file doesn't need to exist yet.
    let resolved = sandbox_root.join(p);
    let canon_sandbox = normalize_lexical(sandbox_root);
    let canon_resolved = normalize_lexical(&resolved);

    if !canon_resolved.starts_with(&canon_sandbox) {
        return Err(ValidationError::PathEscapesSandbox {
            path: raw.to_string(),
            resolved: canon_resolved,
        });
    }

    Ok(resolved)
}

/// Validate content size against the hard limit.
pub fn validate_content_size(path: &str, content: &str) -> Result<(), ValidationError> {
    let bytes = content.len();
    if bytes > MAX_CONTENT_BYTES {
        return Err(ValidationError::ContentTooLarge {
            path: path.to_string(),
            bytes,
            max: MAX_CONTENT_BYTES,
        });
    }
    Ok(())
}

/// Validate a line range for `ReplaceRange`.
/// Lines are 1-indexed; start <= end required.
pub fn validate_range(path: &str, start: usize, end: usize) -> Result<(), ValidationError> {
    if start == 0 || end == 0 {
        return Err(ValidationError::ZeroLineIndex {
            path: path.to_string(),
        });
    }
    if start > end {
        return Err(ValidationError::InvalidRange {
            path: path.to_string(),
            start,
            end,
        });
    }
    Ok(())
}

/// Validate a single `EditAction`.
pub fn validate_edit(action: &EditAction, sandbox_root: &Path) -> Result<(), ValidationError> {
    match action {
        EditAction::WriteFile { path, content } => {
            validate_path(path, sandbox_root)?;
            validate_content_size(path, content)?;
        }
        EditAction::ReplaceRange {
            path,
            start_line,
            end_line,
            content,
        } => {
            validate_path(path, sandbox_root)?;
            validate_content_size(path, content)?;
            validate_range(path, *start_line, *end_line)?;
        }
    }
    Ok(())
}

/// Validate an entire `EditBatch`.
pub fn validate_batch(batch: &EditBatch, sandbox_root: &Path) -> Result<(), ValidationError> {
    if batch.edits.is_empty() {
        return Err(ValidationError::EmptyBatch);
    }
    if batch.edits.len() > MAX_ACTIONS_PER_BATCH {
        return Err(ValidationError::BatchTooLarge {
            count: batch.edits.len(),
            max: MAX_ACTIONS_PER_BATCH,
        });
    }
    for edit in &batch.edits {
        validate_edit(edit, sandbox_root)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// JSON extraction helper
// ---------------------------------------------------------------------------

/// Attempt to extract the first valid JSON object from a raw LLM response.
///
/// Handles the common failure mode where the model wraps JSON in markdown
/// fences or adds preamble text. Tries in order:
/// 1. Direct parse of the full string
/// 2. Strip ```json ... ``` fences, parse inner content
/// 3. Find first `{` and last `}`, parse that substring
pub fn extract_json<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    let trimmed = raw.trim();

    // Attempt 1: direct parse
    if let Ok(val) = serde_json::from_str::<T>(trimmed) {
        return Ok(val);
    }

    // Attempt 2: strip markdown fences
    let stripped = strip_markdown_fences(trimmed);
    if let Ok(val) = serde_json::from_str::<T>(&stripped) {
        return Ok(val);
    }

    // Attempt 3: find first { and last }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            let slice = &trimmed[start..=end];
            if let Ok(val) = serde_json::from_str::<T>(slice) {
                return Ok(val);
            }
        }
    }

    Err(format!(
        "Failed to extract JSON from LLM response. Raw (first 200 chars): {}",
        safe_preview(trimmed, 200)
    ))
}
/// Strip markdown code fences from a string.
fn strip_markdown_fences(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().collect();

    // Remove leading fence
    if let Some(first) = lines.first() {
        let trimmed_first = first.trim();
        if trimmed_first.starts_with("```") {
            lines.remove(0);
        }
    }

    // Remove trailing fence
    if let Some(last) = lines.last() {
        let trimmed_last = last.trim();
        if trimmed_last == "```" {
            lines.pop();
        }
    }

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Poor-man's lexical path normalization (no filesystem access).
/// Resolves `.` and collapses redundant separators.
fn normalize_lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            std::path::Component::CurDir => {} // skip `.`
            other => out.push(other),
        }
    }
    out
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

    fn sandbox() -> PathBuf {
        PathBuf::from("/sandbox/myproject")
    }

    // -- Path validation --------------------------------------------------

    #[test]
    fn valid_relative_path() {
        let result = validate_path("src/main.rs", &sandbox());
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/sandbox/myproject/src/main.rs")
        );
    }

    #[test]
    fn reject_empty_path() {
        let result = validate_path("", &sandbox());
        assert_eq!(result.unwrap_err(), ValidationError::EmptyPath);
    }

    #[test]
    fn reject_absolute_path() {
        let result = validate_path("/etc/passwd", &sandbox());
        assert!(matches!(result.unwrap_err(), ValidationError::AbsolutePath { .. }));
    }

    #[test]
    fn reject_traversal() {
        let result = validate_path("../escape/file.rs", &sandbox());
        assert!(matches!(result.unwrap_err(), ValidationError::PathTraversal { .. }));
    }

    #[test]
    fn reject_sneaky_traversal() {
        let result = validate_path("src/../../escape.rs", &sandbox());
        assert!(matches!(result.unwrap_err(), ValidationError::PathTraversal { .. }));
    }

    #[test]
    fn allow_dotfile() {
        // `.gitignore` is fine — it's not `..`
        let result = validate_path(".gitignore", &sandbox());
        assert!(result.is_ok());
    }

    #[test]
    fn allow_nested_path() {
        let result = validate_path("src/deeply/nested/module.rs", &sandbox());
        assert!(result.is_ok());
    }

    // -- Range validation -------------------------------------------------

    #[test]
    fn valid_range() {
        assert!(validate_range("f.rs", 1, 10).is_ok());
    }

    #[test]
    fn single_line_range() {
        assert!(validate_range("f.rs", 5, 5).is_ok());
    }

    #[test]
    fn reject_zero_start() {
        assert!(matches!(
            validate_range("f.rs", 0, 5).unwrap_err(),
            ValidationError::ZeroLineIndex { .. }
        ));
    }

    #[test]
    fn reject_inverted_range() {
        assert!(matches!(
            validate_range("f.rs", 10, 5).unwrap_err(),
            ValidationError::InvalidRange { .. }
        ));
    }

    // -- Content size validation ------------------------------------------

    #[test]
    fn content_within_limit() {
        let content = "a".repeat(1024);
        assert!(validate_content_size("f.rs", &content).is_ok());
    }

    #[test]
    fn content_exceeds_limit() {
        let content = "a".repeat(MAX_CONTENT_BYTES + 1);
        assert!(matches!(
            validate_content_size("f.rs", &content).unwrap_err(),
            ValidationError::ContentTooLarge { .. }
        ));
    }

    // -- JSON deserialization ---------------------------------------------

    #[test]
    fn deserialize_write_file() {
        let json = r#"{
            "edits": [
                {
                    "action": "write_file",
                    "path": "src/main.rs",
                    "content": "fn main() {}"
                }
            ]
        }"#;

        let batch: EditBatch = serde_json::from_str(json).unwrap();
        assert_eq!(batch.edits.len(), 1);
        assert!(matches!(&batch.edits[0], EditAction::WriteFile { path, .. } if path == "src/main.rs"));
    }

    #[test]
    fn deserialize_replace_range() {
        let json = r#"{
            "edits": [
                {
                    "action": "replace_range",
                    "path": "src/lib.rs",
                    "start_line": 5,
                    "end_line": 10,
                    "content": "// replaced"
                }
            ]
        }"#;

        let batch: EditBatch = serde_json::from_str(json).unwrap();
        assert_eq!(batch.edits.len(), 1);
        assert!(matches!(&batch.edits[0], EditAction::ReplaceRange { start_line: 5, end_line: 10, .. }));
    }

    #[test]
    fn deserialize_mixed_batch() {
        let json = r#"{
            "edits": [
                { "action": "write_file", "path": "a.rs", "content": "aaa" },
                { "action": "replace_range", "path": "b.rs", "start_line": 1, "end_line": 3, "content": "bbb" }
            ]
        }"#;

        let batch: EditBatch = serde_json::from_str(json).unwrap();
        assert_eq!(batch.edits.len(), 2);
    }

    #[test]
    fn reject_unknown_action_tag() {
        let json = r#"{
            "edits": [
                { "action": "delete_file", "path": "evil.rs" }
            ]
        }"#;

        let result = serde_json::from_str::<EditBatch>(json);
        assert!(result.is_err());
    }

    // -- Batch validation -------------------------------------------------

    #[test]
    fn valid_batch() {
        let batch = EditBatch {
            edits: vec![EditAction::WriteFile {
                path: "src/main.rs".into(),
                content: "fn main() {}".into(),
            }],
        };
        assert!(validate_batch(&batch, &sandbox()).is_ok());
    }

    #[test]
    fn reject_empty_batch() {
        let batch = EditBatch { edits: vec![] };
        assert_eq!(
            validate_batch(&batch, &sandbox()).unwrap_err(),
            ValidationError::EmptyBatch
        );
    }

    #[test]
    fn reject_oversized_batch() {
        let edits = (0..MAX_ACTIONS_PER_BATCH + 1)
            .map(|i| EditAction::WriteFile {
                path: format!("file_{i}.rs"),
                content: String::new(),
            })
            .collect();
        let batch = EditBatch { edits };
        assert!(matches!(
            validate_batch(&batch, &sandbox()).unwrap_err(),
            ValidationError::BatchTooLarge { .. }
        ));
    }

    #[test]
    fn batch_catches_bad_path() {
        let batch = EditBatch {
            edits: vec![EditAction::WriteFile {
                path: "../escape.rs".into(),
                content: "bad".into(),
            }],
        };
        assert!(matches!(
            validate_batch(&batch, &sandbox()).unwrap_err(),
            ValidationError::PathTraversal { .. }
        ));
    }

    // -- JSON extraction --------------------------------------------------

    #[test]
    fn extract_clean_json() {
        let raw = r#"{"edits":[{"action":"write_file","path":"a.rs","content":"x"}]}"#;
        assert!(extract_json::<EditBatch>(raw).is_ok());
    }

    #[test]
    fn extract_from_markdown_fences() {
        let raw = r#"```json
{"edits":[{"action":"write_file","path":"a.rs","content":"x"}]}
```"#;
        assert!(extract_json::<EditBatch>(raw).is_ok());
    }

    #[test]
    fn extract_from_preamble_garbage() {
        let raw = r#"Here is the edit plan:
{"edits":[{"action":"write_file","path":"a.rs","content":"x"}]}
Hope that helps!"#;
        assert!(extract_json::<EditBatch>(raw).is_ok());
    }

    #[test]
    fn extract_fails_on_total_garbage() {
        let raw = "I don't know how to do that, sorry!";
        assert!(extract_json::<EditBatch>(raw).is_err());
    }
}
