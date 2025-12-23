---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent show
  description: Display current agent configuration with syntax highlighting
  keywords: [agent, show, display, config]
  related: [agent-switch, agent-config]
---

# /agent show

Display current agent configuration with syntax highlighting.

## Overview

Shows complete JSON configuration of current agent with syntax highlighting. Useful for understanding agent's tools, settings, and resources.

## Usage

```
/agent show
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

- [/agent](agent-switch.md) - Switch agents
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
