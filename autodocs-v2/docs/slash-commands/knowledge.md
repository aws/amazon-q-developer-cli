---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: slash_command
  title: /knowledge
  description: Manage knowledge base with add, remove, show, update, clear, and cancel operations
  keywords: [knowledge, base, semantic, manage, index]
  related: [knowledge-tool, knowledge-base-settings]
---

## Overview

The `/knowledge` command manages the knowledge base. Add files or directories, remove entries, view all entries, and clear the knowledge base. The AI assistant also uses the `knowledge` tool automatically to store and retrieve information during conversations.

## Usage

```
/knowledge [subcommand]
```

Without a subcommand, defaults to `show`.

## Subcommands

### show

List all knowledge base entries with status.

```
/knowledge show
```

### add

Add a file or directory to the knowledge base.

```
/knowledge add --name <name> --path <path> [--include <pattern>] [--exclude <pattern>]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--name` | `-n` | Name for the knowledge base entry |
| `--path` | `-p` | Path to file or directory to add |
| `--include` | | Include glob patterns (e.g., `**/*.ts`). Can be repeated. |
| `--exclude` | | Exclude glob patterns (e.g., `node_modules/**`). Can be repeated. |
| `--index-type` | | Index type to use (`Fast` or `Best`) |

Paths with spaces can be quoted:
```
/knowledge add --name my-docs --path "/path/with spaces/docs"
```

### remove

Remove an entry by name or path.

```
/knowledge remove <name|path>
```

Paths with spaces can be quoted:
```
/knowledge remove "/path/with spaces/docs"
```

Alias: `/knowledge rm`

### update

Re-index one or all knowledge base entries.

```
/knowledge update [path]
```

Paths with spaces can be quoted:
```
/knowledge update "/path/with spaces/docs"
```

### clear

Clear the entire knowledge base.

```
/knowledge clear
```

### cancel

Cancel a background indexing operation.

```
/knowledge cancel [operation-id]
```

Without ID, cancels the most recent operation.

### fix

Fix knowledge base directory names after agent file path changes.

```
/knowledge fix [--apply]
```

Without `--apply`, performs a dry-run showing what would change.

## Examples

### Add documentation with include pattern

```
/knowledge add --name rust-docs --path docs/ --include "**/*.md"
```

### Show entries

```
/knowledge show
```

### Remove an entry

```
/knowledge remove rust-docs
```

## Troubleshooting

### Entry not found

Use `/knowledge show` to list all entries and verify the name or path.

### Indexing seems stuck

Use `/knowledge cancel` to cancel the current operation and try again.

## Related

- [knowledge tool](../tools/knowledge.md) — Knowledge tool used by the assistant
- [Knowledge settings](../settings/knowledge-base-settings.md) — Configuration options
- [Knowledge management](../features/knowledge-management.md) — Feature overview
