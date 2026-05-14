---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: slash_command
  description: Exit the chat session and return to terminal
  keywords: [quit, exit, close, leave]
  related: [chat-save]
---


Exit the chat session and return to terminal.

## Overview

The `/quit` command exits the current chat session. You can also use `/exit` as an alias.

## Usage

```
/quit
```

Or:

```
/exit
```



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
/exit
```

Same behavior as `/quit`.


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

## Related Features

- [/chat save](chat-save.md) - Save to file
- [kiro-cli chat --resume](../commands/chat.md) - Resume conversation

## Technical Details

**Auto-Save**: Conversation saved to database with current directory as key


**Keyboard**: Ctrl+C also exits (with confirmation if in middle of response)
