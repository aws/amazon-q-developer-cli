---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat list
  description: List all saved chat sessions for current directory
  keywords: [chat, list, sessions, saved]
  related: [chat-save, chat-load, chat-delete]
---

# kiro-cli chat --list-sessions

List all saved chat sessions for current directory.

## Overview

Shows all conversations saved in database for current directory with session IDs, timestamps, summaries, and message counts.

## Usage

```
kiro-cli chat --list-sessions
```

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
- [kiro-cli chat --delete-session <sessionId>](chat-delete.md) - Delete sessions
- [kiro-cli chat --list-sessions](../commands/chat.md) - CLI version

## Examples

### Example 1: List Sessions

```
kiro-cli chat --list-sessions
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
