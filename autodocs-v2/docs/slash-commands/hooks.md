---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: slash_command
  title: /hooks
  description: View configured hooks in a searchable panel
  keywords: [hooks, context, commands, triggers, stop, panel]
  related: [hooks, agent-configuration]
---

# /hooks

View configured hooks in a searchable panel.

## Overview

Opens a panel displaying all hooks configured for the current agent. Hooks are commands executed at specific triggers (agentSpawn, userPromptSubmit, preToolUse, postToolUse, stop) to provide dynamic context, validation, or post-processing.

## Usage

```
/hooks
```

## Output

Opens a panel showing a table with:
- **Trigger** - When the hook runs (agentSpawn, preToolUse, etc.)
- **Command** - The shell command or tool to execute
- **Matcher** - Optional pattern for tool-scoped hooks (e.g., `fs_write`, `@git/*`)

The panel supports:
- Fuzzy search to filter hooks by trigger, command, or matcher
- Scrolling for long hook lists

## Related

- [Hooks](../features/hooks.md) - Complete hooks guide
- [Agent Configuration](../features/agent-configuration.md) - Configure hooks

## Examples

### Example 1: View All Hooks

```
/hooks
```

**Panel output**:
```
/hooks · 3 hooks

Trigger          Command              Matcher
─────────────────────────────────────────────────
agentSpawn       git status           —
preToolUse       validate.sh          fs_write
postToolUse      audit.sh             *
```

### Example 2: No Hooks Configured

```
/hooks
```

**Panel output**:
```
/hooks · 0 hooks

No hooks configured
```

### Example 3: Search Within Panel

After opening the panel, type to filter:
- Type `pre` to show only preToolUse hooks
- Type `git` to find hooks with "git" in the command
- Type `fs_` to find hooks matching filesystem tools

## Troubleshooting

### Issue: No Hooks Shown

**Symptom**: Panel shows "No hooks configured"  
**Cause**: No hooks defined in current agent configuration  
**Solution**: Add hooks to your agent's JSON config file (`.kiro/agents/your-agent.json`)

### Issue: Hook Not Executing

**Symptom**: Hook appears in panel but doesn't run  
**Cause**: Command invalid or permission issue  
**Solution**: Test command in terminal. Ensure script is executable (`chmod +x`).

### Issue: Matcher Not Working

**Symptom**: Hook runs for wrong tools or not at all  
**Cause**: Incorrect matcher pattern  
**Solution**: Check matcher syntax. Use `*` for all tools, `@server/tool` for MCP tools, or exact tool names like `fs_write`.
