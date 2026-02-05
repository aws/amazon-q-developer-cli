# jq Queries for Session Analysis

Replace `{session_id}` with the actual session UUID.

## Basic Queries

### List all event kinds in a session
```bash
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | sort | uniq -c
```

### View events with line numbers
```bash
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | cat -n
```

### View the last N events
```bash
tail -n 10 ~/.kiro/sessions/cli/{session_id}.jsonl | jq .
```

### Pretty print entire session
```bash
jq . ~/.kiro/sessions/cli/{session_id}.jsonl
```

## User & Assistant Messages

### View all user prompts
```bash
jq 'select(.kind == "Prompt")' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### View all assistant messages (text only)
```bash
jq 'select(.kind == "AssistantMessage") | .data.content[] | select(.kind == "text") | .data' ~/.kiro/sessions/cli/{session_id}.jsonl
```

## Tool Analysis

### List all tool uses
```bash
jq 'select(.kind == "AssistantMessage") | .data.content[] | select(.kind == "toolUse") | .data | {name, toolUseId}' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### Count tool uses by name
```bash
jq -r 'select(.kind == "AssistantMessage") | .data.content[] | select(.kind == "toolUse") | .data.name' ~/.kiro/sessions/cli/{session_id}.jsonl | sort | uniq -c
```

### View tool results for a specific tool use ID
```bash
jq --arg id "tooluse_xxx" 'select(.kind == "ToolResults") | .data.results[$id]' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### Find failed tool executions
```bash
jq 'select(.kind == "ToolResults") | .data.results | to_entries[] | select(.value.result | has("Error") or . == "Cancelled")' ~/.kiro/sessions/cli/{session_id}.jsonl
```

## Compaction Analysis

### View compaction events
```bash
jq 'select(.kind == "Compaction")' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### View compaction summaries
```bash
jq 'select(.kind == "Compaction") | .data.summary' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### View compaction strategy used
```bash
jq 'select(.kind == "Compaction") | .data.strategy' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### Count messages in each compaction snapshot
```bash
jq 'select(.kind == "Compaction") | .data.messages_snapshot | length' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### Get message count by role (from compaction snapshots)
```bash
jq 'select(.kind == "Compaction") | .data.messages_snapshot | group_by(.role) | map({role: .[0].role, count: length})' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### Compare message counts before/after compaction
```bash
jq 'select(.kind == "Compaction") | {
  summary_length: (.data.summary | length),
  snapshot_count: (.data.messages_snapshot | length),
  strategy: .data.strategy
}' ~/.kiro/sessions/cli/{session_id}.jsonl
```

### Check message sizes in compaction snapshots
```bash
jq 'select(.kind == "Compaction") | .data.messages_snapshot | [.[] | {role, content_length: (.content | tostring | length)}]' ~/.kiro/sessions/cli/{session_id}.jsonl
```

## Debugging Compaction Issues

### Check for multiple consecutive compactions (potential loop indicator)
```bash
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | grep -n Compaction
```

### View sequence of events around compaction
```bash
# Get line numbers of compactions
grep -n '"kind":"Compaction"' ~/.kiro/sessions/cli/{session_id}.jsonl
# Then use sed to view lines around it:
sed -n '45,55p' ~/.kiro/sessions/cli/{session_id}.jsonl | jq .
```

### Check if compaction is reducing messages
```bash
jq -r '.kind' ~/.kiro/sessions/cli/{session_id}.jsonl | awk '
  /Compaction/ { if (count > 0) print "Events before compaction:", count; count = 0; next }
  { count++ }
  END { if (count > 0) print "Events after last compaction:", count }
'
```

## Session Metadata

View session metadata (not the log):
```bash
jq . ~/.kiro/sessions/cli/{session_id}.json
```

Key fields:
- `session_id` - Unique identifier
- `cwd` - Working directory when session was created
- `created_at` / `updated_at` - Timestamps
- `session_state` - Contains conversation metadata and RTS model state
