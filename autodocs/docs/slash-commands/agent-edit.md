---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
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

The `/agent edit` command opens an existing agent configuration for editing, either by agent name or direct file path.

## Usage

```
/agent edit [OPTIONS]
```

## Options

- `-n, --name <NAME>` - Name of the agent to edit
- `--path <PATH>` - Path to the agent config file to edit
- `-h, --help` - Print help

## Examples

### Example 1: Edit by Name

```
/agent edit --name python-dev
```

Opens the `python-dev` agent configuration for editing.

### Example 2: Edit by Path

```
/agent edit --path ~/.kiro/agents/my-agent.toml
```

Opens the agent configuration file at the specified path.

### Example 3: Interactive Selection

```
/agent edit
```

Shows picker to select agent to edit (if no options provided).

## Agent Resolution

When using `--name`:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory
3. **Built-in**: Default agents (read-only)

## Editor Behavior

- Opens configuration file in default system editor
- Changes are saved automatically when editor closes
- Built-in agents cannot be modified

## Related Commands

- [/agent list](agent-list.md) - List available agents
- [/agent create](agent-create.md) - Create new agent
- [/agent](agent-swap.md) - Switch to different agent
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**File Format**: Agent configurations are typically TOML files.

**Editor**: Uses system default editor or `$EDITOR` environment variable.