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
use crate::llm::AnthropicProvider;

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

            match r#loop::run(&provider, &goal, &config) {
                Ok(report) => {
                    println!(
                        "completed {} step(s) in {} iteration(s)",
                        report.steps_completed, report.total_iterations
                    );
                }
                Err(e) => {
                    eprintln!("run failed: {e}");
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
        Command::Resume { project, force } => {
            let provider = match AnthropicProvider::from_env() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("failed to initialize LLM provider: {e}");
                    std::process::exit(2);
                }
            };

            let config = crate::config::RunConfig {
                project_root: project,
                ..crate::config::RunConfig::default()
            };

            match r#loop::resume(&provider, &config, force) {
                Ok(report) => {
                    println!(
                        "completed {} step(s) in {} iteration(s)",
                        report.steps_completed, report.total_iterations
                    );
                }
                Err(e) => {
                    eprintln!("resume failed: {e}");
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
    use crate::test_util::TempSandbox;

    fn count_tod_entries(contents: &str) -> usize {
        contents
            .lines()
            .filter(|line| line.trim() == ".tod/")
            .count()
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
}
