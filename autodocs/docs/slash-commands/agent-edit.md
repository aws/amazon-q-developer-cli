---
doc_meta:
  validated: 2026-02-05
  commit: adc1a97a
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent edit
  description: Edit an existing agent configuration
  keywords: [agent, edit, modify, configuration]
  related: [agent-swap, agent-list, agent-create, cmd-agent, agent-config]
---

# /agent edit

Edit an existing agent configuration.

## Overview

The `/agent edit` command opens an existing agent configuration for editing. By default, it edits the currently active agent. You can also specify a different agent by name or path.

## Usage

```
/agent edit [OPTIONS]
```

## Options

- `-n, --name <NAME>` - Name of the agent to edit (defaults to current agent)
- `--path <PATH>` - Path to the agent config file to edit
- `-h, --help` - Print help

## Examples

### Example 1: Edit Current Agent

```
/agent edit
```

Opens the currently active agent's configuration for editing.

### Example 2: Edit by Name

```
/agent edit --name python-dev
```

Opens the `python-dev` agent configuration for editing.

### Example 3: Edit by Path

```
/agent edit --path ~/.kiro/agents/my-agent.json
```

Opens the agent configuration file at the specified path.

## Agent Resolution

When using `--name`:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory

## Editor Behavior

- Opens configuration file in default system editor
- Changes are saved automatically when editor closes

## Limitations

Built-in agents (`kiro_default`, `kiro_help`, `kiro_planner`) cannot be edited. Attempting to edit a built-in agent returns an error:

```
Cannot edit built-in agent 'kiro_default'. Create a new agent with '/agent create'
```

To customize behavior, create a new agent based on your needs.

## Related Commands

- [/agent list](agent-list.md) - List available agents
- [/agent create](agent-create.md) - Create new agent
- [/agent](agent-swap.md) - Switch to different agent
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**File Format**: Agent configurations are typically TOML files.

**Editor**: Uses system default editor or `$EDITOR` environment variable.

## Related Commands

- [/agent list](agent-list.md) - List available agents
- [/agent create](agent-create.md) - Create new agent
- [/agent](agent-swap.md) - Switch to different agent
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**File Format**: Agent configurations are JSON files.

**Editor**: Uses system default editor or `$EDITOR` environment variable.
