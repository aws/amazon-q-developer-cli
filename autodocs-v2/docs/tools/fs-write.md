---
doc_meta:
  title: fs_write
  description: Create and modify text files with create, strReplace, and insert operations
  category: tool
  keywords: [fs_write, write, create, edit, modify, file, strReplace, insert, append, replaceAll]
  related: [fs-read, code]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The fs_write tool creates and modifies text files. It supports three commands: `create` (new file), `strReplace` (find and replace), and `insert` (insert at line or append).

## Usage

### Parameters

- `command` (string, required) — One of: `create`, `strReplace`, `insert`
- `path` (string, required) — Path to the file

#### create

Creates a new file. Parent directories are created if missing. Overwrites existing files.

- `content` (string, required) — File content

#### strReplace

Replaces text in an existing file.

- `oldStr` (string, required) — String to find in the file
- `newStr` (string, required) — Replacement string
- `replaceAll` (boolean, optional) — When true, replaces all occurrences. Default: false (expects exactly one match)

#### insert

Inserts content at a specific line or appends to end.

- `content` (string, required) — Content to insert
- `insertLine` (integer, optional) — 0-indexed line number. If omitted, appends to end of file

## Examples

### Create a new file

```json
{
  "command": "create",
  "path": "src/config.json",
  "content": "{\n  \"port\": 3000\n}"
}
```

### Replace a string

```json
{
  "command": "strReplace",
  "path": "src/main.rs",
  "oldStr": "fn old_name()",
  "newStr": "fn new_name()"
}
```

### Replace all occurrences

```json
{
  "command": "strReplace",
  "path": "src/main.rs",
  "oldStr": "v1",
  "newStr": "v2",
  "replaceAll": true
}
```

### Insert at a specific line

```json
{
  "command": "insert",
  "path": "src/lib.rs",
  "content": "use std::collections::HashMap;",
  "insertLine": 2
}
```

### Append to end of file

```json
{
  "command": "insert",
  "path": "README.md",
  "content": "\n## License\nMIT"
}
```

## Troubleshooting

### "old_str was not found in the file"

The exact string wasn't found. Check for whitespace differences, line endings, or encoding issues. The match is exact and case-sensitive.

### "X occurrences of old_str were found when only 1 is expected"

Multiple matches found but `replaceAll` is false (default). Either make `oldStr` more specific to match exactly one location, or set `replaceAll: true`.

### "Path must not be empty"

The `path` parameter is missing or empty.

### "The provided path must exist"

`strReplace` and `insert` require the file to already exist. Use `create` for new files.

## Related

- [fs_read](fs-read.md) — Read files and directories
- [code](code.md) — LSP-powered code intelligence
