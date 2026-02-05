---
name: acp-session-storage
description: Guide for analyzing and debugging ACP session data stored as JSONL logs. Use when debugging conversation history issues, compaction bugs, infinite loops, tool execution patterns, or investigating session state. Triggers on questions about session files, event logs, compaction behavior, or jq queries for session analysis.
---

# ACP Session Storage Analysis

## Session Storage Location

Sessions are stored in `~/.kiro/sessions/cli/` with three files per session:
- `{session_id}.json` - Session metadata (cwd, timestamps, state)
- `{session_id}.jsonl` - Append-only event log (conversation history)
- `{session_id}.lock` - Lock file (exists only when session is active)

## JSONL Log Structure

Each line is a `LogEntry` with structure: `{"version":"v1","kind":"<EventKind>","data":{...}}`

**Warning**: Entry data can be extremely large (e.g., large MCP tool use results). Avoid viewing full message content by default—use length queries first.

| Kind | Description | Key Fields |
|------|-------------|------------|
| `Prompt` | User message | `message_id`, `content` |
| `AssistantMessage` | Assistant response | `message_id`, `content` |
| `ToolResults` | Tool execution results | `message_id`, `content`, `results` |
| `Compaction` | Conversation summary | `summary`, `strategy`, `messages_snapshot` |
| `ResetTo` | Reset to previous point | `target_index` |
| `CancelledPrompt` | Cancelled user message | (no data) |
| `Clear` | Clear conversation | (no data) |

## Quick Reference jq Queries

```bash
# List all event kinds
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | sort | uniq -c

# View events with line numbers
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | cat -n

# List all tool uses
jq 'select(.kind == "AssistantMessage") | .data.content[] | select(.kind == "toolUse") | .data | {name, toolUseId}' ~/.kiro/sessions/cli/{session_id}.jsonl

# View compaction details
jq 'select(.kind == "Compaction") | {
  summary_length: (.data.summary | length),
  snapshot_count: (.data.messages_snapshot | length),
  strategy: .data.strategy
}' ~/.kiro/sessions/cli/{session_id}.jsonl

# Check message sizes in compaction snapshots
jq 'select(.kind == "Compaction") | .data.messages_snapshot | [.[] | {role, content_length: (.content | tostring | length)}]' ~/.kiro/sessions/cli/{session_id}.jsonl
```

## Debugging Compaction Issues

Multiple consecutive `Compaction` events indicate a potential infinite loop:
```bash
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | grep -n Compaction
```

## References

- [Schema Reference](references/schema.md) - Full schema for all event types
- [jq Queries](references/jq-queries.md) - Comprehensive jq query examples
