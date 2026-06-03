---
doc_meta:
  validated: 2026-06-01
  commit: 598173c4d
  status: validated
  testable_headless: true
  category: tool
  title: goal
  description: Built-in tool for signaling goal completion or checking progress
  keywords: [goal, complete, iteration, loop, verification]
  related: [task, subagent]
---

## Overview

> This tool is used by the AI assistant. You don't invoke it directly.

The goal tool lets the agent signal goal completion or check progress during a `/goal` loop. It enforces a completion contract: the agent must cite concrete evidence for every success criterion.

## Commands

### complete

Mark the goal as complete. Requires a `summary` citing specific tool output as evidence.

```json
{
  "command": "complete",
  "summary": "Pagination implemented: GET /users?page=2 returns 200 with 10 items (cargo test: 4 passed, 0 failed)."
}
```

```

## Completion Contract

1. Each criterion must be verified by cited tool output
2. Belief or narrative confidence is not evidence
3. If any criterion lacks evidence, the agent must not call complete
4. When stuck, the agent stops and explains the impediment
