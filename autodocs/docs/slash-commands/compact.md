---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /compact
  description: Summarize conversation history to free context space while preserving essential information
  keywords: [compact, summarize, context, memory, space]
  related: [context]
---

# /compact

Summarize conversation history to free context space while preserving essential information.

## Overview

The `/compact` command creates an AI-generated summary of conversation history to free up context window space. Useful for long-running conversations approaching memory constraints. Preserves key information, code, and tool executions in summary.

## Usage

### Basic Usage

```
/compact
```

### With Custom Prompt

```
/compact Focus on technical decisions and code changes
```

### With Options

```
/compact --show-summary --messages-to-exclude 2
```

## How It Works

1. AI generates summary of conversation history
2. Retains key information, code, tool executions
3. Clears conversation history
4. Summary becomes context for future responses
5. Auto-compaction occurs when context window overflows

## Options

| Option | Description |
|--------|-------------|
| `--show-summary` | Display generated summary |
| `--messages-to-exclude <N>` | Exclude last N message pairs from summarization |
| `--truncate-large-messages` | Truncate large messages in history |
| `--max-message-length <N>` | Max message size (requires --truncate-large-messages) |

## When to Use

- Memory constraint warning appears
- Long-running conversation
- Before starting new topic in same session
- After completing complex tool operations

## Examples

### Example 1: Basic Compaction

```
/compact
```

**Output**:
```
Summarizing conversation...
✔ Conversation compacted. Summary created.
```

### Example 2: With Summary Display

```
/compact --show-summary
```

Shows generated summary after compaction.

### Example 3: Exclude Recent Messages

```
/compact --messages-to-exclude 3
```

Keeps last 3 message pairs, summarizes rest.

### Example 4: Custom Summary Focus

```
/compact Emphasize API design decisions and performance considerations
```

## Auto-Compaction

Automatic compaction occurs when context window overflows.

**Disable**:
```bash
kiro-cli settings chat.disableAutoCompaction true
```

## Troubleshooting

### Issue: Summary Too Generic

**Symptom**: Summary lacks important details  
**Cause**: Default summarization prompt  
**Solution**: Use custom prompt: `/compact Focus on X and Y`

### Issue: Recent Context Lost

**Symptom**: AI doesn't remember recent discussion  
**Cause**: Recent messages included in summary  
**Solution**: Use `--messages-to-exclude` to keep recent messages

### Issue: Auto-Compaction Unwanted

**Symptom**: Conversation compacted automatically  
**Cause**: Context window overflow  
**Solution**: Disable with `kiro-cli settings chat.disableAutoCompaction true`

## Related

- [/context](context.md) - View context usage
- [/clear](clear.md) - Clear conversation without summary
- [chat.disableAutoCompaction](../settings/disable-auto-compaction.md) - Disable auto-compaction

## Limitations

- Summary quality depends on AI
- Can't undo compaction
- Original messages lost after compaction
- Summary adds to context (though much smaller)

## Technical Details

**Default Strategy**:
- messages_to_exclude: 0
- truncate_large_messages: false
- max_message_length: MAX_USER_MESSAGE_SIZE

**Summary Generation**: Uses AI to create concise summary preserving key information.

**Auto-Compaction**: Triggered when context window reaches capacity.
