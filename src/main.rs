mod cli;
mod config;
mod context;
mod editor;
mod llm;
mod r#loop;
mod loop_io;
mod planner;
mod reviewer;
mod runner;
mod schema;
mod log_schema;
mod stats;
mod util;
#[cfg(test)]
mod test_util;

use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::cli::{Cli, Command};
use crate::config::run_mode_label;
use crate::llm::AnthropicProvider;

fn read_checkpoint_log_dir(project_root: &Path) -> Option<String> {
    let state_path = project_root.join(".tod/state.json");
    let raw = fs::read_to_string(state_path).ok()?;
    let state: crate::r#loop::RunState = serde_json::from_str(&raw).ok()?;
    if state.log_dir.trim().is_empty() {
        return None;
    }
    Some(format!("{}/", state.log_dir.trim_end_matches('/')))
}

fn supports_precise_log_pointer(err: &crate::r#loop::LoopError) -> bool {
    matches!(
        err,
        crate::r#loop::LoopError::Edit { .. }
            | crate::r#loop::LoopError::Apply { .. }
            | crate::r#loop::LoopError::Aborted { .. }
            | crate::r#loop::LoopError::TotalIterationCap { .. }
            | crate::r#loop::LoopError::TokenCapExceeded { .. }
            | crate::r#loop::LoopError::InvalidPlanPath { .. }
            | crate::r#loop::LoopError::FingerprintMismatch { .. }
    )
}

fn failure_log_pointer(project_root: &Path, err: &crate::r#loop::LoopError) -> String {
    if supports_precise_log_pointer(err) {
        if let Some(pointer) = read_checkpoint_log_dir(project_root) {
            return pointer;
        }
    }
    ".tod/logs/".to_string()
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        run_cmd @ Command::Run { .. } => {
            let Some((goal, config)) = run_cmd.into_run_config() else {
                eprintln!("failed to build run configuration");
                std::process::exit(1);
            };

            let provider = match AnthropicProvider::from_env() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("failed to initialize LLM provider: {e}");
                    std::process::exit(2);
                }
            };

            if !config.quiet {
                if config.dry_run {
                    eprintln!(
                        "tod: dry-run mode on {} (no filesystem writes)",
                        config.project_root.display()
                    );
                } else {
                    let token_cap_description = if config.max_tokens == 0 {
                        "no token cap".to_string()
                    } else {
                        format!("max {} tokens", config.max_tokens)
                    };
                    eprintln!(
                        "tod: running in {} mode on {} (max {} iters/step, {})",
                        run_mode_label(config.mode),
                        config.project_root.display(),
                        config.max_iterations_per_step,
                        token_cap_description
                    );
                }
            }

            match r#loop::run(&provider, &goal, &config) {
                Ok(report) => {
                    println!(
                        "completed {} step(s) in {} iteration(s)",
                        report.steps_completed, report.total_iterations
                    );
                    if report.input_tokens > 0 || report.output_tokens > 0 {
                        println!(
                            "  tokens: {} in / {} out ({} requests)",
                            report.input_tokens, report.output_tokens, report.llm_requests
                        );
                    }
                    println!("  logs: {}/", report.log_dir);
                }
                Err(e) => {
                    eprintln!("run failed: {e}");
                    eprintln!("tod: logs at {}", failure_log_pointer(&config.project_root, &e));
                    std::process::exit(1);
                }
            }
        }
        Command::Init { name } => match init_project(&name) {
            Ok(()) => println!("initialized project: {name}"),
            Err(e) => {
                eprintln!("init failed: {e}");
                std::process::exit(1);
            }
        },
        Command::Resume {
            project,
            force,
            quiet,
        } => {
            let provider = match AnthropicProvider::from_env() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("failed to initialize LLM provider: {e}");
                    std::process::exit(2);
                }
            };

            let config = crate::config::RunConfig {
                project_root: project,
                quiet,
                ..crate::config::RunConfig::default()
            };

            match r#loop::resume(&provider, &config, force) {
                Ok(report) => {
                    println!(
                        "completed {} step(s) in {} iteration(s)",
                        report.steps_completed, report.total_iterations
                    );
                    if report.input_tokens > 0 || report.output_tokens > 0 {
                        println!(
                            "  tokens: {} in / {} out ({} requests)",
                            report.input_tokens, report.output_tokens, report.llm_requests
                        );
                    }
                    println!("  logs: {}/", report.log_dir);
                }
                Err(e) => {
                    eprintln!("resume failed: {e}");
                    eprintln!("tod: logs at {}", failure_log_pointer(&config.project_root, &e));
                    std::process::exit(1);
                }
            }
        }
        Command::Status { project, json } => match stats::summarize_current(&project) {
            Ok(summary) => {
                if json {
                    println!("{}", stats::format_run_summary_json(&summary));
                } else {
                    println!("{}", stats::format_run_summary(&summary));
                }
            }
            Err(stats::StatsError::NoData) => {
                eprintln!("no run data found (.tod/state.json missing or logs unavailable)");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Command::Stats {
            project,
            last,
            json,
        } => {
            let tod_dir = project.join(".tod");
            match stats::summarize_runs(&tod_dir, last) {
                Ok(summary) => {
                    if json {
                        println!("{}", stats::format_multi_run_summary_json(&summary));
                    } else {
                        println!("{}", stats::format_multi_run_summary(&summary));
                    }
                }
                Err(stats::StatsError::NoData) => {
                    eprintln!("no run history found at .tod/logs/");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn init_project(name: &str) -> Result<(), String> {
    let output = std::process::Command::new("cargo")
        .args(["init", name])
        .output()
        .map_err(|e| format!("failed to run cargo init: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo init failed: {stderr}"));
    }

    append_tod_gitignore(Path::new(name))
}

fn append_tod_gitignore(project_dir: &Path) -> Result<(), String> {
    let gitignore_path = project_dir.join(".gitignore");
    let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();

    if existing.lines().any(|line| line.trim() == ".tod/") {
        return Ok(());
    }

    let append = if existing.is_empty() || existing.ends_with('\n') {
        ".tod/\n"
    } else {
        "\n.tod/\n"
    };

    fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&gitignore_path)
        .and_then(|mut f| f.write_all(append.as_bytes()))
        .map_err(|e| format!("failed to update .gitignore: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::EditError;
    use crate::llm::LlmError;
    use crate::r#loop::LoopError;
    use crate::test_util::TempSandbox;
    use serde_json::json;

    fn count_tod_entries(contents: &str) -> usize {
        contents
            .lines()
            .filter(|line| line.trim() == ".tod/")
            .count()
    }

    fn write_state_with_log_dir(project_root: &Path, run_id: &str, log_dir: &str) {
        let state = json!({
            "goal": "goal",
            "plan": { "steps": [ { "description": "s", "files": ["src/main.rs"] } ] },
            "step_index": 0,
            "step_state": { "attempt": 0, "retry_context": null },
            "steps_completed": 0,
            "total_iterations": 0,
            "max_iterations_per_step": 5,
            "max_total_iterations": 25,
            "run_id": run_id,
            "log_dir": log_dir,
            "last_log_path": null,
            "fingerprint": {
                "fingerprint_version": 2,
                "file_count": 0,
                "total_bytes": 0,
                "hash": "h"
            }
        });

        let tod_dir = project_root.join(".tod");
        fs::create_dir_all(&tod_dir).unwrap();
        fs::write(
            tod_dir.join("state.json"),
            serde_json::to_string_pretty(&state).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn init_creates_project() {
        let sandbox = TempSandbox::new();
        let project_dir = sandbox.join("test_proj");
        let project = project_dir.to_str().unwrap();

        init_project(project).unwrap();

        assert!(project_dir.join("Cargo.toml").exists());
        assert!(project_dir.join("src/main.rs").exists());
    }

    #[test]
    fn init_adds_tod_to_gitignore() {
        let sandbox = TempSandbox::new();
        let project_dir = sandbox.join("test_proj_gitignore");
        let project = project_dir.to_str().unwrap();

        init_project(project).unwrap();

        let gitignore = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
        assert!(gitignore.lines().any(|line| line.trim() == ".tod/"));
    }

    #[test]
    fn init_does_not_duplicate_gitignore_entry() {
        let sandbox = TempSandbox::new();
        let project_dir = sandbox.join("test_proj_no_dup");
        let project = project_dir.to_str().unwrap();

        init_project(project).unwrap();
        append_tod_gitignore(&project_dir).unwrap();

        let gitignore = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
        assert_eq!(count_tod_entries(&gitignore), 1);
    }

    #[test]
    fn init_fails_on_existing_dir() {
        let sandbox = TempSandbox::new();
        let project_dir = sandbox.join("existing");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(
            project_dir.join("Cargo.toml"),
            "[package]\nname=\"existing\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();

        let project = project_dir.to_str().unwrap();
        let result = init_project(project);
        assert!(result.is_err());
    }

    #[test]
    fn failure_log_pointer_prefers_checkpoint_log_dir_for_terminal_error() {
        let sandbox = TempSandbox::new();
        write_state_with_log_dir(&sandbox, "run_x", ".tod/logs/run_x");
        let err = LoopError::Edit {
            step_index: 0,
            iteration: 1,
            source: EditError::Llm(LlmError::RequestFailed("x".to_string())),
        };

        let pointer = failure_log_pointer(&sandbox, &err);
        assert_eq!(pointer, ".tod/logs/run_x/");
    }

    #[test]
    fn failure_log_pointer_falls_back_for_plan_errors() {
        let sandbox = TempSandbox::new();
        write_state_with_log_dir(&sandbox, "old_run", ".tod/logs/old_run");
        let err = LoopError::Plan(crate::planner::PlanError::Llm(LlmError::RequestFailed(
            "transport".to_string(),
        )));

        let pointer = failure_log_pointer(&sandbox, &err);
        assert_eq!(pointer, ".tod/logs/");
    }

    #[test]
    fn failure_log_pointer_falls_back_without_checkpoint() {
        let sandbox = TempSandbox::new();
        let err = LoopError::TokenCapExceeded { used: 2, cap: 1 };

        let pointer = failure_log_pointer(&sandbox, &err);
        assert_eq!(pointer, ".tod/logs/");
    }
}
