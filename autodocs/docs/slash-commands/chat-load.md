---
doc_meta:
  validated: 2026-01-27
  commit: 85403a86
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat load
  description: Load previously saved conversation from file to resume session
  keywords: [chat, load, import, restore, resume, session]
  related: [chat-save, chat-list, cmd-chat]
---

# /chat load

Load previously saved conversation from file to resume session.

## Overview

The `/chat load` command imports a conversation from a JSON file saved with `/chat save`. Restores message history, context files, and conversation state. Automatically requests summary of conversation after loading.

## Usage

### Basic Usage

```
/chat load <path>
```

**Aliases**: `/load` (deprecated - use `/chat load`)

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `path` | Yes | Path to saved conversation file |

## How It Works

Loads conversation JSON file and restores:
- Message history
- Context file paths (as temporary context)
- Conversation state

Preserves current session's:
- Tool manager
- MCP connections
- Model configuration
- Agent configuration

After loading, automatically asks "In a few words, summarize our conversation so far" to provide context.

## Examples

### Example 1: Load from File

```
/chat load my-session.json
```

**Output**:
```
✔ Imported chat session state from my-session.json

[AI provides conversation summary]
```

### Example 2: Load with .json Extension Auto-Added

```
/chat load backup
```

Tries `backup` first, then `backup.json` if not found.

### Example 3: Load from Path

```
/chat load ~/backups/important-conversation.json
```

## Advanced: Script-Based Load

Load via custom script that outputs JSON to stdout:

```
/chat load-via-script ./my-load-script.sh
```

Script should output conversation JSON to stdout and exit 0 on success.

## Context File Handling

Context files from loaded conversation are added as **temporary context** (session-scoped). They don't override agent's permanent context files.

If context file path doesn't exist, it's still added but won't be readable.

## Troubleshooting

### Issue: File Not Found

**Symptom**: "Failed to import" error  
**Cause**: File doesn't exist at path  
**Solution**: Check path. Command tries adding `.json` extension automatically.

### Issue: Invalid JSON

**Symptom**: "Failed to import" error with JSON parse error  
**Cause**: File is not valid conversation JSON  
**Solution**: Ensure file was created with `/chat save`

### Issue: Context Files Missing

**Symptom**: Loaded but context files not working  
**Cause**: Context file paths from saved session don't exist  
**Solution**: Context files are referenced by path. Ensure files exist at same paths.

### Issue: Different Agent Behavior

**Symptom**: Agent behaves differently after load  
**Cause**: Current session's agent used, not saved agent  
**Solution**: Switch to desired agent before loading, or after loading

## Related Features

- [/chat save](chat-save.md) - Save conversation to file
- [/chat resume](chat-list.md) - Resume from database
- [kiro-cli chat --resume](../commands/chat.md) - Resume last conversation
- [/context](context.md) - Manage context files

## Limitations

- Uses current session's agent (not saved agent)
- Uses current session's model (not saved model)
- Context files added as temporary (not permanent)
- Hooks not restored (uses current agent's hooks)
- Tool manager recreated (not restored)
- MCP connections reestablished

## Technical Details

**File Format**: JSON with ConversationState structure

**State Restoration**:
- ✅ Message history
- ✅ Context file paths (as temporary)
- ✅ Conversation metadata
- ❌ Tool manager (uses current)
- ❌ MCP state (reconnects)
- ❌ Model info (uses current)
- ❌ Agent config (uses current)
- ❌ Hooks (uses current)

**Auto-Summary**: After loading, automatically triggers conversation summary to provide context.

**Extension Handling**: If path doesn't end with `.json` and file not found, automatically tries with `.json` appended.

**Deprecated**: `/load` command deprecated in favor of `/chat load`
