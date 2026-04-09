---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
  category: slash_command
  title: /clear
  description: Erase conversation history and context from current session
  keywords: [clear, erase, reset, history]
  related: [compact]
---

# /clear

Erase conversation history and context from current session.

## Overview

The `/clear` command erases all conversation history and context for the current session. Unlike `/compact`, does not create a summary - completely clears the conversation.

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

## Related

- [/compact](compact.md) - Summarize and compact conversation without clearing

## Limitations

- Cannot be undone
- Clears all history (no selective clearing)

## Technical Details

**What's Cleared**:
- All message history
- Conversation metadata
- Tool state

**What's Preserved**:
- Agent configuration
- Tool permissions
- MCP connections
