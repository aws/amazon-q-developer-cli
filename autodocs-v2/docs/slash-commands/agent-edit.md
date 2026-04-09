---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent edit
  description: Edit an existing agent configuration
  keywords: [agent, edit, modify, configuration]
  related: [agent-swap, agent-create, agent-configuration]
---

# /agent edit

Edit an existing agent configuration.

## Overview

The `/agent edit` command opens an existing agent configuration for editing. When no name is provided, it edits the currently active agent. You can also specify an agent by name.

## Usage

```
/agent edit [NAME]
```

## Arguments

- `[NAME]` - Name of the agent to edit (optional, defaults to current agent)

## Examples

### Example 1: Edit Current Agent

```
/agent edit
```

Opens the currently active agent's configuration for editing.

### Example 2: Edit by Name

```
/agent edit python-dev
```

Opens the `python-dev` agent configuration for editing.

## Agent Resolution

When specifying an agent name:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory

## Limitations

**Built-in agents** (`kiro_default`, `kiro_guide`, `kiro_planner`) cannot be edited. Attempting to edit one returns:

```
Cannot edit built-in agent 'kiro_default'. Create a new agent with '/agent create'
```

**Ephemeral agents** (created in-memory without a config file) cannot be edited:

```
Agent 'temp' has no config file on disk
```

To customize behavior, create a new agent with `/agent create`.

## Related Commands

- [/agent swap](agent-swap.md) - Switch to different agent
- [/agent create](agent-create.md) - Create new agent
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**File Format**: Agent configurations are JSON files.

**Editor**: Uses system default editor or `$EDITOR` environment variable.
