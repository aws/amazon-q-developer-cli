---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: false
  category: slash_command
  title: /hooks
  description: View context hooks configuration and execution status
  keywords: [hooks, context, commands, triggers, stop]
  related: [hooks-feature, agent-config]
---

# /hooks

View context hooks configuration and execution status.

## Overview

Displays hooks configured in current agent. Hooks are commands executed at specific triggers (onStart, onContextLoad, beforeToolUse, afterToolUse) to provide dynamic context.

## Usage

```
/hooks
```

## Output

Shows:
- Hook trigger points
- Commands configured
- Descriptions
- Matcher patterns

## Related

- [Hooks](../features/hooks.md) - Complete hooks guide
- [Agent Configuration](../agent-config/overview.md) - Configure hooks

## Examples

### Example 1: View Hooks

```
/hooks
```

**Output**:
```
onStart:
  Command: git status
  Description: Show git status

beforeToolUse (fs_write):
  Command: git diff
  Description: Show pending changes
```

## Troubleshooting

### Issue: No Hooks Shown

**Symptom**: Empty output  
**Cause**: No hooks configured in agent  
**Solution**: Add hooks to agent configuration

### Issue: Hook Not Executing

**Symptom**: Hook command not running  
**Cause**: Command invalid or permission issue  
**Solution**: Test command in terminal. Check it's executable.
