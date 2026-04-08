---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /clear
  description: Erase conversation history and context from current session
  keywords: [clear, erase, reset, history]
  related: [compact]
---

# /clear

Erase conversation history and context from current session.

## Overview

The `/clear` command erases all conversation history and context from hooks for the current session. Requires confirmation. Unlike `/compact`, does not create summary - completely clears conversation.

## Usage

```
/clear
```

Prompts for confirmation before clearing.

## Examples

### Example 1: Clear Conversation

```
/clear
```

**Output**:
```
Are you sure? This will erase the conversation history and context from hooks for the current session. [y/n]:

> y

✔ Conversation cleared
```

## Related

- [/compact](compact.md) - Summarize before clearing
- [/tangent](tangent.md) - Temporary branch without clearing

## Limitations

- Cannot be undone
- Clears all history (no selective clearing)
- Requires confirmation

## Technical Details

**Confirmation Required**: Prevents accidental clearing.

**What's Cleared**:
- All message history
- Hook-generated context
- Conversation state

**What's Preserved**:
- Agent configuration
- Tool permissions
- MCP connections
