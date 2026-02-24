use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::{RunConfig, RunMode};

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

/// A minimal coding agent that edits Rust projects via LLM-generated JSON.
#[derive(Debug, Parser)]
#[command(name = "agent", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize a new project sandbox.
    Init {
        /// Name of the project directory to create.
        name: String,
    },

    /// Run the agent on a goal.
    Run {
        /// What you want the agent to build or fix.
        goal: String,

        /// Path to the target project root.
        #[arg(short, long, default_value = ".")]
        project: PathBuf,

        /// Use strict mode (fmt + clippy + test).
        #[arg(long)]
        strict: bool,

        /// Max fix iterations per plan step.
        #[arg(long, default_value_t = 5, value_parser = parse_max_iters)]
        max_iters: usize,

        /// Validate and log edits without writing to disk.
        #[arg(long)]
        dry_run: bool,
    },

    /// Resume the last interrupted run.
    Resume {
        /// Path to the target project root.
        #[arg(short, long, default_value = ".")]
        project: PathBuf,

        /// Continue even if workspace fingerprint has changed.
        #[arg(long)]
        force: bool,
    },

    /// Show the status of the last run.
    Status,

    /// Analyze run history.
    Stats {
        /// Number of recent runs to summarize.
        #[arg(long, default_value = "5")]
        last: usize,
    },
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

impl Command {
    /// Build a `RunConfig` from the `Run` variant's arguments.
    /// Returns `None` for non-Run commands.
    pub fn into_run_config(self) -> Option<(String, RunConfig)> {
        match self {
            Command::Run {
                goal,
                project,
                strict,
                max_iters,
                dry_run,
            } => {
                let config = RunConfig {
                    project_root: project,
                    mode: if strict {
                        RunMode::Strict
                    } else {
                        RunMode::Default
                    },
                    max_iterations_per_step: max_iters,
                    max_total_iterations: max_iters.saturating_mul(5),
                    dry_run,
                    ..RunConfig::default()
                };
                Some((goal, config))
            }
            _ => None,
        }
    }
}

fn parse_max_iters(raw: &str) -> Result<usize, String> {
    let parsed: usize = raw
        .parse()
        .map_err(|_| format!("invalid integer for --max-iters: {raw}"))?;
    if parsed == 0 {
        return Err("--max-iters must be >= 1".to_string());
    }
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse_from(args)
    }

    #[test]
    fn parse_run_defaults() {
        let cli = parse(&["agent", "run", "add a hello world function"]);
        match cli.command {
            Command::Run {
                goal,
                strict,
                max_iters,
                dry_run,
                ..
            } => {
                assert_eq!(goal, "add a hello world function");
                assert!(!strict);
                assert_eq!(max_iters, 5);
                assert!(!dry_run);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_run_strict_with_flags() {
        let cli = parse(&[
            "agent",
            "run",
            "--strict",
            "--max-iters",
            "10",
            "fix the bug",
        ]);
        match cli.command {
            Command::Run {
                goal,
                strict,
                max_iters,
                ..
            } => {
                assert_eq!(goal, "fix the bug");
                assert!(strict);
                assert_eq!(max_iters, 10);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_run_dry_run() {
        let cli = parse(&["agent", "run", "--dry-run", "test goal"]);
        match cli.command {
            Command::Run { dry_run, .. } => assert!(dry_run),
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn parse_init() {
        let cli = parse(&["agent", "init", "myproject"]);
        assert!(matches!(cli.command, Command::Init { name } if name == "myproject"));
    }

    #[test]
    fn parse_status() {
        let cli = parse(&["agent", "status"]);
        assert!(matches!(cli.command, Command::Status));
    }

    #[test]
    fn parse_stats_default() {
        let cli = parse(&["agent", "stats"]);
        assert!(matches!(cli.command, Command::Stats { last } if last == 5));
    }

    #[test]
    fn parse_stats_with_last() {
        let cli = parse(&["agent", "stats", "--last", "9"]);
        assert!(matches!(cli.command, Command::Stats { last } if last == 9));
    }

    #[test]
    fn run_config_conversion() {
        let cli = parse(&["agent", "run", "--strict", "--max-iters", "8", "do stuff"]);
        let (goal, config) = cli.command.into_run_config().unwrap();
        assert_eq!(goal, "do stuff");
        assert_eq!(config.mode, RunMode::Strict);
        assert_eq!(config.max_iterations_per_step, 8);
        assert_eq!(config.max_total_iterations, 40);
        assert!(!config.dry_run);
        assert_eq!(config.max_runner_output_bytes, 4096);
    }

    #[test]
    fn non_run_returns_none() {
        let cli = parse(&["agent", "status"]);
        assert!(cli.command.into_run_config().is_none());
    }

    #[test]
    fn reject_zero_max_iters() {
        let result = Cli::try_parse_from(["agent", "run", "--max-iters", "0", "goal"]);
        assert!(result.is_err());
    }
}
