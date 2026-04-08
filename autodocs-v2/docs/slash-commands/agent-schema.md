---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent schema
  description: Show agent config schema
  keywords: [agent, schema, configuration, format]
  related: [agent-swap, agent-list, agent-create, agent-edit, agent-config]
---

# /agent schema

Show agent config schema.

## Overview

The `/agent schema` command displays the configuration schema for agent files, showing the structure and available fields.

## Usage

```
/agent schema
```

## Options

- `-h, --help` - Print help

## Examples

### Example 1: Display Schema

```
/agent schema
```

**Output**:
```toml
[agent]
name = "string"
description = "string"
version = "string"

[agent.system]
prompt = "string"
temperature = 0.7
max_tokens = 4096

[agent.tools]
enabled = ["tool1", "tool2"]
disabled = ["tool3"]

[agent.mcp]
servers = ["server1", "server2"]
```

## Schema Sections

- **agent** - Basic agent metadata
- **agent.system** - System prompt and model parameters
- **agent.tools** - Tool configuration and permissions
- **agent.mcp** - MCP server configuration

## Use Cases

- Reference when creating new agents
- Validate existing agent configurations
- Understand available configuration options
- Template for manual agent creation

## Related Commands

- [/agent create](agent-create.md) - Create new agent
- [/agent edit](agent-edit.md) - Edit existing agent
- [/agent list](agent-list.md) - List available agents
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**Format**: Displays TOML schema with field types and example values.

**Validation**: Use schema to ensure agent configurations are properly formatted.