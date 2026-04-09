---
doc_meta:
  validated: 2026-04-09
  commit: 727bdf89
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
/knowledge add <name> <path>
```

### remove

Remove an entry by name or path.

```
/knowledge remove <name|path>
```

Alias: `/knowledge rm`

### update

Re-index an existing entry.

```
/knowledge update <path>
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

## Examples

### Add documentation

```
/knowledge add rust-docs docs/
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
