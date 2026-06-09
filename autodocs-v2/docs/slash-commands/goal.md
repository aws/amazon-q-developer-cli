---
doc_meta:
  validated: 2026-06-05
  commit: 46d015fca
  status: validated
  testable_headless: true
  category: slash_command
  title: /goal
  description: Set a goal with validation criteria for iterative agent completion
  keywords: [goal, iterate, loop, validation, criteria, complete, autonomous, agent, retry, failure]
  related: [compact, spawn, effort]
---

# /goal

Set a goal for iterative autonomous completion.

## Overview

The `/goal` command starts a goal-driven loop where the agent works autonomously toward a stated objective. The agent iterates — taking actions, verifying progress, and re-attempting with different strategies — until it can prove all success criteria are met or the iteration limit is reached.

**Availability**: Internal nightly builds only (rollout-gated).

## Usage

```
/goal <description> [--max <N>]
/goal clear
```

- `description` — What you want accomplished (max 4,000 characters)
- `--max <N>` — Maximum iterations before stopping (default: 5, ceiling: 50)
- `clear` — Cancel the active goal

## Examples

```
/goal implement pagination for the /users endpoint
/goal fix all failing tests in the auth module --max 15
/goal clear
```

## Goal Status Display

When a goal starts, a system message appears in the conversation:

```
⟳ Goal: "fix all failing tests in the auth module" · 15 iterations max
```

While a goal is active, the prompt bar shows real-time status:

```
Goal Active: fix all failing tests... · Iteration 1/15 · Ctrl+C to pause
```

This lets you track progress at a glance. The description is truncated if longer than 50 characters.

## Goal States

| State | Icon | Meaning |
|-------|------|---------|
| Active | ⟳ | Agent is working toward the goal |
| Completed | ✓ | All criteria verified with cited evidence |
| Exhausted | ✗ | Max iterations reached without completion |

## Failure Handling

When a dispatch failure occurs (network error, server 5xx response), the goal automatically retries with exponential backoff:

| Failure # | Backoff Delay | Action |
|-----------|---------------|--------|
| 1 | 2 seconds | Retry |
| 2 | 4 seconds | Retry |
| 3 | — | Goal paused |

After 3 consecutive failures, the goal enters the **Exhausted** state with a message like:

```
Paused after 3 consecutive dispatch failures
```

Successful turns reset the failure counter. You can resume a paused goal by starting a new one.

## Troubleshooting

- **"No active goal"** — No goal is set. Use `/goal <description>` first.
- **"Goal description is too long"** — Keep under 4,000 characters.
- **"--max exceeds the 50 iteration ceiling"** — Use `--max 50` or lower.
- **Goal exhausted** — Increase `--max`, simplify the goal, or break into sub-goals.
- **"Paused after 3 consecutive dispatch failures"** — Network or server issues caused repeated failures. Check your connection and start a new goal to retry.
