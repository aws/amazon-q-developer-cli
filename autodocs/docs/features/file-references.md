---
doc_meta:
  validated: 2026-02-09
  commit: 41976c78
  status: validated
  testable_headless: true
  category: feature
  title: File References
  description: Use @path syntax to include file contents or directory listings inline in chat messages
  keywords: [file, directory, reference, at, path, inline, context, tree, quoted]
  related: [prompts, fs-read, context]
---

# File References

Use `@path` syntax to include file contents or directory listings inline in chat messages.

## Overview

Type `@` followed by a file or directory path to automatically inject its contents into your message. Files are included as code blocks, directories as tree listings. This provides quick context without manually copying content.

## Usage

```
@path/to/file.rs          # Include file contents
@src/                     # Include directory tree
@./relative/path          # Relative paths work
@"path with spaces.txt"   # Quoted paths for spaces
```

References can appear anywhere in your message:

```
Review @src/main.rs and suggest improvements
Compare @old.rs with @new.rs
What's in @src/ that handles auth?
```

## Quoted Paths

For paths containing spaces, wrap the path in quotes:

```
@"my file.txt"            # File with space in name
@"path/to/my file.rs"     # Path with space
```

Tab completion automatically adds quotes when completing paths with spaces.

## Tab Completion

Press Tab after `@` to auto-complete paths:

```
@src/<Tab>                # Shows files in src/
@Cargo<Tab>               # Completes to @Cargo.toml
@"Screenshot<Tab>         # Completes quoted paths with spaces
```

Completion works anywhere in the line, not just at the start.

## Syntax Highlighting

References are highlighted in purple as you type, making them easy to identify.

## Priority Rules

When a reference could match both a prompt and a file:

1. **Known prompts win** - If `@name` matches a prompt from `/prompts list`, it's treated as a prompt
2. **Files second** - Otherwise, checked as file path
3. **Directories third** - If not a file, checked as directory

## File Handling

### Supported Files

- Text files (source code, config, markdown, etc.)
- Files up to 250KB (larger files truncated with warning)

### Unsupported Files

- Binary files (images, executables, archives) - shows error
- Files without read permission - shows error

### Truncation

Large files are truncated at 250KB with a warning:

```
⚠ File 'large-file.json' was truncated (exceeds 250KB limit)
```

## Directory Handling

Directories show a tree listing:

```
@src/
```

Expands to:

```
src/
├── lib/
│   ├── mod.rs
│   └── utils.rs
├── main.rs
└── config.rs
```

### Tree Limits

- Max depth: 3 levels
- Max items per level: 10 (shows "... (N more items)" if exceeded)
- Ignores: node_modules, .git, target, dist, build, etc.

## Examples

### Example 1: Review a File

```
> Review @src/auth.rs for security issues
```

Sends message with file contents injected.

### Example 2: Compare Files

```
> What's different between @v1/api.rs and @v2/api.rs?
```

Both files included inline.

### Example 3: Explore Directory

```
> What's the structure of @crates/agent/?
```

Shows directory tree.

### Example 4: Mixed Context

```
> Using the config in @config.toml, update @src/settings.rs
```

Both file contents included.

### Example 5: With Regular Text

```
> I'm getting an error in @src/parser.rs on line 42. The error is "unexpected token"
```

File contents provide context for the error.

## Error Messages

**Binary file**:
```
⚠ File 'image.png' appears to be binary and cannot be included inline.
```

**Read error**:
```
⚠ Failed to read 'secret.key': permission denied
```

## Troubleshooting

### Issue: Reference Treated as Prompt

**Symptom**: `@myfile` runs a prompt instead of including file  
**Cause**: A prompt named "myfile" exists  
**Solution**: Use explicit path: `@./myfile` or rename prompt

### Issue: File Not Found

**Symptom**: Error for existing file  
**Cause**: Path is relative to wrong directory  
**Solution**: Use absolute path or check current directory

### Issue: Directory Too Deep

**Symptom**: Subdirectories not shown  
**Cause**: Tree depth limit (3 levels)  
**Solution**: Reference specific subdirectory: `@src/deep/path/`

### Issue: Large File Warning

**Symptom**: "File truncated" warning  
**Cause**: File exceeds 250KB  
**Solution**: Reference specific sections with line numbers in your question

### Issue: Path Has Spaces

**Symptom**: Only part of path is recognized  
**Cause**: Space breaks the reference  
**Solution**: Use quoted syntax: `@"path with spaces.txt"`

### Issue: Completion Not Working

**Symptom**: Tab doesn't complete paths  
**Cause**: No `@` prefix or invalid path start  
**Solution**: Ensure `@` precedes the path

## Limitations

- Max file size: 250KB (truncated beyond)
- Max tree depth: 3 levels
- Max items per directory level: 10
- Binary files not supported
- No glob patterns (`@*.rs` won't work)
- No line range syntax (`@file.rs:10-20` won't work)
- No home directory expansion (`@~` won't work)

## Related

- [/prompts](../slash-commands/prompts.md) - Prompt templates (also use @ syntax)
- [/context](../slash-commands/context.md) - Persistent context files
- [fs_read](../tools/fs-read.md) - Tool for reading files
