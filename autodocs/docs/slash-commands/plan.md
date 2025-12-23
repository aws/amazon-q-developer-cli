---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /plan
  description: Switch to Plan agent for breaking down ideas into implementation plans
  keywords: [plan, planning, agent, breakdown]
  related: [agent-switch]
---

# /plan

Switch to Plan agent for breaking down ideas into implementation plans.

## Overview

Switches to specialized Plan agent that breaks down complex ideas into actionable implementation plans. Use Shift+Tab to return to previous agent.

## Usage

```
/plan [prompt]
```

Optional prompt sent to Plan agent immediately.

## Examples

### Example 1: Switch to Planner

```
/plan
```

Switches to Plan agent.

### Example 2: Plan with Prompt

```
/plan Build a REST API for user management
```

Switches to Plan agent and sends prompt.

## Related

- [/agent](agent-switch.md) - Switch agents
- [Planning Agent](../features/planning-agent.md) - Plan agent details

## Technical Details

**Return**: Use Shift+Tab to return to previous agent.

**Built-in**: Plan agent is built-in, always available.

## Troubleshooting

### Issue: Can't Return to Previous Agent

**Symptom**: Shift+Tab doesn't work  
**Cause**: Terminal doesn't support key binding  
**Solution**: Use `/agent swap <previous-agent-name>`

### Issue: Plan Agent Not Available

**Symptom**: Error switching to Plan agent  
**Cause**: Built-in agent missing  
**Solution**: Plan agent should always be available. Check Kiro CLI installation.
