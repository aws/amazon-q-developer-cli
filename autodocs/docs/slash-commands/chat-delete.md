---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat delete
  description: Delete saved chat session by ID
  keywords: [chat, delete, remove, session]
  related: [chat-list, chat-save]
---

# /chat delete

Delete saved chat session by ID.

## Overview

Deletes a saved conversation from database by session ID. Use `/chat list` to see available session IDs.

## Usage

```
/chat delete <session-id>
```

**Alternative**: `kiro-cli chat --delete-session <session-id>`

## Examples

### Example 1: Delete Session

```
/chat delete abc123
```

**Output**:
```
✔ Deleted chat session abc123
```

### Example 2: Delete from List

```
/chat list
```

Copy session ID, then:

```
/chat delete <session-id>
```

## Troubleshooting

### Issue: Session Not Found

**Symptom**: Error "Session not found"  
**Cause**: Invalid session ID or already deleted  
**Solution**: Use `/chat list` to verify session ID exists

### Issue: Can't Delete

**Symptom**: Delete fails  
**Cause**: Database error or permission issue  
**Solution**: Check database permissions. Try `kiro-cli chat --delete-session` CLI version

### Issue: Wrong Session Deleted

**Symptom**: Deleted wrong conversation  
**Cause**: Copied wrong ID  
**Solution**: Cannot undo. Be careful when copying session IDs from list

## Related

- [/chat list](chat-list.md) - List sessions to find IDs
- [kiro-cli chat --delete-session](../commands/chat.md) - CLI version
