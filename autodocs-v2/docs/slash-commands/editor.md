---
doc_meta:
  title: /editor
  description: Open $EDITOR to compose a multi-line prompt
  category: slash_command
  keywords: [editor, compose, multi-line, prompt, EDITOR, VISUAL]
  related: [reply]
  validated: 2026-04-08
  commit: 1a984cb0
  status: validated
  testable_headless: false
---

## Overview

The `/editor` command opens your system editor (`$VISUAL` or `$EDITOR`) to compose a prompt. Useful for writing multi-line messages, pasting large code blocks, or crafting detailed instructions.

The content you write is sent as a message when you save and close the editor.

## Usage

```
/editor
```

Opens an empty editor.

```
/editor <initial text>
```

Opens the editor pre-filled with the provided text.

## Examples

### Compose a multi-line prompt

```
/editor
```

Your editor opens with an empty `prompt.md` file. Write your message, save, and quit.

### Pre-fill with text

```
/editor Please review the following code changes
```

Opens the editor with the text already filled in for you to expand on.

## How It Works

1. Creates a temporary file (`kiro-editor-*.prompt.md`)
2. Opens it in `$VISUAL` or `$EDITOR` (falls back to `vi`)
3. On save and quit, sends the content as your message
4. Empty content is not submitted

## Troubleshooting

### Editor doesn't open

Set the `EDITOR` or `VISUAL` environment variable:

```bash
export EDITOR=vim
```

### Empty content not submitted

If you save an empty file or quit without saving, nothing is sent.

## Related

- [/reply](reply.md) — Open editor pre-filled with the last assistant message
