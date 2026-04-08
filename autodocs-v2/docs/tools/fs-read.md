---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: fs_read
  description: Read files, directories, and images with support for line ranges, pattern search, and batch operations
  keywords: [fs_read, read, file, directory, image, search, batch]
  related: [fs-write, grep, glob]
---

# fs_read

Read files, directories, and images with support for line ranges, pattern search, and batch operations.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to read files and directories as needed.

The fs_read tool provides comprehensive file system reading capabilities. It supports four operation modes: reading file lines with range selection, listing directory contents, searching for patterns within files, and reading images. Multiple operations can be batched in a single tool invocation.

## How It Works

The tool accepts an array of operations, each specifying a mode (Line, Directory, Search, or Image) and mode-specific parameters. Operations execute sequentially and return combined results. Path validation ensures files/directories exist before reading.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

### Basic Usage

```json
{
  "operations": [{
    "mode": "Line",
    "path": "/path/to/file.txt"
  }]
}
```

### Common Use Cases

#### Use Case 1: Read Specific Lines from a File

```json
{
  "operations": [{
    "mode": "Line",
    "path": "src/main.rs",
    "start_line": 10,
    "end_line": 50
  }]
}
```

**What this does**: Reads lines 10-50 from src/main.rs. Negative indices count from end (-1 is last line).

#### Use Case 2: List Directory Contents

```json
{
  "operations": [{
    "mode": "Directory",
    "path": "src/",
    "depth": 2,
    "max_entries": 100
  }]
}
```

**What this does**: Lists files in src/ up to 2 levels deep, maximum 100 entries. Respects .gitignore patterns.

#### Use Case 3: Search for Pattern in File

```json
{
  "operations": [{
    "mode": "Search",
    "path": "src/lib.rs",
    "pattern": "pub fn",
    "context_lines": 2
  }]
}
```

**What this does**: Finds all lines containing "pub fn" with 2 lines of context before/after each match.

#### Use Case 4: Read Images

```json
{
  "operations": [{
    "mode": "Image",
    "image_paths": ["screenshot.png", "diagram.jpg"]
  }]
}
```

**What this does**: Reads image files for vision-capable models. Supports PNG, JPG, JPEG, GIF, WEBP.

#### Use Case 5: Batch Operations

```json
{
  "operations": [
    {"mode": "Line", "path": "README.md"},
    {"mode": "Directory", "path": "src/"},
    {"mode": "Search", "path": "Cargo.toml", "pattern": "dependencies"}
  ],
  "summary": "Read project overview"
}
```

**What this does**: Executes multiple operations in one tool call. Optional summary describes the batch purpose.

## Configuration

Configure path restrictions in agent's `toolsSettings`:

```json
{
  "toolsSettings": {
    "fs_read": {
      "allowedPaths": ["~/projects", "./src/**"],
      "deniedPaths": ["/etc/**", "~/.ssh/**"]
    }
  }
}
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `allowedPaths` | array | `[]` | Paths readable without prompting. Supports glob patterns (gitignore syntax) |
| `deniedPaths` | array | `[]` | Paths that are blocked. Evaluated before allowedPaths. Supports glob patterns |

**Glob Pattern Behavior**: Patterns like `~/temp` match `~/temp/child` and all descendants.

## Operation Modes

### Line Mode

Read file contents with optional line range.

**Parameters**:
- `path` (string, required): File path
- `start_line` (integer, optional): Starting line (1-indexed, default: 1). Negative values count from end
- `end_line` (integer, optional): Ending line (default: -1 for last line). Negative values count from end

**Example**:
```json
{"mode": "Line", "path": "file.txt", "start_line": 1, "end_line": 100}
```

### Directory Mode

List directory contents recursively.

**Parameters**:
- `path` (string, required): Directory path
- `depth` (integer, optional): Max recursion depth (default: 0 for non-recursive)
- `max_entries` (integer, optional): Max entries to return (default: 1000)
- `offset` (integer, optional): Skip first N entries for pagination (default: 0)
- `exclude_patterns` (array, optional): Glob patterns to exclude (default: `["node_modules", ".git", "dist", "build", "out", ".cache", "target"]`). Empty array `[]` shows everything
- `show_deleted` (boolean, optional): Include deleted items (default: false)

**Example**:
```json
{
  "mode": "Directory",
  "path": "src/",
  "depth": 2,
  "max_entries": 500,
  "exclude_patterns": ["*.test.js", "**/__pycache__/**"]
}
```

### Search Mode

Search for pattern within a file (case-insensitive, per-line matching).

**Parameters**:
- `path` (string, required): File path
- `pattern` (string, required): Search pattern (case-insensitive)
- `context_lines` (integer, optional): Lines of context around matches (default: 2)

**Example**:
```json
{
  "mode": "Search",
  "path": "src/main.rs",
  "pattern": "TODO",
  "context_lines": 3
}
```

### Image Mode

Read image files for vision models.

**Parameters**:
- `image_paths` (array, required): List of image file paths

**Supported formats**: PNG, JPG, JPEG, GIF, WEBP

**Example**:
```json
{
  "mode": "Image",
  "image_paths": ["screenshot.png", "diagram.jpg"]
}
```

## Examples

### Example 1: Read Configuration File

```json
{
  "operations": [{
    "mode": "Line",
    "path": ".kiro/agents/my-agent.json"
  }]
}
```

**Expected Output**: Complete file contents with syntax highlighting.

### Example 2: Explore Project Structure

```json
{
  "operations": [{
    "mode": "Directory",
    "path": ".",
    "depth": 1,
    "exclude_patterns": ["node_modules", ".git"]
  }]
}
```

**Expected Output**: Tree view of current directory, one level deep, excluding node_modules and .git.

### Example 3: Find All TODOs

```json
{
  "operations": [{
    "mode": "Search",
    "path": "src/lib.rs",
    "pattern": "TODO|FIXME",
    "context_lines": 1
  }]
}
```

**Expected Output**: All lines containing TODO or FIXME with 1 line of context.

### Example 4: Read Multiple Files

```json
{
  "operations": [
    {"mode": "Line", "path": "package.json"},
    {"mode": "Line", "path": "tsconfig.json"},
    {"mode": "Line", "path": "README.md", "start_line": 1, "end_line": 20}
  ],
  "summary": "Read project configuration files"
}
```

**Expected Output**: Contents of all three files, with README limited to first 20 lines.

## Troubleshooting

### Issue: "does not exist" Error

**Symptom**: Error message "'path' does not exist"  
**Cause**: File or directory not found at specified path  
**Solution**: Verify path is correct. Use relative paths from current working directory or absolute paths.

### Issue: "is not a file" Error

**Symptom**: Error when trying to read a directory with Line mode  
**Cause**: Path points to directory, not file  
**Solution**: Use Directory mode for directories, Line mode only for files.

### Issue: Directory Listing Truncated

**Symptom**: Message "showing X of Y entries"  
**Cause**: Directory has more entries than max_entries limit  
**Solution**: Increase `max_entries` or use `offset` for pagination. Default limit is 1000.

### Issue: Permission Denied

**Symptom**: Tool prompts for permission or is denied  
**Cause**: Path not in agent's `allowedPaths` or is in `deniedPaths`  
**Solution**: Add path to `allowedPaths` in agent config or approve when prompted.

### Issue: Image Not Supported

**Symptom**: "'path' is not a supported image type"  
**Cause**: File format not supported  
**Solution**: Convert image to PNG, JPG, JPEG, GIF, or WEBP format.

## Related Features

- [fs_write](fs-write.md) - Write and modify files
- [grep](grep.md) - Fast regex pattern search across multiple files
- [glob](glob.md) - Find files matching glob patterns
- [code](code.md) - LSP-powered code intelligence for semantic file navigation

## Limitations

- Line mode reads entire file into memory - may be slow for very large files
- Search mode is case-insensitive and matches per-line only (no multi-line patterns)
- Directory mode respects .gitignore but may still list many files in large projects
- Image mode requires vision-capable model
- Maximum tool response size is enforced - very large outputs may be truncated
- Batch operations execute sequentially, not in parallel

## Technical Details

**Aliases**: `fs_read`, `read`

**Path Handling**: All paths are sanitized and resolved relative to current working directory. Tilde (`~`) expands to user home directory.

**Permissions**: fs_read is trusted by default but can be restricted via `allowedPaths`/`deniedPaths` in agent config. Deny rules evaluated before allow rules.

**Output Limits**: Tool enforces MAX_TOOL_RESPONSE_SIZE to prevent context window overflow. Large files may be truncated with continuation markers.
