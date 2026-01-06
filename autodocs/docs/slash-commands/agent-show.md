---
doc_meta:
  validated: 2025-12-19
  commit: a1d370b5
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent show
  description: Display current agent configuration with syntax highlighting
  keywords: [agent, show, display, config]
  related: [agent-swap, agent-config]
---

# ~~~/agent show~~~

~~Display current agent configuration with syntax highlighting.~~

**Note: This command is not available. Use `/agent list` or `/agent` to view available agents.**

## Overview

~~Shows complete JSON configuration of current agent with syntax highlighting. Useful for understanding agent's tools, settings, and resources.~~

**This command does not exist.** Use the following alternatives:
- `/agent list` - List all available agents
- `/agent` - Switch to a different agent

## Usage

~~```
/agent show
```~~

**Command not available.** Use these instead:
```
/agent list    # List all agents
/agent         # Switch agents
```

## Output

Displays agent JSON with:
- Name and description
- Available tools
- Allowed tools
- Tool settings
- Resources (context files)
- Hooks
- MCP servers

## Related

- [/agent](agent-swap.md) - Switch agents
- [Agent Configuration](../agent-config/overview.md) - Config format

## Examples

### Example 1: Show Current Agent

```
/agent show
```

**Output**:
```json
{
  "name": "rust-expert",
  "description": "Rust development expert",
  "tools": ["fs_read", "fs_write", "code"],
  "allowedTools": ["fs_read", "code"],
  "resources": ["src/**/*.rs", "Cargo.toml"]
}
```

## Troubleshooting

### Issue: No Agent Shown

**Symptom**: Empty output  
**Cause**: Using default agent with no file  
**Solution**: Default agent is built-in, has no file to show

### Issue: Can't Read Config

**Symptom**: Error reading configuration  
**Cause**: Agent file corrupted or invalid  
**Solution**: Validate with `kiro-cli agent validate <name>`
