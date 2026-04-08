---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: grep
  description: Fast regex pattern search in files with configurable output modes and limits
  keywords: [grep, search, regex, pattern, find, text]
  related: [fs-read, glob, code]
---

# grep

Fast regex pattern search in files with configurable output modes and limits.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to search for patterns in files as needed.

The grep tool searches for regex patterns across files in a directory tree. It respects .gitignore, supports file filtering, and offers three output modes: content (matching lines), files_with_matches (paths only), and count (match counts). Configurable limits prevent overwhelming output.

## How It Works

Grep walks the directory tree using ignore rules (.gitignore), applies file filters, and searches each file for the pattern. Results are limited by max_matches_per_file, max_files, and max_total_lines to keep output manageable. Case-insensitive by default.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

### Basic Usage

```json
{
  "pattern": "TODO"
}
```

### Common Use Cases

#### Use Case 1: Find TODOs in Project

```json
{
  "pattern": "TODO|FIXME",
  "include": "*.rs"
}
```

**What this does**: Searches all Rust files for TODO or FIXME comments. Returns matching lines with file paths and line numbers.

#### Use Case 2: Find Function Definitions

```json
{
  "pattern": "fn main",
  "path": "src/",
  "case_sensitive": true
}
```

**What this does**: Case-sensitive search for "fn main" in src/ directory. Useful for finding specific code patterns.

#### Use Case 3: List Files Containing Pattern

```json
{
  "pattern": "import React",
  "include": "*.{ts,tsx}",
  "output_mode": "files_with_matches"
}
```

**What this does**: Returns only file paths containing "import React" in TypeScript files. No line content shown.

#### Use Case 4: Count Matches Per File

```json
{
  "pattern": "console\\.log",
  "output_mode": "count"
}
```

**What this does**: Shows how many times console.log appears in each file. Useful for auditing.

#### Use Case 5: Deep Search with Limits

```json
{
  "pattern": "error",
  "max_depth": 5,
  "max_files": 50,
  "max_matches_per_file": 10
}
```

**What this does**: Searches up to 5 directory levels deep, returns max 50 files, max 10 matches per file.

## Configuration

No agent configuration available - grep is trusted by default.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pattern` | string | required | Regex pattern to search for |
| `path` | string | `.` (cwd) | Directory to search from |
| `include` | string | none | File filter glob (e.g., `*.rs`, `*.{ts,tsx}`) |
| `case_sensitive` | boolean | `false` | Enable case-sensitive matching |
| `output_mode` | string | `content` | Output format: `content`, `files_with_matches`, `count` |
| `max_matches_per_file` | integer | 5 | Max matches per file (content mode). Max: 30 |
| `max_files` | integer | 100 | Max files in results. Max: 400 |
| `max_total_lines` | integer | 100 | Max total output lines (content mode). Max: 300 |
| `max_depth` | integer | 30 | Max directory depth. Max: 50 |

## Output Modes

### content (default)

Shows matching lines with file path and line number.

**Format**: `file:line:content`

**Example Output**:
```
src/main.rs:15:    // TODO: implement error handling
src/lib.rs:42:    // TODO: add tests
```

### files_with_matches

Shows only file paths containing matches.

**Example Output**:
```json
{
  "numFiles": 2,
  "numMatches": 5,
  "filePaths": [
    "src/main.rs",
    "src/lib.rs"
  ]
}
```

### count

Shows match count per file.

**Example Output**:
```json
{
  "numFiles": 2,
  "numMatches": 5,
  "results": [
    {"file": "src/main.rs", "count": 3},
    {"file": "src/lib.rs", "count": 2}
  ]
}
```

## Examples

### Example 1: Find Error Messages

```json
{
  "pattern": "error|Error|ERROR"
}
```

**Expected Output**:
```
src/main.rs:23:    return Err(Error::NotFound);
src/lib.rs:45:    log::error!("Failed to connect");
```

### Example 2: Search Specific File Types

```json
{
  "pattern": "class.*Component",
  "include": "*.{js,jsx,ts,tsx}"
}
```

**Expected Output**: All React component class definitions in JavaScript/TypeScript files.

### Example 3: Case-Sensitive Search

```json
{
  "pattern": "^import",
  "case_sensitive": true,
  "include": "*.py"
}
```

**Expected Output**: Python import statements at start of lines (case-sensitive).

### Example 4: Count API Calls

```json
{
  "pattern": "fetch\\(|axios\\.",
  "output_mode": "count",
  "include": "*.ts"
}
```

**Expected Output**: Count of fetch() and axios calls per TypeScript file.

## Troubleshooting

### Issue: Too Many Results

**Symptom**: Output truncated or overwhelming  
**Cause**: Pattern matches too broadly  
**Solution**: Refine pattern to be more specific. Reduce max_files or max_matches_per_file. Use files_with_matches mode.

### Issue: No Results Found

**Symptom**: Empty results for pattern that should match  
**Cause**: Case sensitivity, wrong path, or .gitignore exclusion  
**Solution**: Try case_sensitive: false. Check path is correct. Verify files aren't gitignored.

### Issue: Pattern Syntax Error

**Symptom**: Error about invalid regex  
**Cause**: Invalid regex syntax  
**Solution**: Test pattern with simpler version first. Escape special characters: `.`, `*`, `+`, `?`, `(`, `)`, `[`, `]`, `{`, `}`, `|`, `^`, `$`, `\`

### Issue: Binary Files Searched

**Symptom**: Garbled output from binary files  
**Cause**: Grep searches all files by default  
**Solution**: Use include parameter to filter file types: `"include": "*.{rs,md,txt}"`

### Issue: Results Truncated

**Symptom**: Message about truncation  
**Cause**: Hit max_files, max_matches_per_file, or max_total_lines limit  
**Solution**: Increase limits or refine search pattern. Use count mode to see totals.

## Related Features

- [fs_read](fs-read.md) - Read specific files found by grep
- [glob](glob.md) - Find files by name pattern
- [code](code.md) - Semantic code search with LSP

## Limitations

- Respects .gitignore - won't search ignored files
- Max line length 500 chars (prevents minified file issues)
- No multi-line pattern matching
- Binary files detected and skipped
- Output limits enforced to prevent context overflow
- No real-time streaming - results returned after completion
- Regex doesn't support look-around assertions

## Technical Details

**Aliases**: `grep`

**Pattern Syntax**: Rust regex syntax. Case-insensitive by default.

**File Walking**: Uses ignore crate - respects .gitignore, .ignore, .git/info/exclude

**Binary Detection**: Automatically skips binary files

**Performance**: Yields every 100 files to allow cancellation

**Limits**:
- Max matches per file: 5 (default), 30 (max)
- Max files: 100 (default), 400 (max)
- Max total lines: 100 (default), 300 (max)
- Max depth: 30 (default), 50 (max)
- Max line length: 500 chars

**Permissions**: Trusted by default, no configuration needed.
