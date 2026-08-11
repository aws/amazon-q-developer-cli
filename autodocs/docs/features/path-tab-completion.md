---
doc_meta:
  title: Path Tab Completion
  description: Press Tab to auto-complete filesystem paths while typing in the chat input
  category: feature
  keywords: [tab, completion, path, filesystem, autocomplete, input]
  related: [file-references, chat]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
---

# Path Tab Completion

Press Tab to auto-complete filesystem paths while typing in the chat input.

## Overview

When typing a path in the chat input, press Tab to auto-complete it based on files and directories on your filesystem. This works for any path-like token, not just `@` file references.

## Usage

Start typing a path and press Tab:

```
/home/user/pro<Tab>     → /home/user/projects/
./src/ma<Tab>           → ./src/main.rs
../con<Tab>             → ../config/
~/Doc<Tab>              → ~/Documents/
```

### Path Triggers

Tab completion activates for tokens that look like paths:
- Absolute paths: `/path/to/file`
- Relative paths: `./file` or `../file`
- Home directory: `~/file`
- Any token containing `/`

### Completion Behavior

- Directories get a trailing `/` appended
- Spaces in paths are escaped with `\`
- Hidden files (starting with `.`) are excluded unless you type the `.`
- Multiple matches complete to the longest common prefix

## Examples

### Example 1: Complete a Directory

```
> Check the files in /etc/sys<Tab>
> Check the files in /etc/systemd/
```

### Example 2: Complete a File

```
> Read ./READ<Tab>
> Read ./README.md
```

### Example 3: Home Directory

```
> Look at ~/.bash<Tab>
> Look at ~/.bashrc
```

### Example 4: Path with Spaces

```
> Open ~/My\ Docu<Tab>
> Open ~/My\ Documents/
```

Spaces are automatically escaped.

### Example 5: Multiple Matches

If multiple files match, completion stops at the common prefix:

```
> ./src/lib<Tab>
> ./src/lib
```

If `lib.rs` and `lib/` both exist, Tab completes to `lib` and you can press Tab again or type more.

## Troubleshooting

### Tab Does Nothing

**Cause**: Token doesn't look like a path  
**Solution**: Ensure the token starts with `/`, `./`, `../`, `~`, or contains `/`

### Wrong Completion

**Cause**: Multiple matches with same prefix  
**Solution**: Type more characters to narrow matches, then Tab again

### Completion Skipped

**Cause**: Slash menu or file picker is open  
**Solution**: Close the menu first (Escape), then use Tab

## Limitations

- Only completes paths, not commands or other tokens
- No glob pattern expansion
- Hidden files excluded by default
- Requires the path token to be at cursor position

## Related

- [File References](file-references.md) - Use `@path` to include file contents in messages
- [kiro-cli chat](../commands/chat.md) - Start a chat session
