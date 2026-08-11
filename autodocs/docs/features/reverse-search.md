---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: feature
  title: Reverse Incremental Search
  description: Search command history with Ctrl+R, similar to bash/readline
  keywords: [ctrl-r, reverse search, history, search, readline, bash]
  related: [session-management, key-bindings-settings]
---

# Reverse Incremental Search

Search command history with Ctrl+R, similar to bash/readline.

## Overview

Kiro CLI supports reverse incremental search (Ctrl+R) for quickly finding and reusing previous commands from your session history. As you type, the search narrows to show matching history entries. This works like the familiar Ctrl+R in bash or other readline-based shells.

## Usage

Press `Ctrl+R` to enter reverse search mode. The prompt changes to:

```
(reverse-i-search)`': 
```

Type characters to search. Matches appear as you type:

```
(reverse-i-search)`echo': echo hello world
```

The cursor is positioned at the match location within the found entry.

## Key Bindings

| Key | Action |
|-----|--------|
| `Ctrl+R` | Enter search / cycle to older match |
| Any printable char | Append to search query |
| `Backspace` | Delete last character from query |
| `Escape` | Accept match and exit search |
| `Ctrl+C` | Abort search, clear input |
| `Ctrl+A` | Accept match, cursor to beginning |
| `Ctrl+E` | Accept match, cursor to end |
| `Right Arrow` | Accept match, cursor at match position |
| `Enter` | Accept match and submit |

## Behavior

### Incremental Matching

Search matches are found incrementally as you type:
- Searches from newest to oldest history entries
- Matches any substring within history entries
- Cursor positioned at the start of the matched text

### Cycling Through Matches

Press `Ctrl+R` again while in search mode to find older matches:

```
# History: "echo alpha", "echo beta"
# Type "echo" -> matches "echo beta" (most recent)
# Press Ctrl+R again -> matches "echo alpha" (older)
```

### No Match Behavior

When no match is found for the current query:
- The last successful match remains displayed
- The query shows what you typed
- Backspace to shorten the query and find matches again

### Double Ctrl+R

If you press `Ctrl+R` twice quickly (with empty query), the previous search string is reused:

```
# Previous search: "echo"
# Press Ctrl+R, then Ctrl+R again
# Automatically searches for "echo"
```

## Examples

### Basic Search

```
> Ctrl+R
(reverse-i-search)`': 

> Type "git"
(reverse-i-search)`git': git commit -m "fix bug"
```

### Narrowing Search

```
> Ctrl+R, type "cd"
(reverse-i-search)`cd': cd packages/tui

> Type " packages/c"
(reverse-i-search)`cd packages/c': cd packages/core
```

### Accept and Edit

```
> Ctrl+R, type "npm"
(reverse-i-search)`npm': npm run build

> Press Right Arrow (accept at match position)
npm run build
    ^ cursor here

> Edit to: npm run test
```

## Troubleshooting

### Search Not Finding Expected Entry

- History only includes commands from the current session
- Commands must have been submitted (Enter pressed)
- Search is case-sensitive

### Ctrl+R Not Working

- Ensure you're in the input prompt (not during AI response)
- Check that no selection menu is open

### Lost My Original Input

- If you had text before searching, `Ctrl+C` aborts and clears
- `Escape` accepts the match (original input is replaced)

## Technical Details

- History is stored per-session in memory
- Search uses simple substring matching
- Match position determines cursor placement after accept
- State is managed independently from main input buffer

## Related

- [Session Management](session-management.md) - Session save/load
- [Key Bindings Settings](../settings/key-bindings-settings.md) - Other keyboard shortcuts
