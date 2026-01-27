---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: false
  category: slash_command
  title: /prompts
  description: Manage local and MCP prompts with list, get, create, edit, and remove operations
  keywords: [prompts, templates, mcp, local, global, manage, autocomplete]
  related: [agent-config, mcp]
---

# /prompts

Manage local and MCP prompts with list, get, create, edit, and remove operations.

## Overview

The `/prompts` command manages prompt templates from local files, global files, and MCP servers. Create reusable prompts, edit them, retrieve with arguments, and list available prompts from all sources.

## Quick Access with @

Type `@` followed by Tab to auto-complete available prompts from all sources (local, global, and MCP). Local and global file-based prompts are included alongside MCP prompts.

```
@<Tab>           # Shows all available prompts
@fix<Tab>        # Auto-completes prompts starting with "fix"
```

File-based prompts take precedence over MCP prompts with the same name.

## Usage

```
/prompts <subcommand>
```

## Prompt Locations

**Local (workspace)**: `.kiro/prompts/` in current directory  
**Global (user-wide)**: `~/.kiro/prompts/` in home directory  
**MCP**: From connected MCP servers

Local prompts take precedence over global with same name.

## Subcommands

### list

List available prompts.

```
/prompts list [search-word]
```

Shows prompts from local, global, and MCP sources. Optional search word filters results.

**Output**:
- Prompt name
- Description
- Required arguments (marked with *)
- Source (local/global/MCP server)

### get

Retrieve and use prompt.

```
/prompts get <name> [arguments...]
```

Loads prompt content and sends as message. Supports argument substitution.

### create

Create new local prompt.

```
/prompts create --name <name> [--content <content>] [--global]
```

**Options**:
- `--name, -n`: Prompt name (required)
- `--content`: Prompt content (if omitted, opens editor)
- `--global`: Create in global directory instead of local

### edit

Edit existing prompt.

```
/prompts edit <name> [--global]
```

Opens editor with prompt content.

**Options**:
- `--global`: Edit global prompt instead of local

### remove

Delete prompt.

```
/prompts remove <name> [--global]
```

**Options**:
- `--global`: Remove global prompt instead of local

### details

Show detailed information about prompt.

```
/prompts details <name>
```

Shows description, arguments, and source.

## Examples

### Example 1: List All Prompts

```
/prompts list
```

**Output**:
```
Local Prompts
- code-review          Review code for issues           file*, language
- api-design           Design REST API                  

Global Prompts
- debug-help           Debug assistance                 error*

@git
- commit-message       Generate commit message          changes*
```

### Example 2: Search Prompts

```
/prompts list code
```

Shows only prompts matching "code".

### Example 3: Get Prompt

```
/prompts get code-review src/main.rs rust
```

Loads code-review prompt with arguments: file=src/main.rs, language=rust.

### Example 4: Create Local Prompt

```
/prompts create --name test-helper --content "Help me write tests for this code"
```

Creates `.kiro/prompts/test-helper.md`.

### Example 5: Create with Editor

```
/prompts create --name refactor-guide
```

Opens editor to write prompt content.

### Example 6: Create Global Prompt

```
/prompts create --name sql-optimizer --global
```

Creates in `~/.kiro/prompts/`.

### Example 7: Edit Prompt

```
/prompts edit code-review
```

Opens editor with existing content.

### Example 8: Remove Prompt

```
/prompts remove old-prompt
```

Deletes local prompt.

### Example 9: View Details

```
/prompts details commit-message
```

**Output**:
```
Prompt: commit-message
Source: @git
Description: Generate commit message
Arguments: changes* (required)
```

## Prompt File Format

Prompts are markdown files (`.md`) stored in prompts directory.

**Simple prompt**:
```markdown
Review this code for potential issues and suggest improvements.
```

**With arguments** (for MCP prompts):
Arguments defined by MCP server, passed when using `/prompts get`.

## Prompt Name Rules

- Alphanumeric, hyphens, underscores only
- Max 50 characters
- No spaces or special characters
- Pattern: `^[a-zA-Z0-9_-]+$`

## Troubleshooting

### Issue: Prompt Not Found

**Symptom**: "Prompt does not exist" error  
**Cause**: Prompt name doesn't exist  
**Solution**: Use `/prompts list` to see available prompts

### Issue: Ambiguous Prompt

**Symptom**: "Prompt offered by more than one server"  
**Cause**: Multiple MCP servers provide same prompt name  
**Solution**: Use server-specific name: `@server-name/prompt-name`

### Issue: Invalid Prompt Name

**Symptom**: Error creating prompt  
**Cause**: Name contains invalid characters or too long  
**Solution**: Use only alphanumeric, hyphens, underscores. Max 50 chars.

### Issue: Can't Edit Global Prompt

**Symptom**: Edit fails  
**Cause**: Trying to edit global without --global flag  
**Solution**: Add `--global` flag: `/prompts edit name --global`

### Issue: Local Overrides Global

**Symptom**: Editing global but local version used  
**Cause**: Local prompt with same name exists  
**Solution**: Remove local prompt or edit local instead

## Related Features

- [Agent Configuration](../agent-config/overview.md) - Agent prompts
- [/mcp](mcp.md) - MCP server prompts
- [/editor](editor.md) - Compose prompts

## Limitations

- Prompt names max 50 characters
- Alphanumeric, hyphens, underscores only
- Local overrides global with same name
- MCP prompts read-only (can't edit)
- No prompt versioning
- No prompt sharing (except via file system)

## Technical Details

**File Format**: Markdown (.md)

**Storage**:
- Local: `.kiro/prompts/` in workspace
- Global: `~/.kiro/prompts/` in home directory

**Resolution**: Local checked first, then global, then MCP servers

**Editor**: Uses $EDITOR environment variable, falls back to vi

**MCP Integration**: MCP servers can provide prompts with arguments

**Arguments**: MCP prompts support argument substitution. Required arguments marked with *.
