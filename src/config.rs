use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Run mode
// ---------------------------------------------------------------------------

/// Controls the quality pipeline the runner executes after each edit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RunMode {
    /// cargo build → cargo test
    Default,
    /// cargo fmt → cargo clippy -D warnings → cargo test
    Strict,
}

pub fn run_mode_label(mode: RunMode) -> &'static str {
    match mode {
        RunMode::Default => "default",
        RunMode::Strict => "strict",
    }
}

// ---------------------------------------------------------------------------
// Run config
// ---------------------------------------------------------------------------

/// All settings for a single agent run.
///
/// Built from CLI args, then passed into the loop.
/// Immutable once constructed — the loop reads it, never mutates it.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Root directory of the target project. All paths are jailed to this.
    pub project_root: PathBuf,

    /// Default or Strict quality pipeline.
    pub mode: RunMode,

    /// Max fix iterations per plan step before giving up on that step.
    pub max_iterations_per_step: usize,

    /// Max total iterations across all steps before aborting the run.
    pub max_total_iterations: usize,

    /// If true, validate and log edits but don't write to disk or run cargo.
    pub dry_run: bool,

    /// If true, suppress cosmetic lifecycle progress messages.
    pub quiet: bool,

    /// Max bytes of runner output (compiler errors, test failures) to keep.
    /// Truncated output is snapped to the nearest line boundary.
    /// Keeps context budget sane for the fixer LLM call.
    pub max_runner_output_bytes: usize,

    /// Max total tokens (input + output) across all LLM calls. 0 = no limit.
    pub max_tokens: u64,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            mode: RunMode::Default,
            max_iterations_per_step: 5,
            max_total_iterations: 25,
            dry_run: false,
            quiet: false,
            max_runner_output_bytes: 4096,
            max_tokens: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let cfg = RunConfig::default();
        assert_eq!(cfg.mode, RunMode::Default);
        assert!(!cfg.dry_run);
        assert!(!cfg.quiet);
        assert!(cfg.max_iterations_per_step > 0);
        assert!(cfg.max_total_iterations >= cfg.max_iterations_per_step);
        assert_eq!(cfg.max_runner_output_bytes, 4096);
        assert_eq!(cfg.max_tokens, 0);
    }

    #[test]
    fn strict_mode_is_distinct() {
        assert_ne!(RunMode::Default, RunMode::Strict);
    }

    #[test]
    fn run_mode_labels_are_stable() {
        assert_eq!(run_mode_label(RunMode::Default), "default");
        assert_eq!(run_mode_label(RunMode::Strict), "strict");
    }
}
