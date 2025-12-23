---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat list
  description: List all saved chat sessions for current directory
  keywords: [chat, list, sessions, saved]
  related: [chat-save, chat-load, chat-delete]
---

# /chat list

List all saved chat sessions for current directory.

## Overview

Shows all conversations saved in database for current directory with session IDs, timestamps, summaries, and message counts.

## Usage

```
/chat list
```

**Alternative**: `kiro-cli chat --list-sessions`

## Output

```
Chat sessions for /path/to/project:

Chat SessionId: abc123
  2 hours ago | Implement user authentication | 15 msgs

Chat SessionId: def456
  1 day ago | Refactor database layer | 23 msgs

To delete a session, use: kiro-cli chat --delete-session <SESSION_ID>
```

## Related

- [/chat save](chat-save.md) - Save conversations
- [/chat load](chat-load.md) - Load conversations
- [/chat delete](chat-delete.md) - Delete sessions
- [kiro-cli chat --list-sessions](../commands/chat.md) - CLI version

## Examples

### Example 1: List Sessions

```
/chat list
```

**Output**:
```
Chat sessions for /Users/me/project:

Chat SessionId: abc123
  2 hours ago | Implement authentication | 15 msgs

Chat SessionId: def456
  1 day ago | Refactor database | 23 msgs
```

## Troubleshooting

### Issue: No Sessions Shown

**Symptom**: "No saved chat sessions"  
**Cause**: No conversations saved in current directory  
**Solution**: Sessions are per-directory. Use `/chat save` to save.

### Issue: Can't Find Old Session

**Symptom**: Expected session not in list  
**Cause**: Session from different directory  
**Solution**: Navigate to that directory first
