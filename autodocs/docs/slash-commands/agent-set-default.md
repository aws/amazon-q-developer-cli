---
doc_meta:
  validated: 2026-02-11
  commit: a9a37454
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent set-default
  description: Define a default agent to use when kiro-cli chat launches
  keywords: [agent, default, set, launch, configuration]
  related: [agent-swap, agent-list, agent-create, cmd-agent, agent-config]
---

# /agent set-default

Define a default agent to use when kiro-cli chat launches.

## Overview

The `/agent set-default` command sets a specific agent as the default for new chat sessions launched with `kiro-cli chat`.

## Usage

```
/agent set-default <NAME>
```

## Arguments

- `<NAME>` - Name of the agent to set as default (required)

## Options

- `-h, --help` - Print help

## Examples

### Example 1: Set Default Agent

```
/agent set-default python-dev
```

**Output**:
```
✔ Set python-dev as default agent
```

### Example 2: Set Built-in Default

```
/agent set-default default
```

**Output**:
```
✔ Set default as default agent
```

## Behavior

- New `kiro-cli chat` sessions will start with the specified agent
- Current session is not affected by this change
- Setting persists across application restarts
- Agent must exist in available agents

## Agent Resolution

Validates agent exists in:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory
3. **Built-in**: Default system agents

## Related Commands

- [/agent list](agent-list.md) - List available agents
- [/agent](agent-swap.md) - Switch current session agent
- [kiro-cli chat](../commands/chat.md) - Start chat with default agent
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**Storage**: Default agent preference saved in user configuration.

**Validation**: Ensures specified agent exists before setting as default.