# Tod Operator Runbook

This runbook is for day-to-day `tod` operation decisions, not implementation details.

## Mode Decision Matrix

| Scenario | Recommended command/flags | Why |
|---|---|---|
| Fast local iteration on a small change | `tod run --max-iters 3 "<goal>"` | Default mode is faster (`build` + `test`) and usually enough for tight loops. |
| CI-like quality gate required before accepting edits | `tod run --strict --max-iters 3 "<goal>"` | Strict mode enforces `fmt` + `clippy -D warnings` + `test`. |
| Preview plan/edits without mutating files | `tod run --dry-run "<goal>"` | Produces logs and validates flow while leaving workspace unchanged. |
| Complex multi-file refactor | `tod run --strict --max-iters 8 --max-tokens 120000 "<goal>"` | More retries plus a global budget reduces unbounded runs on hard tasks. |
| Continue interrupted run after crash/stop | `tod resume` | Reuses checkpointed state/profile and continues remaining work. |
| Continue despite intentional local drift | `tod resume --force` | Overrides fingerprint mismatch when operator accepts drift risk. |
| Keep logs quiet during scripted operation | `tod run --quiet "<goal>"` or `tod resume --quiet` | Suppresses lifecycle progress chatter while preserving warnings/errors and stdout output. |

## Cap Tuning Guidance

- `--max-iters N` sets per-step retry cap.
- Total run cap is derived as `max_total_iterations = max_iters * 5`.
- `--max-tokens N` caps cumulative `input + output` tokens across planning and edits.
- `--max-tokens 0` disables token capping.

Recommended starting points:
- Small bugfix: `--max-iters 3`
- Complex refactor: `--max-iters 8 --max-tokens 120000`

Practical rule:
- Raise `--max-iters` when retries are improving output.
- Add/lower `--max-tokens` when you need predictable LLM spend.

## Resume and `--force`

- `tod resume` loads `.tod/state.json` and compares current workspace fingerprint to checkpoint fingerprint.
- Fingerprint mismatch means files changed since checkpoint (content-aware v2 in current runs).
- Without `--force`, mismatch aborts resume to prevent replaying against unexpected workspace state.
- `--force` bypasses mismatch protection and resumes anyway.

`--force` risk:
- Edits may be applied against a drifted codebase, which can cause invalid patches, unexpected behavior, or misleading attempt history.
- Prefer `--force` only when drift is understood and intentional.

## Failure Recovery Decision Tree

1. `plan_error`
   - Action: narrow/rephrase goal; rerun with clearer acceptance criteria.
   - If persistent: reduce scope and retry from a smaller sub-goal.
2. `cap_reached`
   - Action: rerun with higher `--max-iters` (for example `3 -> 6` or `8`) or split task into smaller goals.
3. `token_cap`
   - Action: rerun with higher `--max-tokens` or tighter scope to reduce context size.
4. `aborted`
   - Action: inspect latest attempt log under `.tod/logs/<run_id>/`; fix root compile/test issue manually or refine goal and rerun.
5. `edit_error`
   - Action: rerun first; if repeated, simplify goal wording and constrain changed files.
6. `apply_error`
   - Action: inspect target paths/ranges in logs; reconcile local file drift, then rerun (or `resume --force` only if drift is intentional).

Actionable runtime errors:
- `run`/`resume` failures include operator guidance in the error text.
- CLI failures print `tod: logs at .tod/logs/<run_id>/` when checkpoint context is available.
- Pre-allocation failures (for example planner/context failures before run identity is available) fall back to `tod: logs at .tod/logs/`.

## Machine-Readable Output

Use JSON mode for scripting and dashboards:
- `tod status --json` prints exactly one JSON object on stdout.
- `tod stats --json` prints exactly one JSON object on stdout.
- Human progress/warnings/errors remain on stderr; do not parse stderr as data.
- `--quiet` only suppresses cosmetic lifecycle progress lines; it does not suppress stderr errors.

`tod status --json` contract fields:
- `run_id`, `goal`, `outcome`, `terminal_message`
- `steps_completed`, `steps_aborted`, `total_attempts`
- `attempts_per_step`, `failure_stages`
- `input_tokens`, `output_tokens`, `total_tokens`
- `llm_requests_total`, `llm_requests_plan`, `llm_requests_edit`

`tod stats --json` contract fields:
- `runs_total`, `runs_succeeded`, `runs_aborted`, `runs_cap_reached`
- `runs_token_cap`, `runs_edit_error`, `runs_apply_error`, `runs_plan_error`
- `avg_attempts`, `avg_tokens`, `most_common_failure_stage`

Outcome values are stable:
- `success`, `aborted`, `cap_reached`, `token_cap`, `edit_error`, `apply_error`, `plan_error`

Compatibility notes:
- Legacy runs without `final.json` are still summarized via fallback heuristics.
- Planner failures (`plan_error`) can be summarized even when `plan.json` is absent.
