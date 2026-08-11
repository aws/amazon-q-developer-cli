---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: slash_command
  title: /hooks
  description: View configured hooks in a searchable panel with trigger, command, and matcher info
  keywords: [hooks, context, commands, triggers, stop, panel, preToolUse, postToolUse, agentSpawn]
  related: [hooks-feature, agent-configuration, tools]
---

# /hooks

View configured hooks in a searchable panel.

## Overview

Displays all hooks configured in the current agent in an interactive panel. Shows trigger type, command, and optional matcher pattern for each hook.

## Usage

```
/hooks
```

Opens a panel displaying configured hooks. No subcommands or arguments.

## Panel Display

The panel shows a table with three columns:

| Column | Description |
|--------|-------------|
| Trigger | Hook trigger point (agentSpawn, preToolUse, postToolUse, stop, userPromptSubmit) |
| Command | Shell command or tool reference (e.g., `tool:my_tool`) |
| Matcher | Tool pattern for tool-scoped hooks, or `—` if none |

### Panel Features

- **Search**: Type to filter hooks by trigger, command, or matcher
- **Scroll**: Arrow keys to scroll through long lists
- **Close**: Press Escape or `q` to close

## Examples

### Example 1: View Hooks

```
/hooks
```

**Panel Output**:
```
/hooks · 3 hooks

Trigger         Command              Matcher
─────────────────────────────────────────────
agentSpawn      git status           —
preToolUse      validate.sh          fs_write
postToolUse     audit.sh             *
```

### Example 2: No Hooks Configured

```
/hooks
```

**Panel Output**:
```
/hooks · 0 hooks

No hooks configured
```

### Example 3: Search Hooks

Type in the panel to filter:

```
/hooks
> pre
```

**Filtered Output**:
```
/hooks · 3 hooks

Trigger         Command              Matcher
─────────────────────────────────────────────
preToolUse      validate.sh          fs_write
```

## Troubleshooting

### Issue: No Hooks Shown

**Symptom**: Panel shows "No hooks configured"  
**Cause**: No hooks defined in current agent  
**Solution**: Add hooks to agent configuration file (`.kiro/agents/your-agent.json`)

### Issue: Expected Hook Missing

**Symptom**: Hook not appearing in list  
**Cause**: Hook defined in different agent  
**Solution**: Switch to correct agent with `/agent swap` or check agent config

## Related

- [Hooks System](../features/hooks.md) - Complete hooks guide
- [Agent Configuration](../features/agent-configuration.md) - Configure hooks in agents
- [/tools](tools.md) - Similar panel for viewing tools

## Limitations

- View-only; cannot add/edit/remove hooks from panel
- Shows hooks for current agent only
- Hook execution status not displayed (use `hooks.showStatus` setting)

## Technical Details

Hooks are sorted alphabetically by trigger, then by command. The panel uses fuzzy search matching across all columns.
