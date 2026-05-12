---
doc_meta:
  title: Paste Chips
  description: Large pasted text collapses into expandable chips to keep the prompt input readable
  category: feature
  keywords: [paste, chip, collapse, expand, tab, multiline, text, input]
  related: [file-references, editor]
  validated: 2026-05-04
  commit: 47e52126
  status: validated
  testable_headless: false
---

# Paste Chips

Large pasted text collapses into expandable chips to keep the prompt input readable.

## Overview

When you paste text with 10 or more lines into the prompt input, it automatically collapses into a compact chip showing the line count (e.g., "12 lines ▸"). This keeps your prompt readable while preserving the full content. Press Tab to expand the chip back into inline editable text.

## How It Works

1. Paste multiline text (10+ lines) into the prompt
2. Text collapses into a chip: `[12 lines ▸]`
3. Hint appears: "Press Tab to expand"
4. Continue typing around the chip
5. Press Tab when cursor is on chip to expand

The chip displays:
- Line count for multiline content (e.g., "12 lines ▸")
- Character count for single-line content (e.g., "150 chars ▸")
- Triangle indicator (▸) showing it's expandable

## Expanding Chips

Press **Tab** when your cursor is on a paste chip to expand it into inline text.

After expansion:
- The chip is replaced with the full pasted content
- Cursor moves to the end of the expanded text
- Content becomes editable inline

## Undo Support

Press **Ctrl+_** (undo) after expanding to restore the chip. This is useful if you accidentally expanded a large paste and want to collapse it again.

## Examples

### Example 1: Paste and Send

```
> [paste 15 lines of code]
[15 lines ▸] explain this code
```

The pasted code is sent as context without cluttering the visible prompt.

### Example 2: Expand Before Editing

```
> [paste error log]
[12 lines ▸]
> [press Tab]
line 1 of error
line 2 of error
...
```

Expand to edit or review the pasted content before sending.

### Example 3: Multiple Pastes

```
> Compare [12 lines ▸] with [8 lines ▸]
```

Multiple paste chips can coexist in the same prompt.

### Example 4: Undo Expansion

```
> [15 lines ▸]
> [press Tab - expands to 15 lines]
> [press Ctrl+_ - restores chip]
[15 lines ▸]
```

## Collapse Threshold

Text collapses into a chip when it has **10 or more lines**. Shorter pastes appear inline as regular text.

## Troubleshooting

### Issue: Paste Didn't Collapse

**Symptom**: Pasted text appears inline instead of as chip  
**Cause**: Text has fewer than 10 lines  
**Solution**: This is expected behavior. Only large pastes collapse.

### Issue: Tab Doesn't Expand

**Symptom**: Tab inserts completion instead of expanding  
**Cause**: Cursor not positioned on the chip  
**Solution**: Move cursor onto the chip, then press Tab.

### Issue: Lost Pasted Content

**Symptom**: Can't find pasted text  
**Cause**: Chip may have been deleted  
**Solution**: Use Ctrl+_ to undo, or paste again.

## Limitations

- Collapse threshold is fixed at 10 lines
- Cannot manually collapse expanded text back into a chip
- Chips cannot be edited without expanding first

## Related

- [File References](file-references.md) - Include file contents with @path syntax
- [/editor](../slash-commands/editor.md) - Compose longer prompts in external editor
