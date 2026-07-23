---
doc_meta:
  validated: 2026-07-22
  commit: 293919d18
  status: validated
  testable_headless: false
  category: slash_command
  title: /prompts
  description: Select and execute available prompts from MCP servers and local files
  keywords: [prompts, mcp, template, reusable, select, at-sign, @]
  related: [mcp, agent-configuration]
---

## Overview

The `/prompts` command lists available prompts from MCP servers and local/global prompt files, and lets you select one to execute. Prompts are reusable templates that can accept arguments.

## Quick Access with @

Type `@` at the start of your input to invoke a prompt directly. The name is matched case-insensitively against all available prompts (local, global, and MCP).

```
@code-review src/main.rs   # Executes the code-review prompt with arguments
@research                   # Executes the research prompt
```

As you type after `@`, a menu filters matching prompts. You can select from the menu or simply press Enter — the typed name resolves to the prompt at submit time regardless of menu state.

```
@code            # Menu shows prompts matching "code"
@code-review     # Full name typed — Enter submits, menu or not
```

The `@` shortcut only triggers prompts when typed at the beginning of the input line. An `@` in the middle of a message is treated as a file reference.

## Usage

```
/prompts
```

Opens a selection menu showing all available prompts grouped by source.

```
/prompts <prompt-name>
```

Executes a prompt by name directly.

## Examples

### Select a prompt interactively

```
/prompts
```

Shows a selection menu with prompts from all sources (MCP servers, local `.kiro/prompts/`, global `~/.kiro/prompts/`).

### Execute a prompt directly

```
/prompts code-review
```

Runs the `code-review` prompt immediately.

### Use @ shortcut

```
@code-review src/main.rs
```

Executes the prompt with arguments.

## Prompt Sources

Prompts are discovered from:
1. MCP servers — prompts exposed by configured MCP servers
2. Local prompts — `.kiro/prompts/*.md` in the workspace
3. Global prompts — `~/.kiro/prompts/*.md`

## Troubleshooting

### Prompt not found

The prompt name may not match any available prompt. Use `/prompts` without arguments to see all available prompts.

### No prompts available

Ensure MCP servers are configured or prompt files exist in `.kiro/prompts/` or `~/.kiro/prompts/`.

## Related

- [/mcp](mcp.md) — Manage MCP servers that provide prompts
- [Agent Configuration](../features/agent-configuration.md) — Configure agents
