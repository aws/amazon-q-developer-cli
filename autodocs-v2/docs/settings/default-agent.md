---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.defaultAgent
  description: Set default agent configuration for new chat sessions
  keywords: [setting, agent, default, configuration]
  related: [cmd-agent, slash-agent, agent-config]
---

# chat.defaultAgent

Set default agent configuration for new chat sessions.

## Overview

The `chat.defaultAgent` setting specifies which agent configuration to use when starting new chat sessions. Without this setting, the built-in default agent is used.

## Usage

### Set Default Agent

```bash
kiro-cli settings chat.defaultAgent rust-expert
```

### Get Current Value

```bash
kiro-cli settings chat.defaultAgent
```

### Delete Setting

```bash
kiro-cli settings --delete chat.defaultAgent
```

## Value

**Type**: String  
**Default**: None (uses built-in default agent)  
**Example**: `rust-expert`, `python-dev`, `code-reviewer`

## Agent Selection Priority

Kiro CLI selects agents in this order:

1. **Command-line flag**: `kiro-cli chat --agent my-agent`
2. **This setting**: `chat.defaultAgent`
3. **Built-in default**: `kiro_default` agent

If specified agent not found, falls back to next level with warning.

## Built-in Default Agent

When no agent specified, uses built-in `kiro_default` with:
- **Tools**: All tools (`"*"` wildcard)
- **Allowed**: Only `fs_read` auto-approved
- **Resources**: `README.md`, `KIRO.md`, `.kiro/rules/**/*.md`
- **Legacy MCP**: Enabled (`useLegacyMcpJson: true`)

## Examples

### Example 1: Set Rust Expert as Default

```bash
kiro-cli settings chat.defaultAgent rust-expert
```

All new sessions will use rust-expert agent.

### Example 2: Check Current Default

```bash
kiro-cli settings chat.defaultAgent
```

**Output**: `rust-expert`

### Example 3: Clear Default

```bash
kiro-cli settings --delete chat.defaultAgent
```

Returns to built-in default agent.

## Related

- [kiro-cli agent](../commands/agent.md) - Manage agents
- [/agent](../slash-commands/agent-switch.md) - Switch agents in session
- [Agent Configuration](../features/agent-configuration.md) - Create agents

## Troubleshooting

### Issue: Agent Not Found

**Symptom**: Error on chat start  
**Cause**: Agent doesn't exist  
**Solution**: Check agent exists with `kiro-cli agent list`

### Issue: Setting Not Applied

**Symptom**: Different agent used  
**Cause**: Agent specified in command  
**Solution**: Remove `--agent` flag to use default
