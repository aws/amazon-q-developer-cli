---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /knowledge
  description: Manage knowledge base with add, search, remove, show, and clear operations
  keywords: [knowledge, base, search, semantic, manage]
  related: [knowledge-tool, enable-knowledge]
---

# /knowledge

Manage knowledge base with add, search, remove, show, and clear operations.

## Overview

The `/knowledge` command provides slash command interface for knowledge base management. The AI assistant also uses the `knowledge` tool automatically to store and retrieve information during conversations. Add files or text, search semantically, remove entries, view all entries, and clear knowledge base.

## Usage

```
/knowledge <subcommand>
```

## Subcommands

### show

List all knowledge base entries.

```
/knowledge show
```

Shows all entries with status and background operations.

### add

Add content to knowledge base.

```
/knowledge add --name <name> --path <path> [--include pattern] [--exclude pattern] [--index-type Fast|Best]
```

**Options**:
- `--name, -n`: Entry name (required)
- `--path, -p`: File or directory path (required)
- `--include`: Include patterns (can specify multiple)
- `--exclude`: Exclude patterns (can specify multiple)
- `--index-type`: Fast or Best (default from settings)

### remove

Remove entry by path.

```
/knowledge remove <path>
```

**Alias**: `/knowledge rm`

### update

Re-index existing entry.

```
/knowledge update <path>
```

### clear

Clear entire knowledge base.

```
/knowledge clear
```

Requires confirmation.

### cancel

Cancel background operation.

```
/knowledge cancel [operation-id]
```

Without ID, cancels most recent operation.

### fix

Fix knowledge base directory names after agent path changes.

```
/knowledge fix [--apply]
```

Default is dry-run. Use `--apply` to actually fix.

## Configuration

Enable knowledge feature:

```bash
kiro-cli settings chat.enableKnowledge true
```

## Examples

### Example 1: Add Files

```
/knowledge add --name rust-docs --path docs/
```

Prompts for additional options (include/exclude patterns, index type).

### Example 2: Show Entries

```
/knowledge show
```

Lists all stored entries with status.

### Example 3: Remove Entry

```
/knowledge remove docs/
```

Removes entry by path.

## Related Features

- [knowledge](../tools/knowledge.md) - Knowledge tool
- [chat.enableKnowledge](../settings/enable-knowledge.md) - Enable setting

## Limitations

- Experimental feature
- Requires explicit enablement
- Interactive only (not in headless mode)

## Technical Details

**Storage**: Local knowledge base in workspace

**Search**: Semantic search using embeddings

## Troubleshooting

### Issue: Feature Not Enabled

**Symptom**: Commands don't work  
**Cause**: Knowledge feature not enabled  
**Solution**: `kiro-cli settings chat.enableKnowledge true`

### Issue: Can't Find Entry

**Symptom**: Entry not in list  
**Cause**: Entry removed or never added  
**Solution**: Use `/knowledge show` to list all entries

### Issue: Search Returns Nothing

**Symptom**: No results from search  
**Cause**: Query doesn't match indexed content  
**Solution**: Try different search terms or verify content was indexed
