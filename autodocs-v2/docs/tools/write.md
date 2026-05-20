---
doc_meta:
  title: write
  description: Create and modify text files with create, strReplace, and insert operations
  category: tool
  keywords: [fs_write, write, create, edit, modify, file, strReplace, insert, append, replaceAll, trust, permission]
  related: [read, code]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The write tool (also known as `fs_write`) creates and modifies text files. It supports three commands: `create` (new file), `strReplace` (find and replace), and `insert` (insert at line or append).

**Naming**: This tool is called `write` (canonical), with `fs_write` and `fsWrite` as legacy aliases. All names work in agent configs.

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

## Permissions

Files within the current working directory are auto-approved. Files outside CWD require approval.

When approval is needed, you see granular trust options:

- **Specific paths** — Trust only the exact file being written
- **Directory** — Trust a parent directory containing the file

Selecting a trust option adds those paths to your session's allowed write paths. The trust persists for the session.

To disable granular options and use simple Yes/No prompts:

```bash
kiro-cli settings chat.disableGranularTrust true
```

## Troubleshooting

### "old_str was not found in the file"

The exact string wasn't found. Check for whitespace differences, line endings, or encoding issues. The match is exact and case-sensitive.

### "X occurrences of old_str were found when only 1 is expected"

Multiple matches found but `replaceAll` is false (default). Either make `oldStr` more specific to match exactly one location, or set `replaceAll: true`.

### "Cannot edit file: old_str is a substring of new_str"

`oldStr` appears verbatim inside `newStr`. This pattern is rejected because repeated calls would silently re-match the just-written content and grow the file on each invocation (linearly when `replaceAll` is false, exponentially when `replaceAll: true` and `oldStr` appears multiple times in `newStr`). Either include more context in `oldStr` so it no longer appears in `newStr`, or use the `insert` command at a specific line if the goal is to add text without removing anything.

**"Wrap" patterns** (e.g., wrapping `Some(value)` in `Ok(...)`, wrapping a function call with retry logic) are supported as long as `oldStr` includes enough surrounding context that the substring relationship breaks.

Rejected (too short):
```json
{
  "command": "strReplace",
  "oldStr": "compute()",
  "newStr": "retry(compute())"
}
```
Accepted (with surrounding context):
```json
{
  "command": "strReplace",
  "oldStr": "    let r = compute();",
  "newStr": "    let r = retry(compute());"
}
```
Including the leading whitespace and `let r = ` prefix means `oldStr` is no longer a substring of `newStr`, so the call is allowed. As a bonus, the extra context also disambiguates the match against other occurrences in the file.

### "Path must not be empty"

The `path` parameter is missing or empty.

### "The provided path must exist"

`strReplace` and `insert` require the file to already exist. Use `create` for new files.

## Related

- [read](read.md) — Read files and directories
- [code](code.md) — LSP-powered code intelligence
