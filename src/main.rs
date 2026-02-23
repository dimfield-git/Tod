mod schema;
mod config;
mod cli;
mod llm;
mod planner;
mod editor;
mod runner;
mod reviewer;
mod r#loop;

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
        Command::Resume => {
            println!("resume not implemented yet");
        }
        Command::Status => {
            println!("status not implemented yet");
        }
    }
}
