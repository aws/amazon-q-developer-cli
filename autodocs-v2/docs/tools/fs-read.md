---
doc_meta:
  title: fs_read
  description: Read files, directories, and images with support for line ranges and batch operations
  category: tool
  keywords: [fs_read, read, file, directory, image, batch, line, offset, limit]
  related: [fs-write, grep, glob]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: true
---

## Overview

> This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally.

The fs_read tool provides file system reading capabilities with three operation modes: reading file lines with offset/limit, listing directory contents, and reading images. Multiple operations can be batched in a single invocation.

## Usage

The tool accepts an `operations` array. Each operation specifies a `mode` and mode-specific parameters.

### Line Mode

Read file contents with optional offset and limit.

- `mode` — `"Line"`
- `path` (string, required) — File path
- `offset` (integer, optional) — Line offset from start of file to begin reading
- `limit` (integer, optional) — Number of lines to read

```json
{
  "operations": [{"mode": "Line", "path": "src/main.rs", "offset": 10, "limit": 40}]
}
```

### Directory Mode

List directory contents with optional recursion.

- `mode` — `"Directory"`
- `path` (string, required) — Directory path
- `depth` (integer, optional) — Recursion depth (default: 0, non-recursive)
- `exclude_patterns` (array, optional) — Glob patterns to exclude (default: `["node_modules", ".git", "dist", "build", "out", ".cache", "target"]`)

```json
{
  "operations": [{"mode": "Directory", "path": "src/", "depth": 2}]
}
```

### Image Mode

Read image files for vision-capable models.

- `mode` — `"Image"`
- `image_paths` (array, required) — List of image file paths

Supported formats: PNG, JPG, JPEG, GIF, WEBP.

```json
{
  "operations": [{"mode": "Image", "image_paths": ["screenshot.png", "diagram.jpg"]}]
}
```

### Batch Operations

```json
{
  "operations": [
    {"mode": "Line", "path": "README.md"},
    {"mode": "Directory", "path": "src/"},
    {"mode": "Image", "image_paths": ["/path/to/image.png"]}
  ]
}
```

## Examples

### Read a specific section of a file

```json
{
  "operations": [{"mode": "Line", "path": "src/lib.rs", "offset": 50, "limit": 20}]
}
```

Reads 20 lines starting from line 50.

### Explore project structure

```json
{
  "operations": [{"mode": "Directory", "path": ".", "depth": 1}]
}
```

Lists current directory one level deep, excluding default patterns.

### Read multiple config files

```json
{
  "operations": [
    {"mode": "Line", "path": "package.json"},
    {"mode": "Line", "path": "tsconfig.json"},
    {"mode": "Line", "path": "README.md", "limit": 20}
  ]
}
```

## Troubleshooting

### "does not exist" error

File or directory not found. Verify the path is correct — use relative paths from the current working directory or absolute paths.

### "is not a file" error

Tried to use Line mode on a directory. Use Directory mode for directories.

### Directory listing truncated

Directory has more entries than the limit (1000). Use `exclude_patterns` to narrow results or increase depth selectively.

### Image not supported

File format not recognized. Convert to PNG, JPG, JPEG, GIF, or WEBP.

## Related

- [fs_write](fs-write.md) — Write and modify files
- [grep](grep.md) — Fast regex pattern search across multiple files
- [glob](glob.md) — Find files matching glob patterns
- [code](code.md) — LSP-powered code intelligence
