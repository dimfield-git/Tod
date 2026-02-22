use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::{RunConfig, RunMode};
use crate::schema::{EditAction, EditBatch};

// ---------------------------------------------------------------------------
// Run result
// ---------------------------------------------------------------------------

/// Outcome of running the quality pipeline after applying edits.
#[derive(Debug, Clone, PartialEq)]
pub enum RunResult {
    Success,
    Failure { stage: String, output: String },
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during edit application (before we even run cargo).
#[derive(Debug)]
pub enum ApplyError {
    /// Failed to create parent directories.
    CreateDir { path: String, cause: String },
    /// Failed to write a file.
    Write { path: String, cause: String },
    /// Failed to read a file for ReplaceRange.
    Read { path: String, cause: String },
    /// Line range exceeds actual file length.
    RangeOutOfBounds {
        path: String,
        end_line: usize,
        file_lines: usize,
    },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateDir { path, cause } => {
                write!(f, "failed to create directories for {path}: {cause}")
            }
            Self::Write { path, cause } => write!(f, "failed to write {path}: {cause}"),
            Self::Read { path, cause } => write!(f, "failed to read {path}: {cause}"),
            Self::RangeOutOfBounds {
                path,
                end_line,
                file_lines,
            } => write!(
                f,
                "line range end {end_line} exceeds file length {file_lines} in {path}"
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

// ---------------------------------------------------------------------------
// Edit application
// ---------------------------------------------------------------------------

/// Apply a validated EditBatch to disk.
///
/// `sandbox_root` is the project root — all paths in the batch are relative
/// and have already been validated by `schema::validate_batch`.
pub fn apply_edits(batch: &EditBatch, sandbox_root: &Path) -> Result<(), ApplyError> {
    for edit in &batch.edits {
        apply_single(edit, sandbox_root)?;
    }
    Ok(())
}

fn apply_single(edit: &EditAction, sandbox_root: &Path) -> Result<(), ApplyError> {
    match edit {
        EditAction::WriteFile { path, content } => {
            let full = sandbox_root.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|e| ApplyError::CreateDir {
                    path: path.clone(),
                    cause: e.to_string(),
                })?;
            }
            fs::write(&full, content).map_err(|e| ApplyError::Write {
                path: path.clone(),
                cause: e.to_string(),
            })?;
        }
        EditAction::ReplaceRange {
            path,
            start_line,
            end_line,
            content,
        } => {
            let full = sandbox_root.join(path);
            let existing = fs::read_to_string(&full).map_err(|e| ApplyError::Read {
                path: path.clone(),
                cause: e.to_string(),
            })?;

            let mut lines: Vec<&str> = existing.lines().collect();

            // Range is 1-indexed, inclusive both ends.
            // Validation already ensured start >= 1 and start <= end.
            if *end_line > lines.len() {
                return Err(ApplyError::RangeOutOfBounds {
                    path: path.clone(),
                    end_line: *end_line,
                    file_lines: lines.len(),
                });
            }

            let start_idx = start_line - 1; // 1-indexed → 0-indexed
            let end_idx = *end_line; // inclusive end → exclusive for drain

            // Remove the target range
            lines.drain(start_idx..end_idx);

            // Insert replacement lines at the same position
            let replacement_lines: Vec<&str> = content.lines().collect();
            for (i, line) in replacement_lines.iter().enumerate() {
                lines.insert(start_idx + i, line);
            }

            let result = lines.join("\n");
            // Preserve trailing newline if original had one
            let result = if existing.ends_with('\n') {
                format!("{result}\n")
            } else {
                result
            };

            fs::write(&full, result).map_err(|e| ApplyError::Write {
                path: path.clone(),
                cause: e.to_string(),
            })?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command execution
// ---------------------------------------------------------------------------

/// Run the quality pipeline for the given mode.
///
/// Returns `RunResult::Success` if all stages pass,
/// or `RunResult::Failure` at the first stage that fails.
/// Output is truncated to `config.max_runner_output_bytes`.
pub fn run_pipeline(config: &RunConfig) -> RunResult {
    let stages: Vec<(&str, Vec<&str>)> = match config.mode {
        RunMode::Default => vec![
            ("build", vec!["cargo", "build"]),
            ("test", vec!["cargo", "test"]),
        ],
        RunMode::Strict => vec![
            ("fmt", vec!["cargo", "fmt", "--all"]),
            ("clippy", vec!["cargo", "clippy", "--", "-D", "warnings"]),
            ("test", vec!["cargo", "test"]),
        ],
    };

    for (stage_name, cmd) in &stages {
        let result = Command::new(cmd[0])
            .args(&cmd[1..])
            .current_dir(&config.project_root)
            .output();

        match result {
            Ok(output) => {
                if !output.status.success() {
                    let raw = merge_output(&output.stdout, &output.stderr);
                    let truncated = truncate_output(&raw, config.max_runner_output_bytes);
                    return RunResult::Failure {
                        stage: stage_name.to_string(),
                        output: truncated,
                    };
                }
            }
            Err(e) => {
                return RunResult::Failure {
                    stage: stage_name.to_string(),
                    output: format!("failed to execute command: {e}"),
                };
            }
        }
    }

    RunResult::Success
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

/// Merge stdout and stderr into a single string.
fn merge_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut merged = String::new();
    if !stderr.is_empty() {
        merged.push_str(&String::from_utf8_lossy(stderr));
    }
    if !stdout.is_empty() {
        if !merged.is_empty() {
            merged.push('\n');
        }
        merged.push_str(&String::from_utf8_lossy(stdout));
    }
    merged
}

/// Truncate output to `max_bytes`, snapping to the nearest line boundary.
///
/// If the output fits, returns it unchanged.
/// If truncation is needed, cuts at the last newline before the cap.
fn truncate_output(raw: &str, max_bytes: usize) -> String {
    if raw.len() <= max_bytes {
        return raw.to_string();
    }

    // Find last newline at or before the cap
    let cut = match raw[..max_bytes].rfind('\n') {
        Some(pos) => pos,
        None => max_bytes, // no newline found — hard cut (shouldn't happen with compiler output)
    };

    let truncated_bytes = raw.len() - cut;
    format!(
        "{}\n\n... [truncated {} bytes] ...",
        &raw[..cut],
        truncated_bytes
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    use std::sync::atomic::{AtomicUsize, Ordering};
    static TEST_ID: AtomicUsize = AtomicUsize::new(0);

    /// Create a unique temp directory per test for isolation.
    fn temp_sandbox() -> PathBuf {
        let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "tod_test_{}_{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    // -- Edit application: WriteFile -------------------------------------

    #[test]
    fn write_file_creates_file() {
        let sandbox = temp_sandbox();
        let batch = EditBatch {
            edits: vec![EditAction::WriteFile {
                path: "hello.txt".into(),
                content: "hello world".into(),
            }],
        };

        apply_edits(&batch, &sandbox).unwrap();
        let content = fs::read_to_string(sandbox.join("hello.txt")).unwrap();
        assert_eq!(content, "hello world");
        cleanup(&sandbox);
    }

    #[test]
    fn write_file_creates_parent_dirs() {
        let sandbox = temp_sandbox();
        let batch = EditBatch {
            edits: vec![EditAction::WriteFile {
                path: "src/deeply/nested/mod.rs".into(),
                content: "pub mod nested;".into(),
            }],
        };

        apply_edits(&batch, &sandbox).unwrap();
        assert!(sandbox.join("src/deeply/nested/mod.rs").exists());
        cleanup(&sandbox);
    }

    #[test]
    fn write_file_overwrites_existing() {
        let sandbox = temp_sandbox();
        let target = sandbox.join("overwrite.txt");
        fs::write(&target, "old content").unwrap();

        let batch = EditBatch {
            edits: vec![EditAction::WriteFile {
                path: "overwrite.txt".into(),
                content: "new content".into(),
            }],
        };

        apply_edits(&batch, &sandbox).unwrap();
        assert_eq!(fs::read_to_string(target).unwrap(), "new content");
        cleanup(&sandbox);
    }

    // -- Edit application: ReplaceRange ----------------------------------

    #[test]
    fn replace_range_single_line() {
        let sandbox = temp_sandbox();
        let target = sandbox.join("lines.txt");
        fs::write(&target, "line1\nline2\nline3\n").unwrap();

        let batch = EditBatch {
            edits: vec![EditAction::ReplaceRange {
                path: "lines.txt".into(),
                start_line: 2,
                end_line: 2,
                content: "replaced".into(),
            }],
        };

        apply_edits(&batch, &sandbox).unwrap();
        let result = fs::read_to_string(target).unwrap();
        assert_eq!(result, "line1\nreplaced\nline3\n");
        cleanup(&sandbox);
    }

    #[test]
    fn replace_range_multiple_lines() {
        let sandbox = temp_sandbox();
        let target = sandbox.join("multi.txt");
        fs::write(&target, "a\nb\nc\nd\ne\n").unwrap();

        let batch = EditBatch {
            edits: vec![EditAction::ReplaceRange {
                path: "multi.txt".into(),
                start_line: 2,
                end_line: 4,
                content: "X\nY".into(),
            }],
        };

        apply_edits(&batch, &sandbox).unwrap();
        let result = fs::read_to_string(target).unwrap();
        assert_eq!(result, "a\nX\nY\ne\n");
        cleanup(&sandbox);
    }

    #[test]
    fn replace_range_out_of_bounds() {
        let sandbox = temp_sandbox();
        let target = sandbox.join("short.txt");
        fs::write(&target, "one\ntwo\n").unwrap();

        let batch = EditBatch {
            edits: vec![EditAction::ReplaceRange {
                path: "short.txt".into(),
                start_line: 1,
                end_line: 5,
                content: "nope".into(),
            }],
        };

        let result = apply_edits(&batch, &sandbox);
        assert!(matches!(result, Err(ApplyError::RangeOutOfBounds { .. })));
        cleanup(&sandbox);
    }

    #[test]
    fn replace_range_preserves_trailing_newline() {
        let sandbox = temp_sandbox();
        let target = sandbox.join("trailing.txt");
        fs::write(&target, "a\nb\nc\n").unwrap();

        let batch = EditBatch {
            edits: vec![EditAction::ReplaceRange {
                path: "trailing.txt".into(),
                start_line: 2,
                end_line: 2,
                content: "B".into(),
            }],
        };

        apply_edits(&batch, &sandbox).unwrap();
        let result = fs::read_to_string(target).unwrap();
        assert!(result.ends_with('\n'));
        assert_eq!(result, "a\nB\nc\n");
        cleanup(&sandbox);
    }

    // -- Edit application: read error ------------------------------------

    #[test]
    fn replace_on_missing_file_fails() {
        let sandbox = temp_sandbox();
        let batch = EditBatch {
            edits: vec![EditAction::ReplaceRange {
                path: "nonexistent.rs".into(),
                start_line: 1,
                end_line: 1,
                content: "nope".into(),
            }],
        };

        let result = apply_edits(&batch, &sandbox);
        assert!(matches!(result, Err(ApplyError::Read { .. })));
        cleanup(&sandbox);
    }

    // -- Truncation -------------------------------------------------------

    #[test]
    fn no_truncation_under_limit() {
        let input = "short output";
        assert_eq!(truncate_output(input, 4096), "short output");
    }

    #[test]
    fn truncation_snaps_to_line_boundary() {
        let input = "line1\nline2\nline3\nline4\nline5\n";
        // Cap at 18 bytes — "line1\nline2\nline3\n" is 18 bytes
        let result = truncate_output(input, 18);
        assert!(result.starts_with("line1\nline2\nline3"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn truncation_reports_byte_count() {
        let input = "a\n".repeat(3000); // 6000 bytes
        let result = truncate_output(&input, 4096);
        assert!(result.contains("truncated"));
        assert!(result.contains("bytes"));
    }

    #[test]
    fn exact_limit_no_truncation() {
        let input = "exactly";
        assert_eq!(truncate_output(input, 7), "exactly");
    }

    // -- Merge output -----------------------------------------------------

    #[test]
    fn merge_stderr_first() {
        let result = merge_output(b"stdout stuff", b"stderr stuff");
        assert!(result.starts_with("stderr stuff"));
        assert!(result.contains("stdout stuff"));
    }

    #[test]
    fn merge_empty_stderr() {
        let result = merge_output(b"just stdout", b"");
        assert_eq!(result, "just stdout");
    }

    #[test]
    fn merge_empty_stdout() {
        let result = merge_output(b"", b"just stderr");
        assert_eq!(result, "just stderr");
    }

    // -- Pipeline (unit-level, no real cargo) -----------------------------

    // Integration tests for run_pipeline require a real Cargo project.
    // Those belong in tests/ or behind an #[ignore] flag.
    // Here we only test the helper functions.
}
