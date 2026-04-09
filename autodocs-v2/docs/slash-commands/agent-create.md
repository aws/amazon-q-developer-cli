---
doc_meta:
  validated: 2026-04-09
  commit: 727bdf89
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent create
  description: Create a new agent configuration
  keywords: [agent, create, new, configuration, profile]
  related: [agent-swap, agent-edit, agent-configuration]
---

## Overview

The `/agent create` command creates a new agent configuration file. Optionally base it on an existing agent with `--from`, or specify a directory with `--directory`.

## Usage

```
/agent create [NAME] [--from <agent>] [--directory <path>]
```

## Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `NAME` | No | Name for the new agent. If omitted, prompted interactively |

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--from` | `-f` | Name of an existing agent to use as a template |
| `--directory` | `-d` | Directory to save the agent config. Defaults to global `~/.kiro/agents/` |

## Examples

### Create a new agent

```
/agent create my-agent
```

Creates `~/.kiro/agents/my-agent.json` with a default template, then opens it in your editor.

### Create from an existing agent

```
/agent create rust-expert --from kiro_default
```

Creates a new agent based on the default agent's configuration.

### Create in workspace directory

```
/agent create project-helper --directory .kiro/agents
```

Creates the agent in the local workspace instead of globally.

## Agent File Locations

- **Global**: `~/.kiro/agents/<name>.json`
- **Local**: `.kiro/agents/<name>.json` (workspace-specific, takes precedence)

## Related

- [/agent](agent-swap.md) - List and switch agents
- [/agent edit](agent-edit.md) - Edit an existing agent
- [Agent Configuration](../features/agent-configuration.md) - Configuration format reference
