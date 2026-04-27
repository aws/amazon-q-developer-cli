---
doc_meta:
  validated: 2026-04-24
  commit: ddff51f6
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent
  description: Switch to different agent configuration during chat session
  keywords: [agent, switch, swap, profile, description, model]
  related: [agent-create, agent-edit, agent-configuration, model]
---

# /agent swap

Switch to different agent configuration during chat session.

## Overview

The `/agent` command lists available agents and switches between them. Use `/agent` alone to list agents, or `/agent <name>` to switch directly.

## Usage

### List Agents

```
/agent
```

Shows all available agents with current agent marked.

### Direct Switch

```
/agent <name>
```

Switches directly to named agent.

### Switch with swap Keyword

```
/agent swap <name>
```

Use `swap` to switch to agents named after subcommands (e.g., `/agent swap create` switches to an agent named "create").

Note: `/agent swap` without a name tries to switch to an agent named "swap" and will fail if none exists.

## Subcommands

### create

Create a new agent configuration.

```
/agent create [name]
```

Interactive process to create new agent with AI assistance.

### edit

Edit an existing agent configuration.

```
/agent edit [name]
```

Opens agent for editing.

## Examples

### Example 1: List Agents

```
/agent
```

**Output**:
```
→ rust-expert - Rust development with cargo and clippy
  python-dev - Python development assistant
  kiro_default - Default agent
```

Current agent marked with `→`. Shows name and description.

### Example 2: Direct Switch

```
/agent python-dev
```

**Output**:
```
Agent changed to python-dev
```

### Example 3: Switch to Agent Named After Subcommand

```
/agent swap create
```

Switches to an agent named "create" (bypasses the create subcommand).

### Example 4: Fuzzy Matching Suggestion

```
/agent pythn-dev
```

**Output**:
```
Agent 'pythn-dev' not found. Did you mean python-dev? Run /agent to browse available agents.
```

Uses Jaro-Winkler similarity (threshold 0.6) to suggest similar agent names.

## Agent Resolution

Agents loaded from:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory
3. **Built-in**: Default agents

Local agents take precedence over global.

## Troubleshooting

### Issue: Agent Not Found

**Symptom**: `Unknown agent: <name>. Run /agent to browse available agents.`  
**Cause**: Agent doesn't exist  
**Solution**: Run `/agent` to see available agents

### Issue: Typo in Agent Name

**Symptom**: `Agent '<name>' not found. Did you mean <suggestion>?`  
**Cause**: Similar agent exists  
**Solution**: Use the suggested name or run `/agent` to browse

### Issue: MCP Servers Not Loading

**Symptom**: Agent switches but MCP servers missing  
**Cause**: MCP servers need initialization  
**Solution**: MCP servers load automatically. Check `/mcp` status.

## Related Features

- [/agent create](agent-create.md) - Create a new agent
- [/agent edit](agent-edit.md) - Edit an existing agent
- [kiro-cli agent](../commands/agent.md) - Manage agents from CLI
- [Agent Configuration](../features/agent-configuration.md) - Agent format guide
- [kiro-cli chat --agent](../commands/chat.md) - Start with specific agent

## Model Behavior

When switching agents, your model selection is preserved unless the new agent explicitly specifies a model in its configuration:

- **New agent has no model configured**: Your current model selection stays active
- **New agent specifies a model**: Switches to the agent's configured model

This lets you use `/model` to pick a model and keep it across agent switches.

## Limitations

- Switching agent doesn't reload conversation history
- Context files from previous agent remain as temporary context
- Tool permissions reset to new agent's configuration
- MCP servers reconnect with new agent's configuration
