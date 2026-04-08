---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /quit
  description: Exit the chat session and return to terminal
  keywords: [quit, exit, close, leave]
  related: [chat-save]
---

# /quit

Exit the chat session and return to terminal.

## Overview

The `/quit` command exits the current chat session and returns to the terminal. Conversation is automatically saved to database before exiting.

## Usage

```
/quit
```

**Aliases**: `/q`, `/exit`

## Behavior

1. Auto-saves conversation to database (per-directory)
2. Closes chat session
3. Returns to terminal

## Examples

### Example 1: Exit Session

```
/quit
```

Session ends, returns to terminal.

### Example 2: Using Alias

```
/q
```

Same as `/quit`.

## Auto-Save

Conversations automatically saved to database before exit. Resume with:

```bash
kiro-cli chat --resume
```

Or:

```
/chat resume
```

## Troubleshooting

### Issue: Want to Save to File

**Symptom**: Need explicit file save  
**Cause**: Auto-save only to database  
**Solution**: Use `/chat save <path>` before `/quit`

## Related Features

- [/chat save](chat-save.md) - Save to file
- [kiro-cli chat --resume](../commands/chat.md) - Resume conversation

## Technical Details

**Auto-Save**: Conversation saved to database with current directory as key

**Aliases**: `/q`, `/exit`

**Keyboard**: Ctrl+C also exits (with confirmation if in middle of response)
