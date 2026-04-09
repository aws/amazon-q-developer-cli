---
doc_meta:
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
  category: slash_command
  title: /prompts
  description: Select and execute available prompts from MCP servers and local files
  keywords: [prompts, mcp, template, reusable, select]
  related: [mcp, agent-configuration]
---

## Overview

The `/prompts` command lists available prompts from MCP servers and local/global prompt files, and lets you select one to execute. Prompts are reusable templates that can accept arguments.

## Quick Access with @

Type `@` followed by Tab to auto-complete available prompts from all sources (local, global, and MCP).

```
@<Tab>           # Shows all available prompts
@code<Tab>       # Filters to prompts matching "code"
```

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
