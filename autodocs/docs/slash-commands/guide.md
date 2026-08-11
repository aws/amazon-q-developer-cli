---
doc_meta:
  title: /guide
  description: Switch to the Kiro guide agent for help with CLI features
  category: slash_command
  keywords: [guide, help, kiro, agent, swap, toggle, features, documentation]
  related: [agent-swap, help]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
---

# /guide

Switch to the Kiro guide agent for help with CLI features.

## Overview

The `/guide` command switches to the `kiro_guide` agent, a specialized agent that answers questions about Kiro CLI features. Running the command again toggles back to your previous agent.

## Usage

```
/guide [question]
```

**Arguments**:
- `question` (optional): Question to ask the guide agent immediately after switching

## Behavior

- If not on guide agent: Switches to `kiro_guide`
- If already on guide agent with no question: Toggles back to previous agent
- If already on guide agent with question: Forwards question to guide agent

## Examples

### Example 1: Switch to Guide Agent

```
/guide
```

**Output**:
```
Agent changed to kiro_guide
```

### Example 2: Ask Question Directly

```
/guide how do I save my session?
```

Switches to guide agent and immediately asks the question.

### Example 3: Toggle Back

When already on the guide agent:

```
/guide
```

**Output**:
```
Agent changed to kiro_default
```

Returns to your previous agent (or `kiro_default` if none).

### Example 4: Ask Follow-up While on Guide

When already on the guide agent:

```
/guide what keyboard shortcuts are available?
```

Stays on guide agent and asks the new question.

## Troubleshooting

### Issue: Guide Agent Not Found

**Symptom**: "Guide agent not found" error  
**Cause**: Built-in agents not available  
**Solution**: This is a built-in agent that should always be available. Try restarting the CLI.

### Issue: Can't Switch Back

**Symptom**: "Previous agent not found" error  
**Cause**: Previous agent was removed or renamed  
**Solution**: Use `/agent swap` to manually select an agent.

## Related Features

- [/agent swap](agent-swap.md) - Switch to any agent
- [/help](help.md) - Show available commands

## Limitations

- Only switches to the built-in `kiro_guide` agent
- Cannot specify a different guide agent
- Previous agent must still exist to toggle back

## Technical Details

**Target Agent**: `kiro_guide` (built-in)

**Toggle Behavior**: Tracks previous agent name to enable toggling back.

**Question Forwarding**: When a question is provided, it's passed as a prompt to be processed after the agent switch.
