---
doc_meta:
  validated: 2026-04-30
  commit: be2c1347
  status: validated
  testable_headless: false
  category: feature
  title: Session Management
  description: Automatic session saving, resumption, and file-based storage
  keywords: [session, save, load, resume, auto-save, storage, history, conversation, KIRO_SESSION_ID, environment]
  related: [chat-save, chat-load, chat, hooks]
---

# Session Management

Automatic session saving, resumption, and file-based storage.

## Overview

Kiro CLI automatically saves all chat sessions on every conversation turn. Sessions are stored per-directory as files. Resume from any previous session or export to portable files.

## Auto-Save

**Automatic**: Every conversation turn saved  
**Scope**: Per-directory (each project has own sessions)  
**Storage**: `~/.kiro/sessions/cli/`
- `{session_id}.json` - metadata (cwd, timestamps, session state)
- `{session_id}.jsonl` - append-only conversation log
- `{session_id}.lock` - lock file (exists only when session is active)

**Session ID**: UUID for each session

## Environment Variable

Kiro sets the `KIRO_SESSION_ID` environment variable when a session starts. All child processes inherit this value:

- Shell commands executed by the agent
- Hook scripts
- MCP servers
- AWS CLI calls

This allows child processes to detect they're running inside a Kiro session and correlate activity back to a specific session for logging or telemetry.

```bash
# In a hook script or shell command
echo "Running in Kiro session: $KIRO_SESSION_ID"
```

The variable is updated when:
- A new chat session starts
- `/chat new` creates a new session
- `/chat load` loads a saved session

## Managing Sessions

### From Command Line

```bash
# Resume most recent session
kiro-cli chat --resume

# Interactive picker
kiro-cli chat --resume-picker

# Resume specific session by ID
kiro-cli chat --resume-id f2946a26-3735-4b08-8d05-c928010302d5

# List all sessions
kiro-cli chat --list-sessions

# Delete session
kiro-cli chat --delete-session <SESSION_ID>
```

### From Chat

```bash
# Save to file
/chat save <path>

# Load from file
/chat load <path>

# Start fresh session
/chat new
```

**Note**: `.json` extension optional when loading.

## Session Storage

**Files**: Sessions auto-saved per-directory to `~/.kiro/sessions/cli/`  
**Export**: Manual export via `/chat save`

**Session ID**: UUID format (e.g., `f2946a26-3735-4b08-8d05-c928010302d5`)

## Examples

### Example 1: Resume Last Session

```bash
kiro-cli chat --resume
```

Continues most recent conversation, restoring the model that was active when the session was saved.

### Example 2: Pick Session

```bash
kiro-cli chat --resume-picker
```

Shows list of sessions to choose from. Displays session title, age, and message count.

### Example 3: Export to File

```
/chat save backup.json
```

Exports current session to file.

### Example 4: Save and Load Workflow

```bash
# Save current session
/chat save ./backup.json

# Later, load it back
/chat load ./backup.json
```

### Example 5: Share Session Across Machines

```bash
# On machine A: export session
/chat save ~/shared/my-session.json

# On machine B: import session
/chat load ~/shared/my-session.json
```

The loaded session gets a new UUID but preserves conversation history.

## FAQ

### How do I name or rename a session?

Sessions are identified by auto-generated UUIDs and auto-titled based on conversation content. You cannot manually rename sessions. To organize sessions:
- Use `/chat save meaningful-name.json` to export with a descriptive filename
- Use `--resume-picker` to see session titles when selecting

### How do I search past conversations by content?

Content search is not built-in. Workarounds:
- Sessions are stored as `.jsonl` files in `~/.kiro/sessions/cli/`
- Use grep to search: `grep -r "search term" ~/.kiro/sessions/cli/*.jsonl`
- Export important sessions with `/chat save` to searchable locations

### How do I share context across sessions?

Sessions are independent. To share context:
- Export relevant context to a file and reference it in new sessions
- Use agent `resources` to load common context files automatically
- Use `/chat save` and `/chat load` to continue the same conversation

### How do I centralize conversations across workspaces?

Sessions are per-directory by design. To work around this:
- Export sessions with `/chat save` to a central location
- Load them with `/chat load` from any directory
- Note: The loaded session runs in the current directory context

## Troubleshooting

### Issue: No Sessions to Resume

**Symptom**: "No saved chat sessions"  
**Cause**: No sessions in current directory  
**Solution**: Sessions are per-directory. Navigate to correct directory or use `/chat load` with a path.

### Issue: Session Shows Wrong Directory

**Symptom**: Resumed session references files from different directory  
**Cause**: Session was created in a different directory  
**Solution**: Sessions are tied to their original directory. Start a new session or navigate to the original directory.

## Related

- [/chat save](../slash-commands/chat-save.md) - Save command
- [/chat load](../slash-commands/chat-load.md) - Load command
- [/chat new](../slash-commands/chat-new.md) - Start new session
- [kiro-cli chat](../commands/chat.md) - CLI options

## Limitations

- Sessions stored per-directory (by design for project isolation)
- Session IDs are UUIDs (auto-titled based on content)
- No built-in content search (use grep on session files)
- No cloud sync (use `/chat save` to export for manual sync)
- No cross-session context sharing (sessions are independent)

## Technical Details

**Storage**: File-based in `~/.kiro/sessions/cli/`

**Scope**: Sessions keyed by directory path

**Auto-Save**: After every conversation turn

**Model Preservation**: When resuming via `--resume`, the model active when the session was saved is restored. This includes models switched mid-session with `/model`. The `--model` CLI flag overrides the saved model.
