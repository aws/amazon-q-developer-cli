---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat delete
  description: Delete saved chat session by ID
  keywords: [chat, delete, remove, session]
  related: [chat-list, chat-save]
---

# kiro-cli chat --delete-session

Delete saved chat session by ID.

## Overview

Deletes a saved conversation from database by session ID. Use `kiro-cli chat --list-sessions` to see available session IDs.

## Usage

```
kiro-cli chat --delete-session <session-id>
```

## Examples

### Example 1: Delete Session

```
kiro-cli chat --delete-session abc123
```

**Output**:
```
✔ Deleted chat session abc123
```

### Example 2: Delete from List

```
kiro-cli chat --list-sessions
```

Copy session ID, then:

```
kiro-cli chat --delete-session <session-id>
```

## Troubleshooting

### Issue: Session Not Found

**Symptom**: Error "Session not found"  
**Cause**: Invalid session ID or already deleted  
**Solution**: Use `kiro-cli chat --list-sessions` to verify session ID exists

### Issue: Can't Delete

**Symptom**: Delete fails  
**Cause**: Database error or permission issue  
**Solution**: Check database permissions. Try `kiro-cli chat --delete-session` CLI version

### Issue: Wrong Session Deleted

**Symptom**: Deleted wrong conversation  
**Cause**: Copied wrong ID  
**Solution**: Cannot undo. Be careful when copying session IDs from list

## Related

- [kiro-cli chat --list-sessions](chat-list.md) - List sessions to find IDs
- [kiro-cli chat --delete-session](../commands/chat.md) - CLI version
