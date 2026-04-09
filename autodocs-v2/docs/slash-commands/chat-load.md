---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat load
  description: Load previously saved conversation from file to resume session
  keywords: [chat, load, import, restore, resume, session, zip, json]
  related: [chat-save, chat-new]
---

# /chat load

Load previously saved conversation from file to resume session.

## Overview

The `/chat load` command imports a conversation from a saved session file. Supports multiple file formats including the native Kiro export format, zip archives, and legacy V1 sessions.

## Usage

```
/chat load <path>
```

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | Yes | Path to saved session file (.json or .zip) |

## Supported Formats

| Format | Description |
|--------|-------------|
| Kiro | `kiro-session-export-v1` JSON (created by `/chat save`) |
| Zip | Archive containing `session_metadata.json` + `conversation_log.jsonl` |
| Legacy | V1 `ConversationState` JSON from older CLI versions |
| SessionDataOnly | Bare `SessionData` JSON with optional companion `.jsonl` file |

## How It Works

1. Reads the file and auto-detects format
2. Extracts session metadata and conversation log
3. Creates a new session with a fresh UUID
4. Sets `imported_from` field to track origin

The loaded session uses your current:
- Agent configuration
- Model configuration
- MCP connections
- Hooks

## Examples

### Example 1: Load from JSON File

```
/chat load my-session.json
```

**Output**:
```
Loaded session from /home/user/my-session.json
```

### Example 2: Load from Zip Archive

```
/chat load backup.zip
```

### Example 3: Load with Extension Fallback

```
/chat load backup
```

Tries `backup` first, then `backup.zip`, then `backup.json` if not found.

### Example 4: Load from Absolute Path

```
/chat load ~/backups/important-conversation.json
```

## Troubleshooting

### Issue: File Not Found

**Symptom**: "Failed to read" error  
**Cause**: File doesn't exist at path  
**Solution**: Check path. Command tries `.zip` then `.json` extensions automatically.

### Issue: Unrecognized Format

**Symptom**: "Unrecognized session file format" error  
**Cause**: File is not a valid session export  
**Solution**: Ensure file was created with `/chat save` or is a valid legacy export.

### Issue: Invalid Zip Archive

**Symptom**: "File has .zip extension but is not a valid zip archive"  
**Cause**: Corrupted or invalid zip file  
**Solution**: Re-export the session or use JSON format.

## Related Features

- [/chat save](chat-save.md) - Save conversation to file
- [/chat new](chat-new.md) - Start a new conversation
- [kiro-cli chat --resume](../commands/chat.md) - Resume last conversation

## Technical Details

**Extension Handling**: If path has no extension and file not found, tries `.zip` first, then `.json`.

**Session ID**: Loaded sessions get a new UUID. The original path is stored in `imported_from`.

**Format Detection**: Uses file extension for `.zip`, then probes JSON structure for other formats.
