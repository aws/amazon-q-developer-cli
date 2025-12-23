---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: glob
  description: Find files and directories matching glob patterns with .gitignore support
  keywords: [glob, find, files, pattern, search, wildcard]
  related: [fs-read, grep, execute-bash]
---

# glob

Find files and directories matching glob patterns with .gitignore support.

## Overview

The glob tool finds files and directories whose paths match a glob pattern. It respects .gitignore rules, supports complex patterns with wildcards and braces, and returns results as JSON with truncation indicators. Prefer this over bash `find` command for path discovery.

## How It Works

Glob walks the directory tree from a base path, applies the pattern to each path, and collects matches. It respects .gitignore, .ignore, and .git/info/exclude files. Results are limited by the limit parameter to prevent overwhelming output.

## Usage

### Basic Usage

```json
{
  "pattern": "**/*.rs"
}
```

### Common Use Cases

#### Use Case 1: Find All Rust Files

```json
{
  "pattern": "**/*.rs"
}
```

**What this does**: Finds all .rs files recursively from current directory. Respects .gitignore.

#### Use Case 2: Find Files in Specific Directory

```json
{
  "pattern": "src/**/*.ts",
  "path": "."
}
```

**What this does**: Finds all TypeScript files under src/ directory.

#### Use Case 3: Find Multiple File Types

```json
{
  "pattern": "**/*.{js,jsx,ts,tsx}"
}
```

**What this does**: Finds all JavaScript and TypeScript files using brace expansion.

#### Use Case 4: Find Test Files

```json
{
  "pattern": "**/*test*.rs",
  "limit": 50
}
```

**What this does**: Finds files with "test" in name, limited to 50 results.

#### Use Case 5: Deep Search with Depth Limit

```json
{
  "pattern": "**/Cargo.toml",
  "max_depth": 3
}
```

**What this does**: Finds Cargo.toml files up to 3 directory levels deep.

## Configuration

No agent configuration available - glob is trusted by default.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `pattern` | string | required | Glob pattern (e.g., `**/*.rs`, `src/**/*.{ts,tsx}`) |
| `path` | string | `.` (cwd) | Root directory to search from |
| `limit` | integer | 200 | Maximum number of results to return |
| `max_depth` | integer | 30 | Maximum directory depth to traverse. Max: 50 |

## Glob Pattern Syntax

- `*` - Matches any characters except `/`
- `**` - Matches any characters including `/` (recursive)
- `?` - Matches single character
- `[abc]` - Matches one character from set
- `[a-z]` - Matches one character from range
- `{a,b,c}` - Matches any of the alternatives (brace expansion)
- `!` or `^` - Negation (at start of pattern)

**Examples**:
- `*.rs` - All .rs files in current directory
- `**/*.rs` - All .rs files recursively
- `src/**/*.{ts,tsx}` - TypeScript files under src/
- `**/test_*.rs` - Test files anywhere
- `target/debug/build/**/*` - Everything under target/debug/build/

## Output Format

Returns JSON with:

```json
{
  "totalFiles": 150,
  "truncated": false,
  "filePaths": [
    "src/main.rs",
    "src/lib.rs",
    "tests/integration_test.rs"
  ]
}
```

**Fields**:
- `totalFiles`: Number of files found
- `truncated`: true if results limited by limit parameter
- `filePaths`: Array of matching paths (relative to search base)

## Examples

### Example 1: Find Configuration Files

```json
{
  "pattern": "**/*.{json,yaml,yml,toml}"
}
```

**Expected Output**:
```json
{
  "totalFiles": 5,
  "truncated": false,
  "filePaths": [
    "package.json",
    "tsconfig.json",
    "Cargo.toml",
    ".github/workflows/ci.yml"
  ]
}
```

### Example 2: Find Source Files

```json
{
  "pattern": "src/**/*.rs",
  "limit": 100
}
```

**Expected Output**: All Rust source files under src/, max 100 files.

### Example 3: Find Markdown Documentation

```json
{
  "pattern": "**/*.md",
  "max_depth": 2
}
```

**Expected Output**: Markdown files up to 2 levels deep.

### Example 4: Find Build Artifacts

```json
{
  "pattern": "target/**/*.so",
  "path": "."
}
```

**Expected Output**: Shared library files in target directory.

## Troubleshooting

### Issue: No Results Found

**Symptom**: Empty filePaths array  
**Cause**: Pattern doesn't match any files, or files are gitignored  
**Solution**: Verify pattern syntax. Check if files are in .gitignore. Try simpler pattern like `**/*` to see all files.

### Issue: Results Truncated

**Symptom**: `truncated: true` in output  
**Cause**: More files match than limit allows  
**Solution**: Increase limit parameter or refine pattern to be more specific.

### Issue: Pattern Not Working

**Symptom**: Expected files not in results  
**Cause**: Incorrect glob syntax  
**Solution**: Test with simpler patterns. Remember `*` doesn't cross directories, use `**` for recursive. Check brace syntax: `{a,b}` not `{a, b}`.

### Issue: Too Many Results

**Symptom**: Large output, slow performance  
**Cause**: Pattern too broad  
**Solution**: Reduce limit, increase specificity of pattern, or use max_depth to limit recursion.

### Issue: Path Not Found

**Symptom**: Error "Path does not exist"  
**Cause**: Invalid path parameter  
**Solution**: Verify path exists. Use relative paths from current directory or absolute paths.

## Related Features

- [fs_read](fs-read.md) - Read files found by glob
- [grep](grep.md) - Search content in files
- [execute_bash](execute-bash.md) - Alternative: use `find` command

## Limitations

- Respects .gitignore - won't find ignored files
- Results limited by limit parameter (default 200)
- Max depth limited to 50 levels
- No sorting of results (order not guaranteed)
- Pattern applied to full path, not just filename
- No exclusion patterns (use .gitignore instead)
- Yields every 500 entries for cancellation

## Technical Details

**Aliases**: `glob`

**Pattern Matching**: Uses globset crate with gitignore-style patterns

**File Walking**: Uses ignore crate - respects .gitignore, .ignore, .git/info/exclude

**Path Normalization**: Extracts directory prefix from pattern for efficient searching

**Performance**: Yields periodically to allow cancellation during long searches

**Limits**:
- Default limit: 200 files
- Default max_depth: 30 levels
- Max allowed depth: 50 levels

**Permissions**: Trusted by default, no configuration needed.

**Output**: Returns JSON with totalFiles count, truncated flag, and filePaths array. When truncated is true, just mention results are truncated without stating the limit number.
