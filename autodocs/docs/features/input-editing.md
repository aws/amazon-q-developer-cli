---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: feature
  title: Input Editing
  description: Emacs-style keybindings for editing text in the chat prompt
  keywords: [keybindings, emacs, readline, input, editing, cursor, undo, kill, yank]
  related: [key-bindings-settings]
---

# Input Editing

Emacs-style keybindings for editing text in the chat prompt.

## Overview

Kiro CLI supports emacs-style keybindings for efficient text editing in the prompt. These keybindings work on logical lines (delimited by newlines) rather than visual wrapped lines.

## Keybindings Reference

### Cursor Movement

| Keybinding | Action |
|------------|--------|
| Ctrl+A | Move to beginning of logical line |
| Ctrl+E | Move to end of logical line |
| Ctrl+B | Move back one character |
| Ctrl+F | Move forward one character |
| Alt+F | Move forward one word |
| Alt+B | Move backward one word |

### Text Deletion

| Keybinding | Action |
|------------|--------|
| Ctrl+D | Delete character under cursor |
| Ctrl+K | Kill from cursor to end of logical line |
| Ctrl+U | Kill from cursor to beginning of logical line |
| Ctrl+W | Delete word backward |
| Alt+D | Delete word forward |

### Undo

| Keybinding | Action |
|------------|--------|
| Ctrl+_ | Undo last edit |

## Logical vs Visual Lines

Keybindings operate on logical lines (delimited by `\n`), not visual lines created by terminal wrapping.

Example with a 30-column terminal:
```
the quick brown fox jumps over|  <- visual wrap
the lazy dog                  |
```

- Ctrl+A moves to `t` in "the quick" (start of logical line)
- Ctrl+E moves to `g` in "dog" (end of logical line)
- Ctrl+K from middle kills to end of logical line, not visual line

## Word Movement Semantics

Word movement follows emacs semantics:

- **Alt+F** (forward-word): Stops right after the last letter of the word
- **Alt+B** (backward-word): Stops at the first letter of the word

Example with cursor at `|`:
```
|hello world
```

After Alt+F:
```
hello| world
```

After Alt+F again:
```
hello world|
```

## Kill Line Behavior

**Ctrl+K** at end of line joins with next line by deleting the newline:

```
hello|
world
```

After Ctrl+K:
```
hello|world
```

## Undo

Undo (Ctrl+_) restores previous input state. The undo stack:

- Saves state before destructive operations (kill, delete, insert)

Example:
```
> hello world
```

After Ctrl+A, Ctrl+K (kill line):
```
> |
```

After Ctrl+_ (undo):
```
> hello world|
```

## Examples

### Example 1: Quick Line Editing

```
> the quick brown fox|

Ctrl+A → |the quick brown fox
Ctrl+K → |
Type "hello" → hello|
```

### Example 2: Word Navigation

```
> hello world foo|

Alt+B → hello world |foo
Alt+B → hello |world foo
Alt+B → |hello world foo
```

### Example 3: Delete Word

```
> hello world foo|

Ctrl+W → hello world |
Ctrl+W → hello |
```

### Example 4: Multi-line Editing

```
> line one
  line two|

Ctrl+A → line one
         |line two
Ctrl+U → line one
         |
```

## Troubleshooting

### Issue: Alt Key Not Working

**Symptom**: Alt+F/B/D do nothing  
**Cause**: Terminal capturing Alt key  
**Solution**: Configure terminal to send Alt as Escape (Meta key)

### Issue: Ctrl+_ Not Working

**Symptom**: Undo doesn't work  
**Cause**: Terminal may not send 0x1F for Ctrl+_  
**Solution**: Check terminal keybinding configuration or remap the undo shortcut

### Issue: Keybinding Conflicts

**Symptom**: Keybinding does something unexpected  
**Cause**: Custom keybinding override  
**Solution**: Check `kiro-cli settings` for custom key settings

## Related

- [Key Bindings Settings](../settings/key-bindings-settings.md) - Configurable shortcuts
