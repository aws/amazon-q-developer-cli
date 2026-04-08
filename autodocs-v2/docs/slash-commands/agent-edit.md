---
doc_meta:
  validated: 2026-02-17
  commit: 86087ff5
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

The `/agent edit` command opens an existing agent configuration for editing. When no name is provided, a fuzzy selector appears to choose from available editable agents. You can also specify an agent by name or path.

## Usage

```
/agent edit [NAME] [OPTIONS]
```

## Arguments

- `[NAME]` - Name of the agent to edit (optional, shows selector if omitted)

## Options

- `--path <PATH>` - Path to the agent config file to edit
- `-h, --help` - Print help

## Examples

### Example 1: Interactive Selection

```
/agent edit
```

Opens a fuzzy selector showing all editable (non-built-in) agents. Type to filter, press Enter to select, or Esc to cancel.

### Example 2: Edit by Name

```
/agent edit python-dev
```

Opens the `python-dev` agent configuration for editing.

### Example 3: Edit by Path

```
/agent edit --path ~/.kiro/agents/my-agent.json
```

Opens the agent configuration file at the specified path.

## Agent Resolution

When specifying an agent name:
1. **Local**: `.kiro/agents/` in current directory
2. **Global**: `~/.kiro/agents/` in home directory

## Editor Behavior

- Opens a temporary copy of the configuration in your default system editor
- After saving and closing the editor, the configuration is validated
- If validation succeeds, changes are saved to the original file
- If validation fails, you're prompted to continue editing or cancel

### Validation Error Handling

If your edited configuration has errors (invalid JSON, missing required fields, etc.), you'll see the error and a prompt:

```
Error: Invalid JSON in agent config: expected `,` or `}` at line 5

? What would you like to do?
> Continue editing
  Cancel
```

- **Continue editing**: Reopens the editor with your changes preserved
- **Cancel**: Discards changes, original file remains unchanged

This safe editing approach ensures you never corrupt an agent configuration file.

## Limitations

Built-in agents (`kiro_default`, `kiro_help`, `kiro_planner`) cannot be edited and are excluded from the interactive selector. Attempting to edit a built-in agent by name returns an error:

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

**File Format**: Agent configurations are JSON files.

**Editor**: Uses system default editor or `$EDITOR` environment variable.
