---
doc_meta:
  title: /spawn
  description: Spawn a new agent session with a task to run in parallel
  category: slash_command
  keywords: [spawn, session, parallel, agent, task, background]
  related: [subagent, agent-swap]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
---

## Overview

The `/spawn` command creates a new agent session that runs a task in parallel with your current conversation. Monitor spawned sessions with Ctrl+G (crew monitor).

## Usage

```
/spawn <task description>
```

```
/spawn --name <session-name> <task description>
```

## Examples

### Spawn a task

```
/spawn Analyze the test coverage in src/utils and suggest improvements
```

### Spawn with a name

```
/spawn --name test-analysis Review all failing tests and categorize the failures
```

### Monitor spawned sessions

Press `Ctrl+G` to open the crew monitor and see status of all spawned sessions.

## Troubleshooting

### "Task description is required"

You must provide a task. `/spawn` with no arguments is not valid.

## Related

- [subagent](../tools/subagent.md) — Agent-driven multi-agent orchestration
- [/agent](agent-swap.md) — Switch between agents
