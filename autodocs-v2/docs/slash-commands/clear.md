---
doc_meta:
  validated: 2026-05-06
  commit: 004e29fd
  status: validated
  testable_headless: true
  category: slash_command
  title: /clear
  description: Clear the conversation history within the current session
  keywords: [clear, erase, reset, history]
  related: [compact, chat-new]
---

# /clear

Clear the conversation history within the current session.

## Overview

The `/clear` command clears all conversation history within the current session. Unlike `/compact`, it does not create a summary - it completely resets the conversation messages and state while keeping the same session. Executes immediately without confirmation.

## Usage

```
/clear
```

Clears immediately without confirmation.

## Examples

### Example 1: Clear Conversation

```
/clear
```

**Output**:
```
Conversation cleared
```

### Example 2: Start Fresh After Complex Task

After completing a multi-step task with accumulated context:

```
/clear
```

This gives you a clean slate without the overhead of previous conversation history, while remaining in the same session.

### Example 3: Reset When Context Gets Confused

If the assistant seems confused by earlier context:

```
/clear
```

Clearing conversation history can help when accumulated context is causing issues.

## Related

- [/compact](compact.md) - Summarize and compact conversation without clearing
- [/chat new](chat-new.md) - Start a new named conversation

## Limitations

- Cannot be undone
- Clears all history (no selective clearing)

## Technical Details

**What Happens**:
- All message history is cleared within the current session
- Conversation metadata and tool state are reset
- The UI is reset to a fresh state

**What's Preserved**:
- The current session (no new session is created)
- Agent configuration
- Model selection
- Tool permissions
- MCP connections

## Troubleshooting

### Clear Failed

If `/clear` fails with an error, it may indicate a connection issue with the backend. Try:

1. Check your network connection
2. Wait a moment and retry
3. If persistent, restart the CLI
