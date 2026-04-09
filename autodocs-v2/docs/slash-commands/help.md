---
doc_meta:
  title: /help
  description: Show all available slash commands with descriptions and usage
  category: slash_command
  keywords: [help, commands, list, usage]
  related: [tools, guide, model, agent]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

The `/help` command displays all available slash commands with their descriptions and usage syntax.

## Usage

```
/help
```

## Examples

### Show available commands

```
/help
```

Output:

```
Available Commands:

  /agent                    Select or list available agents
    Usage: /agent [agent-name|create <name>|edit [name]|swap <name>]

  /chat                     Load a previous session or start a new one
    Usage: /chat [save [--force] <path>|load <path>|new [prompt]]

  /clear                    Clear conversation history
    Usage: /clear

  /code                     Code intelligence workspace management
    Usage: /code [status|init|logs|overview|summary]

  /compact                  Compact conversation history
    Usage: /compact

  /context                  Manage context files or show token usage
    Usage: /context [add [--force] <path>...|remove <path>...|clear]

  /feedback                 Submit feedback, request features, or report issues
    Usage: /feedback

  /guide                    Get help with Kiro CLI features from the guide agent
    Usage: /guide [question]

  /help                     Show available commands
    Usage: /help

  /hooks                    View configured hooks
    Usage: /hooks

  /knowledge                Manage knowledge base
    Usage: /knowledge [show|add <name> <path>|remove <name|path>|update <path>|clear|cancel]

  /mcp                      Show configured MCP servers
    Usage: /mcp

  /model                    Select or list available models
    Usage: /model [model-name]

  /paste                    Paste image from clipboard
    Usage: /paste

  /plan                     Switch to Plan agent for breaking down ideas into implementation plans
    Usage: /plan [prompt]

  /prompts                  Select or list available prompts
    Usage: /prompts [prompt-name]

  /quit                     Quit the application
    Usage: /quit

  /reply                    Open editor pre-filled with the last assistant message to compose a reply
    Usage: /reply

  /tools                    Show available tools
    Usage: /tools [trust-all|trust <name>|untrust <name>|reset]

  /usage                    Show billing and usage information
    Usage: /usage
```

## TUI-Only Commands

These commands are available in the TUI interface and appear in autocomplete, but are not listed in the `/help` panel:

```
  /copy                     Copy last response to clipboard (use /transcript for full conversation)
  /editor                   Open $EDITOR to compose a prompt
  /exit                     Quit the application (alias for /quit)
  /spawn                    Spawn a new agent session with a task
  /theme                    Select a theme that looks best for your terminal
  /transcript               Open conversation transcript in $PAGER (quit with q)
```

## Troubleshooting

### Command not recognized

If a slash command isn't listed in `/help`, it may be a prompt name. Use `/prompts` to see available prompts.

## Related

- [/guide](guide.md) — Get help from the guide agent
- [/tools](tools.md) — Show available tools
- [/model](model.md) — Switch models
- [/agent](agent-swap.md) — Switch agents
