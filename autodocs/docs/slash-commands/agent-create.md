---
doc_meta:
  validated: 2026-02-16
  commit: 36e7dac5
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent create
  description: Create a new agent with AI assistance or manual mode
  keywords: [agent, create, new, configuration, generate, ai]
  related: [agent-swap, agent-list, agent-edit, cmd-agent, agent-config]
---

# /agent create

Create a new agent with AI assistance (default) or using simple manual mode.

## Overview

The `/agent create` command creates a new agent configuration. By default, it uses AI to help construct the agent based on your description. Use the `--manual` flag for simple creation that opens an editor.

**Note:** `/agent generate` is an alias for `/agent create` and behaves identically.

## Usage

```
/agent create [NAME] [OPTIONS]
```

## Arguments

- `[NAME]` - Name of the agent (optional - will prompt if not provided)

## Options

- `-D, --description <DESC>` - Description of the agent (optional - will prompt if not provided)
- `-d, --directory <DIR>` - Directory for agent: "workspace", "global", or a custom path (optional - will prompt if not provided)
- `-m, --mcp-server <NAME>` - MCP server to include (can be used multiple times)
- `-f, --from <FROM>` - Template agent to use as starting point (implies `--manual`)
- `--manual` - Use simple creation mode (opens editor instead of AI generation)
- `-h, --help` - Print help

## Directory Values

The `--directory` option accepts special values:

| Value | Description |
|-------|-------------|
| `workspace` | Creates agent in `.kiro/agents/` in current working directory |
| `global` | Creates agent in `~/.kiro/agents/` (global directory) |
| `./path` or `/path` | Creates agent in the specified custom path |

## Examples

### Example 1: AI-Assisted Creation (Interactive)

```
/agent create
```

Starts interactive AI-assisted creation, prompting for name, description, scope, and MCP servers.

### Example 2: AI-Assisted with Pre-filled Options

```
/agent create my-agent -D "Code review specialist for Python projects"
```

Creates an agent with name and description pre-filled, prompts for remaining options.

### Example 3: AI-Assisted with MCP Servers

```
/agent create my-agent --mcp-server filesystem --mcp-server git
```

Creates an agent with specified MCP servers pre-selected.

### Example 4: Create in Workspace Directory

```
/agent create my-agent -d workspace
```

Creates agent in the local workspace `.kiro/agents/` directory.

### Example 5: Create in Global Directory

```
/agent create my-agent -d global -D "Universal assistant"
```

Creates agent in the global `~/.kiro/agents/` directory.

### Example 6: Create in Custom Path

```
/agent create my-agent -d ./custom/agents
```

Creates agent in a custom directory path.

### Example 7: Manual Mode (Simple Creation)

```
/agent create my-agent --manual
```

Opens editor for manual agent configuration (like traditional create).

### Example 8: Create from Template

```
/agent create my-agent --from python-dev
```

Creates new agent in editor using `python-dev` as a template. Note: `--from` automatically implies `--manual` mode.

### Example 9: Full AI-Assisted with All Options

```
/agent create code-reviewer -D "Reviews code for security issues" -d workspace -m filesystem -m git
```

Creates agent with all options specified, skipping interactive prompts.

## Default Behavior

- **Mode**: AI-assisted creation (prompts for values not provided via CLI)
- **Directory**: If not specified, prompts for Local vs Global selection
- **MCP Servers**: If not specified, shows selection dialog for available servers

## AI-Assisted Mode

When not using `--manual`, the command:

1. Collects agent name and description (from args or prompts)
2. Determines scope/directory (from args or prompts)
3. Selects MCP servers (from args or prompts)
4. Uses AI to generate a complete agent configuration
5. Saves the agent configuration file

## Manual Mode

When using `--manual`:

1. Requires an agent name (provided interactively or as an argument)
2. Creates a basic agent configuration file
3. Opens your default editor (`$EDITOR`) to customize
4. Validates the configuration after editing
5. Agent is immediately available

**Note:** The `--description` (`-D`) and `--mcp-server` (`-m`) flags cannot be used with `--manual` or `--from`. These flags are only available for AI-assisted creation. In manual mode, you'll configure the description and MCP servers directly in the editor.

## Related Commands

- [/agent list](agent-list.md) - List available agents
- [/agent swap](agent-swap.md) - Switch to different agent
- [/agent edit](agent-edit.md) - Edit existing agent configuration
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**Storage**: Creates agent configuration file in specified or selected directory.

**Template**: When using `--from` with `--manual`, copies configuration from existing agent as starting point.

**Validation**: Generated/edited configuration is validated before saving.

**Immediate Availability**: Agent is usable immediately after creation with `/agent swap <name>`.