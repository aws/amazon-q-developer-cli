---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /reply
  description: Open $EDITOR with most recent assistant message quoted for reply
  keywords: [reply, editor, quote, response]
  related: [editor, paste]
---

# /reply

Open $EDITOR with most recent assistant message quoted for reply.

## Overview

Opens editor with last assistant message pre-filled as quoted text. Useful for responding to specific parts of assistant's response or providing detailed feedback.

## Usage

```
/reply
```

Opens editor with assistant's last message quoted.

## Related

- [/editor](editor.md) - Compose without quote
- [/paste](paste.md) - Paste from clipboard

## Limitations

- Requires $EDITOR or vi
- Not available in headless mode
- Quotes last assistant message only

## Technical Details

**Editor**: Uses $EDITOR, falls back to vi.

**Quote Format**: Assistant message prefixed with `>` for markdown quote.

## Examples

### Example 1: Reply to Response

```
/reply
```

Editor opens with:
```
> [Assistant's last message quoted here]

[Cursor here for your reply]
```

## Troubleshooting

### Issue: No Message to Reply To

**Symptom**: Error or empty editor  
**Cause**: No assistant message yet  
**Solution**: Wait for AI response first

### Issue: Editor Doesn't Open

**Symptom**: Command fails  
**Cause**: $EDITOR not set or invalid  
**Solution**: Set $EDITOR: `export EDITOR=vim`
