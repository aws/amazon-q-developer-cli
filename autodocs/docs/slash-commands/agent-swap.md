---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent
  description: Switch to different agent configuration during chat session
  keywords: [agent, switch, swap, profile]
  related: [agent-generate, cmd-agent, agent-config]
---

# /agent swap

Switch to different agent configuration during chat session.

## Overview

The `/agent` command (also `/agent swap`) switches to a different agent configuration mid-session. Shows interactive picker to select from available agents or specify agent name directly.

## Usage

### Interactive Selection

```
/agent swap
```

Shows picker with all available agents.

### Direct Switch

```
/agent swap <name>
```

Switches directly to named agent.

**Aliases**: `/agent swap`, `/agent set`

## Subcommands

### swap (default)

Switch to different agent.

```
/agent swap [name]
```

Without name: Shows interactive picker  
With name: Switches directly

### list

List all available agents.

```
/agent list
```

Shows agents with paths and marks active agent with `*`.

### generate

Generate agent configuration using AI.

```
/agent generate
```

Interactive process to create new agent with AI assistance.

## Examples

### Example 1: Interactive Switch

```
/agent
```

**Output**:
```
Select agent:
  default
* rust-expert
  python-dev
```

### Example 2: Direct Switch

```
/agent swap python-dev
```

**Output**:
```
✔ Switched to agent: python-dev
```

### Example 3: List Agents

```
/agent list
```

**Output**:
```
* rust-expert    ~/.kiro/agents
  python-dev     ~/.kiro/agents
  default        (Built-in)
```

## Agent Resolution

Agents loaded from:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory
3. **Built-in**: Default agents

Local agents take precedence over global.

## Troubleshooting

### Issue: Agent Not Found

**Symptom**: "Agent not found" error  
**Cause**: Agent doesn't exist  
**Solution**: Use `/agent list` to see available agents

### Issue: Can't Switch Agent

**Symptom**: Switch fails  
**Cause**: Invalid agent configuration  
**Solution**: Validate agent with `kiro-cli agent validate <name>`

### Issue: MCP Servers Not Loading

**Symptom**: Agent switches but MCP servers missing  
**Cause**: MCP servers need initialization  
**Solution**: MCP servers load automatically. Check `/mcp` status.

## Related Features

- [/agent generate](agent-generate.md) - Create agent with AI
- [kiro-cli agent](../commands/agent.md) - Manage agents from CLI
- [Agent Configuration](../agent-config/overview.md) - Agent format guide
- [kiro-cli chat --agent](../commands/chat.md) - Start with specific agent

## Limitations

- Switching agent doesn't reload conversation history
- Context files from previous agent remain as temporary context
- Tool permissions reset to new agent's configuration
- MCP servers reconnect with new agent's configuration

## Technical Details

**Agent Loading**: Local (`.kiro/agents/`) checked first, then global (`~/.kiro/agents/`), then built-in.

**State Preservation**: Message history and conversation state preserved when switching agents.

**Context Handling**: Previous agent's context files added as temporary context. New agent's permanent context also loaded.

**Tool Manager**: Recreated with new agent's tool configuration and permissions.

**MCP Servers**: Disconnected from old agent, reconnected with new agent's MCP configuration.
