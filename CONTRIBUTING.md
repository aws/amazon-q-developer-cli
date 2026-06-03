# Contributing to Kiro CLI

Thank you for your interest in contributing to Kiro CLI. We firmly believe we are stronger together — 2,000 builders pushing this forward will always build something better than a team could alone. This document explains how to contribute effectively.

## Table of Contents

- [What's Accepted Today](#whats-accepted-today)
- [Before You Start](#before-you-start)
- [Contribution Workflow](#contribution-workflow)
- [Finding Work](#finding-work)
- [Pull Request Guidelines](#pull-request-guidelines)
- [Changelog Fragments](#changelog-fragments)
- [Test Coverage](#test-coverage)
- [Review Process](#review-process)
- [Proposals (Non-Trivial Changes)](#proposals-non-trivial-changes)
- [Community Reviewers](#community-reviewers)
- [Development Setup](#development-setup)
- [Style Guides](#style-guides)
- [Reporting Bugs](#reporting-bugs)
- [Security Issues](#security-issues)
- [Getting Help](#getting-help)
- [Code of Conduct](#code-of-conduct)

## What's Accepted Today

| Area | Status | Notes |
|------|--------|-------|
| **TUI / TypeScript** (`packages/tui/`) | ✅ Open | Actively reviewed |
| **Server internals / Rust** | ⏸️ Paused | V3 migration in progress — see [announcement](https://github.com/kiro-team/kiro-cli/discussions/XXX) |
| **Documentation** | ✅ Open | Always welcome |
| **Bug reports** | ✅ Open | Use the issue template |

Rust/server PRs will not be merged while the V3 migration is underway. We will announce in [#kiro-cli-contributors](https://amzn-aws.slack.com/archives/C0911UTU5LJ) when server contributions reopen.

## Before You Start

**File an issue before opening a PR.** Every PR should link to a Taskei issue so we can confirm alignment before you write code. This prevents wasted effort on both sides.

### Use the Contribution Agent

The fastest way to get started is the **contribute agent** built into this repo. If you have Kiro CLI installed, run:

```bash
kiro --agent contribute
```

The agent will:
- Help you classify your issue (bug, feature, or contribution)
- Check for duplicates against existing tickets and PRs
- Create the right ticket in the right place
- Tell you whether you can PR directly or need team sign-off first

### Manual workflow

If you prefer not to use the agent:

1. Check [existing issues](https://github.com/kiro-team/kiro-cli/issues) for duplicates
2. Create a [Taskei issue](https://taskei.amazon.dev/tasks/create?template=8283add4-5d01-45c9-8007-fc1cef12f838) describing the problem or feature
3. Wait for acknowledgment on non-trivial changes (see [Proposals](#proposals-non-trivial-changes))
4. Then open your PR

For small fixes (typos, one-line bugs, doc corrections), you may skip the issue and go straight to a PR.

## Contribution Workflow

```
1. File issue on Taskei (or find an existing one)
2. Fork the repository
3. Create a feature branch from `main`
4. Make your changes (focused, single-purpose commits)
5. Ensure tests pass locally
6. Open a PR linking to the issue
7. Respond to automated checks and reviewer feedback
```

## Finding Work

- **Slack**: Join [#kiro-cli-contributors](https://amzn-aws.slack.com/archives/C0911UTU5LJ) for updates, issue discovery, and support
- **Labels**: Look for issues tagged `help-wanted` or `good-first-issue`
- **Office Hours**: Weekly sessions starting 6/1 — check the Slack channel for schedule

## Pull Request Guidelines

- **One concern per PR.** Don't mix refactors with features.
- **Link your issue.** Include `Resolves #123` or a Taskei link in the PR description.
- **Keep it focused.** If you also reformat surrounding code, it's harder to review.
- **Write tests.** New features need tests. Bug fixes need a regression test.
- **Update docs.** If your change affects user-facing behavior, update relevant documentation.
- **Follow commit conventions.** Use clear, descriptive commit messages. See [Style Guides](#style-guides).

## Review Process

We use a delegated review model:

| Step | What happens | SLA |
|------|-------------|-----|
| **Automated checks** | CI runs smoke tests, format checks, commit conventions, secrets scan | Minutes |
| **Auto-labeling** | PR is classified by complexity (`size/small`, `size/medium`, `size/large`) and review gates (`needs-ux-review`, `needs-pm-review`) | Minutes |
| **Human review** | Community Reviewers or core team reviews your code | 48 hours |
| **Merge** | Core team merges approved PRs | After approval |

If your PR has not received review within 48 hours, it automatically escalates to the core team.

Every rejection includes a written rationale explaining why and what would need to change.

## Proposals (Non-Trivial Changes)

Non-trivial changes require a lightweight proposal before code. This includes:

- New commands or subcommands
- Behavior changes to existing commands
- New dependencies
- Architectural changes

<!-- TODO: Link proposal template once available -->

The proposal process ensures alignment before significant effort is invested. Small bug fixes and documentation improvements do not need a proposal.

## Community Reviewers

Trusted contributors can earn review permissions (no merge access). Community Reviewers:

- Provide first-pass code review
- Help maintain the 48-hour SLA
- Escalate to core team when needed

Interested? Post in [#kiro-cli-contributors](https://amzn-aws.slack.com/archives/C0911UTU5LJ) or reply to the contribution model announcement.

## Development Setup

### TypeScript / TUI

```bash
cd packages/tui
bun install
bun test
```

### Rust (paused — V3 migration)

```bash
cargo +nightly fmt
cargo clippy -p chat_cli_v2
cargo test -p chat_cli_v2
```

> **Note:** Rust/server contributions are currently paused. See [What's Accepted Today](#whats-accepted-today).

### Running locally

Refer to the [README](./README.md) for full build and run instructions.

## Changelog Fragments

Every PR with user-facing changes **must** include a changelog fragment. CI will block your PR if one is missing.

Create a JSON file in `.changes/unreleased/` with this naming convention:

```
.changes/unreleased/YYYYMMDD-HHMM-<type>-<short-slug>.json
```

**Use the helper script** to automate this:

```bash
./scripts/new-change.sh <type> "Your concise, customer-facing description"
```

It generates the filename, slug, and validates the entry for you.

**Example (manual):**

```json
{
  "type": "fixed",
  "description": "Prevent fs_write strReplace from silently growing files when oldStr is a substring of newStr"
}
```

**Valid types:** `added`, `changed`, `deprecated`, `removed`, `fixed`, `security`

**Guidelines:**
- Write from the user's perspective — what changed for them, not what you did in the code
- Keep it to one sentence
- No PR with user-facing changes will be merged without a fragment

**No user-facing changes?** Add the `no-changelog` label to your PR to skip this check.

## Test Coverage

We enforce minimum coverage thresholds in CI. Your PR will fail if coverage regresses.

**Current thresholds (TUI):**
- Function coverage: ≥ 90%
- Line coverage: ≥ 90%

**What this means for your PR:**
- New features must include unit tests
- Bug fixes must include a regression test
- If your change touches existing code, ensure existing tests still pass
- Run `bun test` in `packages/tui/` locally before pushing — the output includes a coverage summary

Integration tests run on Linux, macOS, and Windows. E2E tests build the full binary and exercise real user flows. If your change affects cross-platform behavior, CI will catch regressions across all three.

## Style Guides

### TypeScript

- Follow the existing patterns in `packages/tui/`
- Run `bun run lint` before submitting

### Rust

- Run `cargo +nightly fmt` for formatting
- Run `cargo clippy -p chat_cli_v2` for lints
- No warnings allowed

### Commit Messages

We use [conventional commits](https://www.conventionalcommits.org/). CI will enforce this on PR titles.

- Format: `type: description` (e.g., `feat: add voice mode`, `fix: prevent crash on NFS mounts`)
- Valid types: `feat`, `fix`, `docs`, `chore`, `refactor`, `test`, `perf`, `ci`
- Keep the subject line under 72 characters
- Use present tense ("Add feature" not "Added feature")
- Reference the issue number in the body

## Reporting Bugs

Use the [bug report template](https://github.com/kiro-team/kiro-cli/issues/new?template=1_bug_report_template.yml) and include:

- Steps to reproduce
- Expected vs actual behavior
- OS and Kiro CLI version (`kiro --version`)
- Output of `kiro doctor` if applicable

## Security Issues

If you discover a potential security issue, **do not** create a public issue. Report it via the [AWS vulnerability reporting page](http://aws.amazon.com/security/vulnerability-reporting/).

## Getting Help

- **Slack**: [#kiro-cli-contributors](https://amzn-aws.slack.com/archives/C0911UTU5LJ)
- **Office Hours**: Weekly starting 6/1 (schedule posted in Slack)
- **Existing issues**: Search before asking — your question may already be answered

## Code of Conduct

This project follows the [Amazon Open Source Code of Conduct](https://aws.github.io/code-of-conduct). See the [FAQ](https://aws.github.io/code-of-conduct-faq) or contact opensource-codeofconduct@amazon.com.
