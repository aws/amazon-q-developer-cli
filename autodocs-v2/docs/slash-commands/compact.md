---
doc_meta:
  validated: 2026-04-09
  commit: 727bdf89
  status: validated
  testable_headless: true
  category: slash_command
  title: /compact
  description: Summarize conversation history to free context space while preserving essential information
  keywords: [compact, summarize, context, memory, history, compress]
  related: [clear, context]
---

## Overview

The `/compact` command creates an AI-generated summary of conversation history to free up context window space. Useful for long-running conversations approaching memory constraints. Preserves key information, code, and tool executions in the summary.

## Usage

```
/compact
```

## Examples

### Compact conversation

```
/compact
```

The assistant summarizes the conversation history, replacing older messages with a condensed summary while preserving recent context.

## How It Works

1. Takes a snapshot of the current conversation
2. Generates an AI summary of the message history
3. Replaces older messages with the summary
4. Keeps recent messages intact

## When to Use

- Context window usage is high (check with `/context`)
- Long conversation with many tool calls
- Want to continue working without starting a new session

## Troubleshooting

### "Conversation too short to compact"

The conversation doesn't have enough messages to warrant compaction. Continue working and try again later.

## Related

- [/clear](clear.md) — Erase all history (no summary)
- [/context](context.md) — View context window usage
- [/chat new](chat-new.md) — Start a fresh session
