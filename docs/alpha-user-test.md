# Tod Alpha User Test Plan

Date: 2026-03-12

## Purpose

This plan is the operational route for alpha validation.

The goal is not only to answer "did Tod finish?" but also:
1. did it stay within its safety and output contracts,
2. did it choose a sensible scope on a real Rust repo,
3. can an operator explain the run from the CLI output and `.tod` artifacts without rerunning it.

Use this plan for all alpha runs until Phase 19 closes.

---

## Entry Conditions

Before starting an alpha run:
- Use a known Tod commit and record it in the report.
- Run on a disposable branch or worktree.
- Record the target repo commit SHA before the run.
- Keep the entire `.tod/` directory after the run, especially on failure.
- Prefer local repos without secrets in tracked files or logs that cannot be shared.

If a run mutates the workspace in a surprising way:
- stop the test,
- preserve the repo state and `.tod/` artifacts,
- file the report before cleaning anything up.

---

## Test Route

Run the tracks in order. Do not skip failure/recovery tracks just because happy-path runs succeed.

| Track | Repo profile | Command pattern | Primary question |
|---|---|---|---|
| A | Small clean Rust crate | `tod run "<goal>"` | Does the basic end-to-end loop feel correct and legible? |
| B | Medium repo with existing tests | `tod run --strict "<goal>"` | Does Tod stay precise enough under the stricter pipeline? |
| C | Same repo with uncommitted changes | `tod run "<goal>"` and `tod run --dry-run "<goal>"` | Are dirty-workspace and dry-run behaviors clear and non-destructive? |
| D | Forced failure or ambiguous goal | `tod run --max-iters N --max-tokens M "<goal>"` | Are failure output, logs, and accounting truthful and actionable? |
| E | Interrupted or drifted run | `tod resume`, then `tod resume --force` when appropriate | Are resume and fingerprint behaviors understandable and safe? |

---

## Track Guidance

## Track A: Small clean crate smoke test

Suggested goals:
- add or tighten one unit test,
- fix a small lint,
- rename a small internal API with tests updated.

What to verify:
- startup and lifecycle messaging are understandable when not using `--quiet`,
- final output matches the actual result,
- `cargo` pipeline behavior is consistent with the chosen mode,
- `.tod/logs/<run_id>/` is easy to find and inspect.

## Track B: Medium repo strict-mode validation

Suggested repos:
- multiple modules,
- existing formatting and lint setup,
- enough file volume to make context selection non-trivial.

What to verify:
- plans are not overly broad,
- edit scopes are plausible relative to the goal,
- strict-mode failures are surfaced clearly,
- retry behavior feels bounded rather than noisy.

## Track C: Dirty workspace and dry-run behavior

Operations:
- create a harmless uncommitted change in the target repo,
- run a normal mutable command,
- run a `--dry-run` command with the same or similar goal.

What to verify:
- dirty-workspace warning appears only for mutable runs,
- dry-run does not mutate the repo,
- stdout remains clean and operator messaging stays on stderr.

## Track D: Failure-path truthfulness

Suggested ways to induce failure:
- use a vague or contradictory goal,
- set a low `--max-iters`,
- set a low `--max-tokens`,
- choose a repo state likely to fail `cargo test` or `cargo clippy`.

What to verify:
- error output explains what happened,
- failure output points to the correct log location,
- `final.json` outcome matches the observed run failure,
- request and token accounting look plausible for the path taken.

## Track E: Resume and drift handling

Operations:
- interrupt a run after artifacts exist, then use `tod resume`,
- change the workspace before resume to trigger fingerprint behavior,
- use `--force` only when the report is specifically about forced resume behavior.

What to verify:
- resume messaging is understandable,
- fingerprint mismatch behavior is conservative by default,
- `--force` is obviously an override, not a silent mode.

---

## Operator Rules During Alpha

- Prefer visible lifecycle output on manual exploratory runs. Use `--quiet` only for automation-oriented contract checks.
- Record the exact command line used, including flags.
- Do not delete failed logs.
- Do not rerun immediately after a confusing failure without first capturing the report.
- If a run appears to succeed but the diff is low-quality, treat that as a failure of precision, not a success.
- If a run appears to fail but the logs do not clearly explain why, treat that as an observability failure.

---

## What To Capture For Every Run

- Tod commit SHA
- Target repo name and commit SHA
- Approximate repo size:
  - small: single crate / narrow tree
  - medium: multiple modules/crates
  - large: broad tree or multiple crates with substantial context pressure
- Goal text
- Exact command
- Outcome:
  - success
  - aborted
  - cap reached
  - token cap
  - plan error
  - edit/apply error
- Exit code if relevant
- Run ID and log directory
- Steps completed and total iterations
- Token and request counts if present
- Whether repo changes were acceptable
- Whether the run was understandable from output plus logs

---

## Report Template

Use one report per run.

```md
## Alpha Run Report

- Date:
- Operator:
- Tod commit:
- Target repo:
- Target repo commit:
- Repo size:
- Goal:
- Command:
- Outcome:
- Exit code:
- Run ID:
- Log directory:
- Steps completed / total iterations:
- Tokens in / out:
- Requests:
- Files changed:
- Result quality:
- Could you explain the run from output + logs alone?:
- Main issue category:
- Severity:
- Reproduction notes:
- Attachments:
```

---

## Issue Categories

Use exactly one primary category per report, even if secondary issues exist.

- `safety`: path handling, rollback, destructive behavior, unexpected mutation
- `precision`: wrong files, overly broad edits, poor plan scope, low-quality diff
- `observability`: logs missing, misleading output, unclear failure path, bad accounting
- `output-contract`: stdout/stderr pollution, JSON shape drift, scripting breakage
- `performance`: obviously slow or wasteful behavior
- `ux`: operator confusion not caused by a correctness bug
- `docs`: runbook/help/reporting guidance missing or misleading

Severity scale:
- `S1`: safety or data-loss risk
- `S2`: incorrect or misleading behavior that blocks trust
- `S3`: significant friction but workable with operator effort
- `S4`: minor issue or polish gap

---

## Reporting Cadence

- File a report after every run, not after every day.
- Group reports into a daily summary only after the individual run reports exist.
- Escalate immediately on any `S1` or `S2` finding.
- Treat repeated `S3` findings in the same category as a Phase 19 prioritization signal.

---

## Alpha Exit Signal

Alpha is producing useful engineering evidence when most failed runs still allow the operator to answer:
1. what happened,
2. where it happened,
3. whether the output and accounting were truthful,
4. whether the change scope was acceptable for the goal.

If those answers require guesswork, the product needs more Phase 19 work before wider exposure.
