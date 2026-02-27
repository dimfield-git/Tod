use crate::runner::RunResult;

// ---------------------------------------------------------------------------
// Decision type
// ---------------------------------------------------------------------------

/// What the loop should do after the runner returns.
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewDecision {
    /// Step succeeded. Move to the next plan step.
    Proceed,
    /// Step failed but we have retries left.
    /// `error_context` is the truncated compiler/test output
    /// to feed back to the editor as fixer context.
    Retry { error_context: String },
    /// Hit the iteration cap. Stop the run.
    Abort { reason: String },
}

// ---------------------------------------------------------------------------
// Core function
// ---------------------------------------------------------------------------

/// Decide what to do after a runner result.
///
/// - `result`: what the runner returned
/// - `iteration`: current iteration count for this step (1-indexed)
/// - `max_iterations`: cap from RunConfig
pub fn review(result: &RunResult, iteration: usize, max_iterations: usize) -> ReviewDecision {
    match result {
        RunResult::Success => ReviewDecision::Proceed,
        RunResult::Failure { stage, output, .. } => {
            if iteration >= max_iterations {
                ReviewDecision::Abort {
                    reason: format!(
                        "step failed at stage '{stage}' after {iteration} iteration(s)"
                    ),
                }
            } else {
                ReviewDecision::Retry {
                    error_context: format!("Build/test failed at stage '{stage}':\n{output}"),
                }
            }
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
    fn success_returns_proceed() {
        let decision = review(&RunResult::Success, 1, 5);
        assert_eq!(decision, ReviewDecision::Proceed);
    }

    #[test]
    fn success_on_last_iteration_still_proceeds() {
        let decision = review(&RunResult::Success, 5, 5);
        assert_eq!(decision, ReviewDecision::Proceed);
    }

    #[test]
    fn failure_under_cap_returns_retry() {
        let result = RunResult::Failure {
            stage: "build".into(),
            output: "error[E0308]: mismatched types".into(),
            truncated: false,
        };
        let decision = review(&result, 1, 5);
        match decision {
            ReviewDecision::Retry { error_context } => {
                assert!(error_context.contains("build"));
                assert!(error_context.contains("mismatched types"));
            }
            other => panic!("expected Retry, got {other:?}"),
        }
    }

    #[test]
    fn failure_at_cap_returns_abort() {
        let result = RunResult::Failure {
            stage: "test".into(),
            output: "test failed".into(),
            truncated: false,
        };
        let decision = review(&result, 5, 5);
        match decision {
            ReviewDecision::Abort { reason } => {
                assert!(reason.contains("test"));
                assert!(reason.contains("5 iteration"));
            }
            other => panic!("expected Abort, got {other:?}"),
        }
    }

    #[test]
    fn failure_over_cap_returns_abort() {
        // Defensive: iteration somehow exceeds max
        let result = RunResult::Failure {
            stage: "clippy".into(),
            output: "warning promoted to error".into(),
            truncated: false,
        };
        let decision = review(&result, 6, 5);
        assert!(matches!(decision, ReviewDecision::Abort { .. }));
    }

    #[test]
    fn retry_at_penultimate_iteration() {
        let result = RunResult::Failure {
            stage: "build".into(),
            output: "undefined reference".into(),
            truncated: false,
        };
        let decision = review(&result, 4, 5);
        assert!(matches!(decision, ReviewDecision::Retry { .. }));
    }

    #[test]
    fn retry_error_context_includes_stage() {
        let result = RunResult::Failure {
            stage: "fmt".into(),
            output: "diff detected".into(),
            truncated: false,
        };
        let decision = review(&result, 1, 3);
        if let ReviewDecision::Retry { error_context } = decision {
            assert!(error_context.contains("fmt"));
            assert!(error_context.contains("diff detected"));
        } else {
            panic!("expected Retry");
        }
    }

    #[test]
    fn abort_reason_includes_stage_and_count() {
        let result = RunResult::Failure {
            stage: "build".into(),
            output: "errors".into(),
            truncated: false,
        };
        let decision = review(&result, 3, 3);
        if let ReviewDecision::Abort { reason } = decision {
            assert!(reason.contains("build"));
            assert!(reason.contains("3"));
        } else {
            panic!("expected Abort");
        }
    }

    #[test]
    fn single_iteration_cap_aborts_on_first_failure() {
        let result = RunResult::Failure {
            stage: "build".into(),
            output: "fatal".into(),
            truncated: false,
        };
        let decision = review(&result, 1, 1);
        assert!(matches!(decision, ReviewDecision::Abort { .. }));
    }
}
