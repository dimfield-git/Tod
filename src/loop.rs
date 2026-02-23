use std::fs;
use std::path::Path;

use crate::config::RunConfig;
use crate::editor::{create_edits, format_file_context, EditError};
use crate::llm::LlmProvider;
use crate::planner::{create_plan, PlanError};
use crate::reviewer::{review, ReviewDecision};
use crate::runner::{apply_edits, run_pipeline, ApplyError, RunResult};
use crate::schema::validate_path;

#[derive(Debug, Clone, PartialEq)]
pub struct LoopReport {
    pub steps_completed: usize,
    pub total_iterations: usize,
}

#[derive(Debug)]
pub enum LoopError {
    Plan(PlanError),
    Edit {
        step_index: usize,
        iteration: usize,
        source: EditError,
    },
    Apply {
        step_index: usize,
        iteration: usize,
        source: ApplyError,
    },
    Io {
        path: String,
        cause: String,
    },
    InvalidPlanPath {
        step_index: usize,
        path: String,
        reason: String,
    },
    Aborted {
        step_index: usize,
        reason: String,
    },
    TotalIterationCap {
        max_total_iterations: usize,
    },
}

impl std::fmt::Display for LoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plan(e) => write!(f, "plan failed: {e}"),
            Self::Edit {
                step_index,
                iteration,
                source,
            } => write!(
                f,
                "edit creation failed (step {}, iteration {}): {source}",
                step_index + 1,
                iteration
            ),
            Self::Apply {
                step_index,
                iteration,
                source,
            } => write!(
                f,
                "edit application failed (step {}, iteration {}): {source}",
                step_index + 1,
                iteration
            ),
            Self::Io { path, cause } => write!(f, "I/O error for {path}: {cause}"),
            Self::InvalidPlanPath {
                step_index,
                path,
                reason,
            } => write!(
                f,
                "invalid plan path at step {} ({path}): {reason}",
                step_index + 1
            ),
            Self::Aborted { step_index, reason } => {
                write!(f, "run aborted at step {}: {reason}", step_index + 1)
            }
            Self::TotalIterationCap {
                max_total_iterations,
            } => write!(
                f,
                "reached total iteration cap: {max_total_iterations}"
            ),
        }
    }
}

impl std::error::Error for LoopError {}

impl From<PlanError> for LoopError {
    fn from(value: PlanError) -> Self {
        Self::Plan(value)
    }
}

pub fn run(provider: &dyn LlmProvider, goal: &str, config: &RunConfig) -> Result<LoopReport, LoopError> {
    let project_context = build_project_context(&config.project_root)?;
    let plan = create_plan(provider, goal, &project_context)?;

    let mut steps_completed = 0usize;
    let mut total_iterations = 0usize;

    for (step_index, step) in plan.steps.iter().enumerate() {
        let mut retry_context = String::new();
        let mut step_done = false;

        for iteration in 1..=config.max_iterations_per_step {
            if total_iterations >= config.max_total_iterations {
                return Err(LoopError::TotalIterationCap {
                    max_total_iterations: config.max_total_iterations,
                });
            }
            total_iterations += 1;

            let mut file_context = build_step_file_context(&config.project_root, &step.files, step_index)?;
            if !retry_context.is_empty() {
                file_context.push_str("\n## Previous runner failure\n");
                file_context.push_str(&retry_context);
            }

            let batch = create_edits(provider, step, &file_context, &config.project_root).map_err(|source| {
                LoopError::Edit {
                    step_index,
                    iteration,
                    source,
                }
            })?;

            let run_result = if config.dry_run {
                RunResult::Success
            } else {
                apply_edits(&batch, &config.project_root).map_err(|source| LoopError::Apply {
                    step_index,
                    iteration,
                    source,
                })?;
                run_pipeline(config)
            };

            match review(&run_result, iteration, config.max_iterations_per_step) {
                ReviewDecision::Proceed => {
                    step_done = true;
                    break;
                }
                ReviewDecision::Retry { error_context } => {
                    retry_context = error_context;
                }
                ReviewDecision::Abort { reason } => {
                    return Err(LoopError::Aborted { step_index, reason });
                }
            }
        }

        if !step_done {
            return Err(LoopError::Aborted {
                step_index,
                reason: "step did not reach success within per-step cap".to_string(),
            });
        }

        steps_completed += 1;
    }

    Ok(LoopReport {
        steps_completed,
        total_iterations,
    })
}

fn build_project_context(project_root: &Path) -> Result<String, LoopError> {
    let mut files = Vec::new();
    collect_paths(project_root, project_root, &mut files)?;
    files.sort();

    let mut out = String::from("Project file tree:\n");
    for file in files {
        out.push_str("- ");
        out.push_str(&file);
        out.push('\n');
    }
    Ok(out)
}

fn collect_paths(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), LoopError> {
    let entries = fs::read_dir(dir).map_err(|e| LoopError::Io {
        path: dir.display().to_string(),
        cause: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| LoopError::Io {
            path: dir.display().to_string(),
            cause: e.to_string(),
        })?;
        let path = entry.path();
        let ty = entry.file_type().map_err(|e| LoopError::Io {
            path: path.display().to_string(),
            cause: e.to_string(),
        })?;

        if ty.is_dir() {
            let name = entry.file_name();
            if name == ".git" || name == "target" {
                continue;
            }
            collect_paths(root, &path, out)?;
        } else if ty.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| LoopError::Io {
                    path: path.display().to_string(),
                    cause: e.to_string(),
                })?
                .to_string_lossy()
                .to_string();
            out.push(rel);
        }
    }

    Ok(())
}

fn build_step_file_context(
    project_root: &Path,
    files: &[String],
    step_index: usize,
) -> Result<String, LoopError> {
    let mut out = String::new();

    for rel in files {
        let full = validate_path(rel, project_root).map_err(|e| LoopError::InvalidPlanPath {
            step_index,
            path: rel.clone(),
            reason: e.to_string(),
        })?;

        match fs::read_to_string(&full) {
            Ok(content) => {
                out.push_str(&format_file_context(rel, &content));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                out.push_str(&format!("=== {rel} ===\n<missing file>\n"));
            }
            Err(e) => {
                return Err(LoopError::Io {
                    path: full.display().to_string(),
                    cause: e.to_string(),
                });
            }
        }
        out.push('\n');
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use crate::config::{RunConfig, RunMode};
    use crate::llm::{LlmError, LlmProvider};

    struct QueueProvider {
        responses: Mutex<VecDeque<String>>,
    }

    impl QueueProvider {
        fn from(responses: Vec<&str>) -> Self {
            let q = responses.into_iter().map(|s| s.to_string()).collect();
            Self {
                responses: Mutex::new(q),
            }
        }
    }

    impl LlmProvider for QueueProvider {
        fn complete(&self, _system: &str, _user: &str) -> Result<String, LlmError> {
            let mut lock = self.responses.lock().unwrap();
            lock.pop_front()
                .ok_or_else(|| LlmError::RequestFailed("no fake response queued".to_string()))
        }
    }

    #[test]
    fn dry_run_completes_plan() {
        let root = std::env::temp_dir().join(format!(
            "tod_loop_test_{}_{}",
            std::process::id(),
            1
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let provider = QueueProvider::from(vec![
            r#"{"steps":[{"description":"update main","files":["src/main.rs"]}]}"#,
            r#"{"edits":[{"action":"replace_range","path":"src/main.rs","start_line":1,"end_line":1,"content":"fn main() { println!(\"ok\"); }"}]}"#,
        ]);

        let config = RunConfig {
            project_root: root.clone(),
            mode: RunMode::Default,
            max_iterations_per_step: 2,
            max_total_iterations: 3,
            dry_run: true,
            max_runner_output_bytes: 4096,
        };

        let report = run(&provider, "update", &config).unwrap();
        assert_eq!(report.steps_completed, 1);
        assert_eq!(report.total_iterations, 1);

        let final_content = fs::read_to_string(root.join("src/main.rs")).unwrap();
        assert_eq!(final_content, "fn main() {}\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn total_iteration_cap_is_enforced() {
        let root = std::env::temp_dir().join(format!(
            "tod_loop_test_{}_{}",
            std::process::id(),
            2
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let provider = QueueProvider::from(vec![
            r#"{"steps":[{"description":"do 1","files":["src/main.rs"]},{"description":"do 2","files":["src/main.rs"]}]}"#,
            r#"{"edits":[{"action":"write_file","path":"src/main.rs","content":"fn main(){}"}]}"#,
        ]);

        let config = RunConfig {
            project_root: root.clone(),
            mode: RunMode::Default,
            max_iterations_per_step: 5,
            max_total_iterations: 1,
            dry_run: true,
            max_runner_output_bytes: 4096,
        };

        let err = run(&provider, "goal", &config).unwrap_err();
        assert!(matches!(err, LoopError::TotalIterationCap { .. }));

        let _ = fs::remove_dir_all(root);
    }
}
