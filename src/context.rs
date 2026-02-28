use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::schema::validate_path;

/// Max bytes for planner context (project tree + Cargo.toml).
pub(crate) const MAX_PLANNER_CONTEXT_BYTES: usize = 128 * 1024; // 128 KiB

/// Max bytes for step file context (file contents sent to editor).
pub(crate) const MAX_STEP_CONTEXT_BYTES: usize = 64 * 1024; // 64 KiB

/// Max bytes for retry context (previous failure output appended to step context).
pub(crate) const MAX_RETRY_CONTEXT_BYTES: usize = 8 * 1024; // 8 KiB

/// Max directory depth to recurse when building project context.
pub(crate) const MAX_TREE_DEPTH: usize = 12;

/// Max files to list in planner context.
pub(crate) const MAX_LISTED_FILES: usize = 200;

#[derive(Debug)]
pub enum ContextError {
    Io {
        path: PathBuf,
        kind: io::ErrorKind,
        message: String,
    },
    InvalidPath {
        step_index: usize,
        path: PathBuf,
        reason: String,
    },
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io {
                path,
                kind: _kind,
                message,
            } => {
                write!(f, "I/O error for {}: {message}", path.display())
            }
            Self::InvalidPath {
                step_index,
                path,
                reason,
            } => write!(
                f,
                "invalid plan path at step {} ({}): {reason}",
                step_index + 1,
                path.display()
            ),
        }
    }
}

impl std::error::Error for ContextError {}

pub fn build_planner_context(project_root: &Path) -> Result<String, ContextError> {
    let mut files = Vec::new();
    collect_paths(project_root, project_root, &mut files, 0)?;
    files.sort();

    let mut out = String::from("Project file tree:\n");
    for file in files.into_iter().take(MAX_LISTED_FILES) {
        out.push_str("- ");
        out.push_str(&file);
        out.push('\n');
    }
    let tree_section_end = out.len();

    // Cargo.toml is high-signal context for the planner.
    let cargo_path = project_root.join("Cargo.toml");
    if let Ok(contents) = fs::read_to_string(&cargo_path) {
        out.push_str("\n---\nCargo.toml:\n");
        out.push_str(&truncate_context(&contents, 8 * 1024));
    }

    if out.len() <= MAX_PLANNER_CONTEXT_BYTES {
        return Ok(out);
    }

    let mut end = snap_to_char_boundary(&out, MAX_PLANNER_CONTEXT_BYTES);
    if let Some(newline_idx) = out[..end].rfind('\n') {
        end = newline_idx + 1;
    }
    // If truncation falls inside Cargo.toml context, snap to last file entry.
    if end > tree_section_end {
        end = tree_section_end;
    }
    if end == 0 {
        end = snap_to_char_boundary(&out, MAX_PLANNER_CONTEXT_BYTES);
    }

    let omitted = out.len().saturating_sub(end);
    let kept = out[..end].trim_end_matches('\n');
    Ok(format!(
        "{kept}\n\n... [context truncated, {omitted} bytes omitted] ..."
    ))
}

pub fn build_step_context(
    project_root: &Path,
    files: &[String],
    step_index: usize,
) -> Result<String, ContextError> {
    let mut rendered: Vec<(String, String)> = Vec::with_capacity(files.len());
    for rel in files {
        let full = validate_path(rel, project_root).map_err(|e| ContextError::InvalidPath {
            step_index,
            path: PathBuf::from(rel),
            reason: e.to_string(),
        })?;

        let mut entry = match fs::read_to_string(&full) {
            Ok(content) => format_file_context(rel, &content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                format!("=== {rel} ===\n<missing file>\n")
            }
            Err(e) => {
                return Err(ContextError::Io {
                    path: full.clone(),
                    kind: e.kind(),
                    message: e.to_string(),
                });
            }
        };
        entry.push('\n');
        rendered.push((rel.clone(), entry));
    }

    let full_size: usize = rendered.iter().map(|(_, entry)| entry.len()).sum();
    if full_size <= MAX_STEP_CONTEXT_BYTES {
        let mut out = String::with_capacity(full_size);
        for (_, entry) in rendered {
            out.push_str(&entry);
        }
        return Ok(out);
    }

    // Fit content + omission note into a fixed budget. Iterating is simpler
    // than trying to solve for note length up front.
    let mut note = String::new();
    let mut out = String::new();
    for _ in 0..4 {
        let content_budget = MAX_STEP_CONTEXT_BYTES.saturating_sub(note.len());
        let fit = fit_entries(&rendered, content_budget);
        let omitted = omitted_files(&rendered, &fit);
        let next_note = if omitted.is_empty() {
            String::new()
        } else {
            omitted_note(&omitted)
        };

        out = fit.content;
        if next_note == note {
            note = next_note;
            break;
        }
        note = next_note;
    }

    out.push_str(&note);
    if out.len() > MAX_STEP_CONTEXT_BYTES {
        let end = snap_to_char_boundary(&out, MAX_STEP_CONTEXT_BYTES);
        out.truncate(end);
    }
    Ok(out)
}

pub fn build_retry_context(error_output: &str) -> String {
    const HEADER: &str = "## Previous runner failure\n";
    if HEADER.len() >= MAX_RETRY_CONTEXT_BYTES {
        let end = snap_to_char_boundary(HEADER, MAX_RETRY_CONTEXT_BYTES);
        return HEADER[..end].to_string();
    }

    let body_budget = MAX_RETRY_CONTEXT_BYTES - HEADER.len();
    let body = truncate_line_snapped(error_output, body_budget);
    format!("{HEADER}{body}")
}

/// Truncate a string for context inclusion, snapping to a UTF-8 boundary.
pub fn truncate_context(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let kept = &s[..end];
    format!("{kept}\n\n... [truncated {} bytes] ...", s.len() - end)
}

/// Format file contents with numbered lines for the LLM.
pub fn format_file_context(path: &str, content: &str) -> String {
    let mut out = format!("=== {path} ===\n");
    for (i, line) in content.lines().enumerate() {
        out.push_str(&format!("{:4} | {line}\n", i + 1));
    }
    out
}

fn collect_paths(
    root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
    depth: usize,
) -> Result<(), ContextError> {
    if depth > MAX_TREE_DEPTH {
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| ContextError::Io {
        path: dir.to_path_buf(),
        kind: e.kind(),
        message: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ContextError::Io {
            path: dir.to_path_buf(),
            kind: e.kind(),
            message: e.to_string(),
        })?;
        let path = entry.path();
        let ty = entry.file_type().map_err(|e| ContextError::Io {
            path: path.clone(),
            kind: e.kind(),
            message: e.to_string(),
        })?;

        if ty.is_dir() {
            let name = entry.file_name();
            if name == ".git" || name == "target" || name == ".tod" {
                continue;
            }
            collect_paths(root, &path, out, depth + 1)?;
        } else if ty.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| ContextError::Io {
                    path: path.clone(),
                    kind: io::ErrorKind::InvalidData,
                    message: e.to_string(),
                })?
                .to_string_lossy()
                .to_string();
            out.push(rel);
        }
    }

    Ok(())
}

#[derive(Debug)]
struct FitResult {
    content: String,
    cut_index: Option<usize>,
    cut_was_truncated: bool,
}

fn fit_entries(rendered: &[(String, String)], budget: usize) -> FitResult {
    let mut content = String::new();
    let mut cut_index = None;
    let mut cut_was_truncated = false;

    for (idx, (_, entry)) in rendered.iter().enumerate() {
        if content.len() + entry.len() <= budget {
            content.push_str(entry);
            continue;
        }

        cut_index = Some(idx);
        let remaining = budget.saturating_sub(content.len());
        if remaining > 0 {
            let truncated = truncate_line_snapped(entry, remaining);
            cut_was_truncated = truncated.len() < entry.len();
            content.push_str(&truncated);
        }
        break;
    }

    FitResult {
        content,
        cut_index,
        cut_was_truncated,
    }
}

fn omitted_files(rendered: &[(String, String)], fit: &FitResult) -> Vec<String> {
    let Some(idx) = fit.cut_index else {
        return Vec::new();
    };

    let mut omitted = Vec::new();
    if fit.cut_was_truncated {
        omitted.push(format!("{} (truncated)", rendered[idx].0));
    } else {
        omitted.push(rendered[idx].0.clone());
    }
    omitted.extend(rendered[idx + 1..].iter().map(|(path, _)| path.clone()));
    omitted
}

fn omitted_note(omitted_files: &[String]) -> String {
    let names = omitted_files.join(", ");
    format!("\n... [context truncated, omitted files: {names}] ...")
}

fn truncate_line_snapped(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }

    let mut end = snap_to_char_boundary(s, max_bytes);
    if let Some(newline_idx) = s[..end].rfind('\n') {
        end = newline_idx + 1;
    }

    s[..end].to_string()
}

fn snap_to_char_boundary(s: &str, max_bytes: usize) -> usize {
    let mut end = max_bytes.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempSandbox;

    #[test]
    fn planner_context_within_budget() {
        let sandbox = TempSandbox::new();
        fs::create_dir_all(sandbox.join("src")).unwrap();
        fs::write(
            sandbox.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(sandbox.join("src/main.rs"), "fn main() {}\n").unwrap();

        let ctx = build_planner_context(&sandbox).unwrap();
        assert!(ctx.len() <= MAX_PLANNER_CONTEXT_BYTES);
        assert!(ctx.contains("src/main.rs"));
    }

    #[test]
    fn planner_context_truncates_large_tree() {
        let sandbox = TempSandbox::new();
        let mut deep_dir = sandbox.join("src");
        for depth in 0..8 {
            let segment = format!("d{depth}_{}", "a".repeat(170));
            deep_dir = deep_dir.join(segment);
        }
        fs::create_dir_all(&deep_dir).unwrap();
        for i in 0..320 {
            fs::write(deep_dir.join(format!("file_{i:03}.rs")), "fn x() {}\n").unwrap();
        }
        fs::write(
            sandbox.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let ctx = build_planner_context(&sandbox).unwrap();
        assert!(ctx.contains("... [context truncated, "));
        assert!(ctx.contains("bytes omitted] ..."));
    }

    #[test]
    fn step_context_within_budget() {
        let sandbox = TempSandbox::new();
        fs::create_dir_all(sandbox.join("src")).unwrap();
        fs::write(sandbox.join("src/a.rs"), "fn a() {}\n").unwrap();
        fs::write(sandbox.join("src/b.rs"), "fn b() {}\n").unwrap();

        let files = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let ctx = build_step_context(&sandbox, &files, 0).unwrap();
        assert!(ctx.len() <= MAX_STEP_CONTEXT_BYTES);
        assert!(ctx.contains("=== src/a.rs ==="));
        assert!(ctx.contains("=== src/b.rs ==="));
    }

    #[test]
    fn step_context_truncates_large_files() {
        let sandbox = TempSandbox::new();
        fs::create_dir_all(sandbox.join("src")).unwrap();
        fs::write(
            sandbox.join("src/large.rs"),
            format!("{}\n", "line\n".repeat(20 * 1024)),
        )
        .unwrap();
        fs::write(sandbox.join("src/small.rs"), "fn small() {}\n").unwrap();

        let files = vec!["src/large.rs".to_string(), "src/small.rs".to_string()];
        let ctx = build_step_context(&sandbox, &files, 1).unwrap();

        assert!(ctx.len() <= MAX_STEP_CONTEXT_BYTES);
        assert!(ctx.contains("omitted files:"));
        assert!(ctx.contains("src/small.rs"));
    }

    #[test]
    fn retry_context_truncates() {
        let err = "line\n".repeat(16 * 1024 / 5);
        let ctx = build_retry_context(&err);
        assert!(ctx.len() <= MAX_RETRY_CONTEXT_BYTES);
        assert!(ctx.ends_with('\n'));
    }

    #[test]
    fn retry_context_prefixes_header() {
        let ctx = build_retry_context("oops");
        assert!(ctx.starts_with("## Previous runner failure\n"));
    }

    #[test]
    fn format_file_context_numbers_lines() {
        let result = format_file_context("src/main.rs", "fn main() {\n    println!(\"hi\");\n}");
        assert!(result.starts_with("=== src/main.rs ===\n"));
        assert!(result.contains("   1 | fn main() {"));
        assert!(result.contains("   2 |     println!(\"hi\");"));
        assert!(result.contains("   3 | }"));
    }

    #[test]
    fn collect_paths_excludes_hidden_dirs() {
        let sandbox = TempSandbox::new();
        fs::create_dir_all(sandbox.join("src")).unwrap();
        fs::create_dir_all(sandbox.join(".git/hooks")).unwrap();
        fs::create_dir_all(sandbox.join("target/debug")).unwrap();
        fs::create_dir_all(sandbox.join(".tod/logs")).unwrap();
        fs::write(sandbox.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(sandbox.join(".git/config"), "x").unwrap();
        fs::write(sandbox.join("target/debug/foo"), "x").unwrap();
        fs::write(sandbox.join(".tod/state.json"), "{}").unwrap();

        let mut files = Vec::new();
        collect_paths(&sandbox, &sandbox, &mut files, 0).unwrap();
        files.sort();

        assert!(files.contains(&"src/main.rs".to_string()));
        assert!(!files.iter().any(|p| p.starts_with(".git/")));
        assert!(!files.iter().any(|p| p.starts_with("target/")));
        assert!(!files.iter().any(|p| p.starts_with(".tod/")));
    }

    #[test]
    fn context_error_io_display() {
        let err = ContextError::Io {
            path: PathBuf::from("src/main.rs"),
            kind: io::ErrorKind::NotFound,
            message: "no such file".to_string(),
        };

        let rendered = err.to_string();
        assert!(rendered.contains("I/O error for src/main.rs"));
        assert!(rendered.contains("no such file"));
    }
}
