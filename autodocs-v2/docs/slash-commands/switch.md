---
doc_meta:
  title: /switch
  description: Switch between spawned agent sessions or return to main chat
  category: slash_command
  keywords: [switch, session, spawn, agent, parallel, monitor, sessions]
  related: [spawn, agent-swap]
  validated: 2026-06-23
  commit: f6731f45a
  status: validated
  testable_headless: false
---

# /switch

Switch between spawned agent sessions.

## Overview

The `/switch` command lets you navigate between your main conversation and any sessions created with `/spawn`. Without arguments it opens a selection menu; with an argument it switches directly by name or ID prefix.

## Usage

```
/switch
/switch <name-or-id>
/switch main
```

| Argument      | Description                          |
| ------------- | ------------------------------------ |
| (none)        | Open interactive session picker      |
| `main`        | Return to main conversation          |
| `<name>`      | Switch to session by exact name      |
| `<id-prefix>` | Switch to session by ID prefix match |

## Examples

```
/switch a1b2            # switch by ID prefix (first session whose ID starts with a1b2)
```

## Behavior

- Sessions must have completed initial setup (status is not `pending`) to appear in the picker
- The selection menu shows each session's name, status, role, and group
- When switching to a spawned session, the terminal enters an alternate screen buffer for the session view
- Switching to `main` (or an empty string) returns to the primary conversation

## Troubleshooting

### "No active sessions"

You have no spawned sessions. Use `/spawn <task>` to create one first.

### "Session not found: <name>"

No session matches the provided name or ID prefix. Use `/switch` without arguments to see available sessions.

## Related

- [/spawn](spawn.md) — Create parallel agent sessions
- [/agent](agent-swap.md) — Switch between agents
