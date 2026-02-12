---
doc_meta:
  validated: 2026-02-11
  commit: 78ada5ad
  status: validated
  testable_headless: false
  category: slash_command
  title: /agent create
  description: Create a new agent with the specified name
  keywords: [agent, create, new, configuration]
  related: [agent-swap, agent-list, agent-generate, cmd-agent, agent-config]
---

# /agent create

Create a new agent with the specified name.

## Overview

The `/agent create` command creates a new agent configuration with the specified name, optionally using an existing agent as a template.

## Usage

```
/agent create <NAME> [OPTIONS]
```

## Arguments

- `<NAME>` - Name of the agent to be created (required)

## Options

- `-d, --directory <DIRECTORY>` - Directory where the agent will be saved (optional)
- `-f, --from <FROM>` - Name of existing agent to use as template (optional)
- `-h, --help` - Print help

## Examples

### Example 1: Basic Agent Creation

```
/agent create my-agent
```

Creates a new agent in the global agent directory.

### Example 2: Create in Specific Directory

```
/agent create my-agent --directory ./custom-agents
```

Creates agent in specified directory.

### Example 3: Create from Template

```
/agent create my-agent --from python-dev
```

Creates new agent using `python-dev` as template.

### Example 4: Full Options

```
/agent create my-agent --directory ./.kiro/agents --from rust-expert
```

Creates agent in local directory using template.

## Default Behavior

- **Directory**: If not specified, saves to global agent directory (`~/.kiro/agents/`)
- **Template**: If not specified, creates basic agent configuration

## Related Commands

- [/agent list](agent-list.md) - List available agents
- [/agent](agent-swap.md) - Switch to different agent
- [/agent generate](agent-generate.md) - Generate agent with AI
- [kiro-cli agent](../commands/agent.md) - CLI agent management

## Technical Details

**Storage**: Creates agent configuration file in specified or default directory.

**Template**: When using `--from`, copies configuration from existing agent as starting point.