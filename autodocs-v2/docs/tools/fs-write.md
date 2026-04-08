---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: fs_write
  description: Create and modify files with support for create, str_replace, insert, and append operations
  keywords: [fs_write, write, create, edit, modify, file, str_replace, insert, append]
  related: [fs-read, code]
---

# fs_write

Create and modify files with support for create, str_replace, insert, and append operations.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to create and modify files as needed.

The fs_write tool provides file creation and editing capabilities through four commands: create (new files or overwrite), str_replace (find and replace), insert (add at line position), and append (add to end). All operations include safety checks and can be restricted via agent configuration.

## How It Works

Each operation validates the target path, creates parent directories if needed, and performs the requested modification. The str_replace command ensures exactly one match to prevent unintended changes. All operations track file modifications for context management.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

### Basic Usage

```json
{
  "command": "create",
  "path": "output.txt",
  "file_text": "Hello, world!"
}
```

### Common Use Cases

#### Use Case 1: Create New File

```json
{
  "command": "create",
  "path": "src/config.json",
  "file_text": "{\"version\": \"1.0\"}",
  "summary": "Create configuration file"
}
```

**What this does**: Creates new file with content. Creates parent directories if needed. Overwrites if file exists.

#### Use Case 2: Replace Text in File

```json
{
  "command": "str_replace",
  "path": "README.md",
  "old_str": "## Installation\n\nComing soon",
  "new_str": "## Installation\n\nnpm install my-package",
  "summary": "Update installation instructions"
}
```

**What this does**: Finds exact match of old_str and replaces with new_str. Fails if 0 or >1 matches found.

#### Use Case 3: Insert at Line Position

```json
{
  "command": "insert",
  "path": "src/main.rs",
  "insert_line": 5,
  "new_str": "use std::collections::HashMap;\n",
  "summary": "Add import statement"
}
```

**What this does**: Inserts new_str after line 5. Line numbers are 0-indexed (0 = before first line).

#### Use Case 4: Append to File

```json
{
  "command": "append",
  "path": "logs/output.log",
  "new_str": "[2025-12-19] Process completed\n",
  "summary": "Add log entry"
}
```

**What this does**: Adds content to end of file. Automatically adds newline if file doesn't end with one.

## Configuration

Configure path restrictions in agent's `toolsSettings`:

```json
{
  "toolsSettings": {
    "fs_write": {
      "allowedPaths": ["~/projects/output/**", "./src/**"],
      "deniedPaths": ["/etc/**", "~/.ssh/**"],
      "fallbackAction": "deny"
    }
  }
}
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedPaths` | array | `[]` | Paths writable without prompting. Supports glob patterns (gitignore syntax) |
| `deniedPaths` | array | `[]` | Paths that are blocked. Evaluated before allowedPaths. Supports glob patterns |
| `fallbackAction` | string | `"interactive"` | Behavior for paths outside allowedPaths: `"interactive"` (prompt), `"deny"` (block completely) |

**Glob Pattern Behavior**: Patterns like `~/temp` match `~/temp/child` and all descendants.

## Commands

### create

Create new file or overwrite existing file.

**Parameters**:
- `path` (string, required): File path
- `file_text` (string, required): Complete file contents
- `summary` (string, optional): Description of the change

**Example**:
```json
{
  "command": "create",
  "path": "output.txt",
  "file_text": "Line 1\nLine 2\nLine 3"
}
```

**Behavior**: Creates parent directories if needed. Overwrites existing files without warning.

### str_replace

Find and replace exact text match.

**Parameters**:
- `path` (string, required): File path
- `old_str` (string, required): Exact text to find (must match exactly once)
- `new_str` (string, required): Replacement text
- `summary` (string, optional): Description of the change

**Example**:
```json
{
  "command": "str_replace",
  "path": "src/lib.rs",
  "old_str": "fn old_function() {\n    todo!()\n}",
  "new_str": "fn new_function() {\n    println!(\"implemented\");\n}"
}
```

**Behavior**: Fails if old_str not found or found multiple times. Include enough context in old_str to ensure unique match.

### insert

Insert text after specified line.

**Parameters**:
- `path` (string, required): File path
- `insert_line` (integer, required): Line number (0-indexed, 0 = before first line)
- `new_str` (string, required): Text to insert
- `summary` (string, optional): Description of the change

**Example**:
```json
{
  "command": "insert",
  "path": "config.yaml",
  "insert_line": 3,
  "new_str": "  debug: true\n"
}
```

**Behavior**: Inserts after specified line. Line number clamped to valid range (0 to num_lines).

### append

Append text to end of file.

**Parameters**:
- `path` (string, required): File path
- `new_str` (string, required): Text to append
- `summary` (string, optional): Description of the change

**Example**:
```json
{
  "command": "append",
  "path": "notes.txt",
  "new_str": "Additional note\n"
}
```

**Behavior**: Adds newline before new_str if file doesn't end with newline.

## Examples

### Example 1: Create Python Script

```json
{
  "command": "create",
  "path": "scripts/hello.py",
  "file_text": "#!/usr/bin/env python3\n\ndef main():\n    print('Hello, world!')\n\nif __name__ == '__main__':\n    main()\n"
}
```

### Example 2: Update Function Implementation

```json
{
  "command": "str_replace",
  "path": "src/utils.rs",
  "old_str": "pub fn calculate(x: i32) -> i32 {\n    x * 2\n}",
  "new_str": "pub fn calculate(x: i32) -> i32 {\n    x * 2 + 1\n}"
}
```

### Example 3: Add Import Statement

```json
{
  "command": "insert",
  "path": "src/main.rs",
  "insert_line": 0,
  "new_str": "use std::env;\n"
}
```

### Example 4: Append Log Entry

```json
{
  "command": "append",
  "path": "build.log",
  "new_str": "[2025-12-19 22:37] Build completed successfully\n"
}
```

## Troubleshooting

### Issue: "no occurrences of old_str were found"

**Symptom**: str_replace fails with this error  
**Cause**: old_str doesn't match any text in file  
**Solution**: Read file first with fs_read to verify exact text. Include whitespace and newlines exactly as they appear.

### Issue: "X occurrences of old_str were found when only 1 is expected"

**Symptom**: str_replace fails with multiple matches  
**Cause**: old_str matches multiple locations in file  
**Solution**: Include more context in old_str to make it unique. Add surrounding lines or unique identifiers.

### Issue: Permission Denied

**Symptom**: Tool prompts for permission or is denied  
**Cause**: Path not in agent's `allowedPaths` or is in `deniedPaths`  
**Solution**: Add path to `allowedPaths` in agent config, or set `fallbackAction: "interactive"` to allow prompts.

### Issue: File Overwritten Unexpectedly

**Symptom**: create command replaced existing file  
**Cause**: create always overwrites existing files  
**Solution**: Use fs_read first to check if file exists. Use str_replace or insert for modifications.

### Issue: Insert at Wrong Position

**Symptom**: Text inserted at unexpected location  
**Cause**: Line numbers are 0-indexed  
**Solution**: insert_line: 0 inserts before first line, insert_line: 1 inserts after first line, etc.

## Related Features

- [fs_read](fs-read.md) - Read files before modifying
- [code](code.md) - LSP-powered code intelligence for precise edits
- [execute_bash](execute-bash.md) - Run commands like `git diff` to verify changes

## Limitations

- str_replace requires exact match - whitespace and newlines must match precisely
- str_replace fails if old_str appears 0 or >1 times (safety feature)
- create overwrites existing files without confirmation
- No built-in undo - use version control
- Large file operations load entire file into memory
- No atomic multi-file operations

## Technical Details

**Aliases**: `fs_write`, `write`

**Path Handling**: All paths sanitized and resolved relative to current working directory. Tilde (`~`) expands to user home directory.

**Permissions**: Prompts by default unless path in `allowedPaths`. Set `fallbackAction: "deny"` to block instead of prompting. Deny rules evaluated before allow rules.

**Parent Directories**: create command automatically creates parent directories if they don't exist.

**Line Tracking**: All operations update internal file line tracker for context management.

**Safety**: str_replace enforces exactly one match to prevent unintended bulk changes. Use multiple str_replace calls for multiple changes.
