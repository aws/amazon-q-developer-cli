---
doc_meta:
  title: Keyboard Shortcuts
  description: Readline-style keyboard shortcuts for text editing in the chat prompt
  category: feature
  keywords: [keyboard, shortcuts, readline, emacs, editing, kill ring, yank, delete, navigation]
  related: [classic-vs-tui]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
---

# Keyboard Shortcuts

The TUI supports readline-style keyboard shortcuts for efficient text editing in the chat prompt.

## Overview

The chat prompt supports standard Emacs/readline keybindings for cursor movement, text deletion, and the kill ring. These shortcuts work in the main prompt input area.

## Cursor Movement

| Shortcut | Action |
|----------|--------|
| Ctrl+A | Move to beginning of line |
| Ctrl+E | Move to end of line |
| Ctrl+B | Move backward one character |
| Ctrl+F | Move forward one character |
| Alt+B | Move backward one word |
| Alt+F | Move forward one word |

## Text Deletion

| Shortcut | Action |
|----------|--------|
| Backspace | Delete character before cursor |
| Ctrl+H | Delete character before cursor (backspace alias) |
| Delete (fn+Delete on macOS) | Delete character after cursor |
| Ctrl+D | Delete character after cursor |
| Ctrl+W | Delete word backward |
| Alt+Backspace | Delete word backward |
| Alt+D | Delete word forward |
| Alt+Delete | Delete word forward |

## Kill Ring

The kill ring stores text deleted by "kill" commands, allowing you to paste (yank) it back later.

### Kill Commands

These commands delete text and save it to the kill ring:

| Shortcut | Action |
|----------|--------|
| Ctrl+K | Kill from cursor to end of line |
| Ctrl+U | Kill from cursor to beginning of line |
| Ctrl+W | Kill word backward |
| Alt+D | Kill word forward |

### Yank Commands

| Shortcut | Action |
|----------|--------|
| Ctrl+Y | Yank (paste) most recent kill |
| Alt+Y | Yank-pop: cycle through previous kills (use after Ctrl+Y) |

## Text Manipulation

| Shortcut | Action |
|----------|--------|
| Ctrl+T | Transpose characters |
| Alt+T | Transpose words |
| Alt+U | Uppercase word |
| Alt+L | Lowercase word |
| Alt+C | Capitalize word |

## Examples

### Basic Editing

Type a message, then use Ctrl+A to jump to the start:
```
hello world|     # cursor at end
|hello world     # after Ctrl+A, cursor at start
```

### Word Navigation

Navigate by word with Alt+F and Alt+B:
```
|hello world     # cursor at start
hello| world     # after Alt+F, cursor after "hello"
|hello world     # after Alt+B, cursor back at start
```

### Kill and Yank

Delete text and restore it:
```
hello world|     # cursor at end
hello |          # after Ctrl+W, "world" killed
hello world|     # after Ctrl+Y, "world" yanked back
```

### Kill Ring Cycling

Access previous kills with Alt+Y:
```
# Kill "first", then kill "second"
first second|
first |          # Ctrl+W kills "second"
|                # Ctrl+U kills "first "

# Yank and cycle
second|          # Ctrl+Y yanks "second" (most recent)
first |          # Alt+Y replaces with "first " (previous kill)
```

### Forward Delete

Delete character after cursor:
```
he|llo           # cursor after "he"
he|lo            # after Delete or Ctrl+D, "l" removed
```

## Troubleshooting

### Alt key not working

Some terminals intercept Alt key combinations. Try:
- Use Escape followed by the key (e.g., Esc then D for Alt+D)
- Configure your terminal to send Alt as Meta/Escape

### fn+Delete not working on macOS

Ensure you're pressing fn+Delete (forward delete), not just Delete (backspace). The Delete key alone acts as backspace on Mac keyboards.

### Kill ring empty

The kill ring only stores text from kill commands (Ctrl+K, Ctrl+U, Ctrl+W, Alt+D). Regular backspace/delete does not add to the kill ring.

## Limitations

- Kill ring is session-local and not persisted between sessions
- Alt+Y only works immediately after Ctrl+Y or another Alt+Y
- Some shortcuts may conflict with terminal emulator bindings

## Related

- [Classic vs TUI](classic-vs-tui.md) - Differences between classic mode and the TUI
