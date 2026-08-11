---
doc_meta:
  validated: 2026-05-06
  commit: 004e29fd
  status: validated
  testable_headless: false
  category: slash_command
  title: /clear
  description: Clear the conversation history within the current session
  keywords: [clear, erase, reset, history, fresh]
  related: [compact, chat-new]
---

# /clear

Clear the conversation history within the current session.

## Overview

The `/clear` command clears all conversation history within the current session. Unlike `/compact`, does not create a summary - completely clears the conversation messages and resets state. Executes immediately without confirmation.

## Usage

```
/clear
```

## Examples

### Example 1: Clear Conversation

```
/clear
```

**Output**:
```
✔ Conversation cleared
```

## Related

- [/compact](compact.md) - Summarize before clearing
- [/tangent](tangent.md) - Temporary branch without clearing
- [/chat new](chat-new.md) - Start a new named session

## Limitations

- Cannot be undone
- Clears all history (no selective clearing)

## Technical Details

**What's Cleared**:
- All message history
- Hook-generated context
- Conversation metadata and tool state

**What's Preserved**:
- Current session (the session is kept, only messages are cleared)
- Agent configuration
- Tool permissions
- MCP connections
- Current model selection
