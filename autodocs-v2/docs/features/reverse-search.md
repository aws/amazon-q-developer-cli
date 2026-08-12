---
doc_meta:
  title: Reverse Search
  description: Search command history interactively with Ctrl+R
  category: feature
  keywords: [history, search, ctrl-r, reverse, incremental, readline]
  related: [classic-vs-tui]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
---

# Reverse Search

Search command history interactively with Ctrl+R.

## Overview

Reverse incremental search lets you find previous commands by typing part of them. Press Ctrl+R, start typing, and matching history entries appear instantly. Works like bash/readline reverse search.

**TUI only**: This feature is available in the new TUI interface, not classic mode.

## Usage

1. Press `Ctrl+R` to enter search mode
2. Type characters to search — matches appear as you type
3. Press `Ctrl+R` again to cycle to older matches
4. Accept the match or cancel

The prompt changes to show your search:
```
(reverse-i-search)`echo': echo hello world
```

## Key Bindings

| Key | Action |
|-----|--------|
| `Ctrl+R` | Enter search / cycle to older match |
| `Backspace` | Delete last character from search query |
| `Enter` | Accept match and submit as input |
| `Escape` | Accept match and edit before submitting |
| `Ctrl+A` | Accept match, cursor at beginning |
| `Ctrl+E` | Accept match, cursor at end |
| `Right Arrow` | Accept match, cursor at match position |
| `Ctrl+C` | Abort search, clear input |

## Examples

### Example 1: Find a Previous Command

```
> Ctrl+R
(reverse-i-search)`': 
```

Type `git`:
```
(reverse-i-search)`git': git push origin main
```

Press Enter to submit, or Escape to edit first.

### Example 2: Cycle Through Matches

If you have multiple commands containing "echo":
```
(reverse-i-search)`echo': echo goodbye
```

Press Ctrl+R again:
```
(reverse-i-search)`echo': echo hello
```

Each Ctrl+R moves to the next older match.

### Example 3: Narrow Your Search

Start broad, then add characters:
```
(reverse-i-search)`cd': cd packages/tui
```

Add more:
```
(reverse-i-search)`cd packages/c': cd packages/core
```

### Example 4: Reuse Last Search

After accepting a search, press Ctrl+R twice quickly. The second Ctrl+R reuses your previous search query.

## Troubleshooting

### Issue: No Match Found

**Symptom**: Query shows but no result appears  
**Cause**: No history entry contains your search string  
**Solution**: Backspace to shorten query, or Escape to cancel

### Issue: Wrong Match

**Symptom**: Found a match but want an older one  
**Solution**: Press Ctrl+R again to cycle to older matches

### Issue: Ctrl+R Does Nothing

**Symptom**: Ctrl+R not recognized  
**Cause**: Using classic mode, not TUI  
**Solution**: Run without `--classic` flag

## Related

- [Classic vs TUI](classic-vs-tui.md) — TUI-specific features
