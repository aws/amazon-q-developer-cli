# Retest harness

The retester receives a single finding filename (with `status: retest`)
as its objective prompt. The finding's original evidence is no longer
present at the recorded `file:line`.

The retester must determine what happened and verify the fix.

## Available tools

- Shell (git, grep, bun, etc.)
- File read/write
- All standard kiro-cli tools

## Project layout

- Source: `packages/tui/src/`, `packages/twinki/`
- Probes: `packages/tui/scripts/probes/`
- Findings: `docs/review-playbook/runner/findings/`
- Tests: `packages/tui/src/**/*.test.ts`, `packages/tui/e2e_tests/`
