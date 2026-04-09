---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat save
  description: Save current conversation to file for later resumption
  keywords: [chat, save, export, persist, session]
  related: [chat-load, chat-new]
---

# /chat save

Save current conversation to file for later resumption.

## Overview

The `/chat save` command exports the current conversation state to a JSON file. Conversations are automatically saved to the database per-directory, but this command allows explicit file exports for backup, sharing, or custom storage.

## Usage

```
/chat save <path>
/chat save <path> --force
```

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | Yes | File path where conversation will be saved |
| `--force`, `-f` | No | Overwrite existing file without prompting |

## How It Works

Exports current conversation state as `kiro-session-export-v1` JSON including:
- Session metadata (session ID, working directory, timestamps, title)
- Complete log entries (message history)

File can be loaded later with `/chat load` to restore the conversation.

## Examples

### Example 1: Save to File

```
/chat save my-session.json
```

**Output**:
```
Saved session to my-session.json
```

### Example 2: Save Without Extension

If no extension is provided, `.json` is automatically added:

```
/chat save my-session
```

Saves to `my-session.json`.

### Example 3: Overwrite Existing File

```
/chat save backup.json --force
```

### Example 4: Save with Path

```
/chat save ~/backups/important-conversation.json
```

## Troubleshooting

### Issue: File Already Exists

**Symptom**: Error "File already exists"  
**Cause**: File at path already exists and --force not used  
**Solution**: Use `--force` flag or choose different path

### Issue: Permission Denied

**Symptom**: "Failed to write" error  
**Cause**: No write permission for path  
**Solution**: Check directory permissions or use different path

### Issue: Invalid Path

**Symptom**: Export fails  
**Cause**: Parent directory doesn't exist  
**Solution**: Create parent directories first or use existing path

## Related Features

- [/chat load](chat-load.md) - Load saved conversation
- [/chat new](chat-new.md) - Start fresh conversation
- [kiro-cli chat](../commands/chat.md) - CLI session management

## Technical Details

**File Format**: `kiro-session-export-v1` JSON envelope

**Structure**:
```json
{
  "format": "kiro-session-export-v1",
  "metadata": {
    "session_id": "...",
    "cwd": "/path/to/dir",
    "created_at": "2026-01-01T00:00:00Z",
    "updated_at": "2026-01-01T00:00:00Z",
    "title": "Session Title",
    "session_state": "..."
  },
  "log_entries": [...]
}
```

**State Preserved**:
- Session ID
- Working directory (cwd)
- Created/updated timestamps
- Session title
- Log entries (complete message history)

**State Not Preserved**:
- Tool manager instance (recreated on load)
- MCP connections (reconnected on load)
- Agent configuration (uses current session's config)

**Auto-Extension**: `.json` extension added automatically if no extension provided.
