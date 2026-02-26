mod cli;
mod config;
mod editor;
mod llm;
mod r#loop;
mod planner;
mod reviewer;
mod runner;
mod schema;
mod stats;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::llm::AnthropicProvider;

fn main() {
    let cli = Cli::parse();

    match cli.command {
        run_cmd @ Command::Run { .. } => {
            let (goal, config) = run_cmd
                .into_run_config()
                .expect("Run command must produce run config");

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
        Command::Init { name } => {
            println!("init not implemented yet (requested project: {name})");
        }
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
        Command::Status => match stats::summarize_current(std::path::Path::new(".")) {
            Ok(summary) => println!("{}", stats::format_run_summary(&summary)),
            Err(stats::StatsError::NoData) => {
                eprintln!("no run data found (.tod/state.json missing or logs unavailable)");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        Command::Stats { last } => {
            match stats::summarize_runs(std::path::Path::new(".tod"), last) {
                Ok(summary) => println!("{}", stats::format_multi_run_summary(&summary)),
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
