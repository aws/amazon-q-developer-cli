---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat save
  description: Save current conversation to file or database for later resumption
  keywords: [chat, save, export, persist, session]
  related: [chat-load, chat-list, cmd-chat]
---

# /chat save

Save current conversation to file or database for later resumption.

## Overview

The `/chat save` command exports the current conversation state to a JSON file. Conversations are automatically saved to the database per-directory, but this command allows explicit file exports for backup, sharing, or custom storage.

## Usage

### Basic Usage

```
/chat save <path>
```

### With Force Overwrite

```
/chat save <path> --force
```

**Aliases**: `/save` (deprecated - use `/chat save`)

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | Yes | File path where conversation will be saved |
| `--force`, `-f` | No | Overwrite existing file without prompting |

## How It Works

Exports current conversation state as JSON including:
- Complete message history
- Agent configuration
- Context files
- Tool manager state
- Model information

File can be loaded later with `/chat load` to restore exact conversation state.

## Examples

### Example 1: Save to File

```
/chat save my-session.json
```

**Output**:
```
✔ Exported chat session state to my-session.json
To restore this session later, use: /chat load my-session.json
```

### Example 2: Overwrite Existing File

```
/chat save backup.json --force
```

### Example 3: Save with Path

```
/chat save ~/backups/important-conversation.json
```

## Advanced: Script-Based Save

Save via custom script that receives JSON via stdin:

```
/chat save-via-script ./my-save-script.sh
```

Script receives conversation JSON on stdin and should exit 0 on success.

## Troubleshooting

### Issue: File Already Exists

**Symptom**: Error "File already exists"  
**Cause**: File at path already exists and --force not used  
**Solution**: Use `--force` flag or choose different path

### Issue: Permission Denied

**Symptom**: "Failed to export" error  
**Cause**: No write permission for path  
**Solution**: Check directory permissions or use different path

### Issue: Invalid Path

**Symptom**: Export fails  
**Cause**: Parent directory doesn't exist  
**Solution**: Create parent directories first or use existing path

## Related Features

- [/chat load](chat-load.md) - Load saved conversation
- [/chat list](chat-list.md) - List saved conversations
- [kiro-cli chat --list-sessions](../commands/chat.md) - List database sessions
- [kiro-cli chat --delete-session](../commands/chat.md) - Delete session

## Limitations

- Saves to file, not database (use auto-save for database)
- File format is JSON (not human-readable)
- Large conversations create large files
- No compression or encryption
- Context files referenced by path (not embedded)

## Technical Details

**File Format**: JSON with ConversationState structure

**Auto-Save**: Conversations automatically saved to database per-directory. This command is for explicit file exports.

**State Preserved**:
- Message history
- Agent configuration
- Context file paths
- Tool manager state
- Model information
- MCP server state

**State Not Preserved**:
- Tool manager instance (recreated on load)
- MCP connections (reconnected on load)
- Model info (uses current session's model)

**Deprecated**: `/save` command deprecated in favor of `/chat save`
