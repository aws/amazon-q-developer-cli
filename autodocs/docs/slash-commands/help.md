---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: slash_command
  title: /help
  description: Display help information for all available slash commands with usage and descriptions
  keywords: [help, commands, usage, reference]
  related: [introspect, tools]
---

# /help

Display help information for all available slash commands with usage and descriptions.

## Overview

The `/help` command displays comprehensive help for all slash commands. Shows command names, descriptions, aliases, and usage information. Generated from actual command definitions.

## Usage

```
/help
```

## Output

Shows for each command:
- Command name
- Description
- Aliases (if any)
- Usage syntax
- Available subcommands

## Examples

### Example 1: View All Commands

```
/help
```

**Output**:
```
Use any of these commands to manage your Kiro session. All commands start with '/'.

Usage: /<COMMAND>

Commands:
  quit, q, exit          Quit the application
  clear                  Clear the conversation history
  agent                  Manage agents
  chat                   Manage saved conversations
  context                Manage context files and view context window usage
  ...
```

## Troubleshooting

### Issue: Help Text Truncated

**Symptom**: Not all commands shown  
**Cause**: Terminal height limitation  
**Solution**: Scroll up or pipe to less: `/help | less`

## Related Features

- [introspect](../tools/introspect.md) - Ask questions about Kiro CLI
- [/tools](tools.md) - View available tools

## Technical Details

**Generation**: Help text generated from clap command definitions

**Aliases**: Shows all command aliases (e.g., /q, /exit for /quit)

**Subcommands**: Shows available subcommands for each command
