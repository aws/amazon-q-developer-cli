---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: feature
  title: Session Management
  description: Automatic session saving, resumption, and file-based storage
  keywords: [session, save, load, resume, auto-save, storage, KIRO_SESSION_ID, environment]
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

Shows list of sessions to choose from.

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

## Troubleshooting

### Issue: No Sessions to Resume

**Symptom**: "No saved chat sessions"  
**Cause**: No sessions in current directory  
**Solution**: Sessions are per-directory. Navigate to correct directory.

## Related

- [/chat save](../slash-commands/chat-save.md) - Save command
- [/chat load](../slash-commands/chat-load.md) - Load command
- [kiro-cli chat](../commands/chat.md) - CLI options

## Limitations

- Sessions stored per-directory
- Auto-save to files only (not cloud)
- Session IDs are UUIDs (not human-readable)
- No cloud sync built-in
- No session search by content

## Technical Details

**Storage**: File-based in `~/.kiro/sessions/cli/`

**Scope**: Sessions keyed by directory path

**Auto-Save**: After every conversation turn

**Model Preservation**: When resuming via `--resume`, the model active when the session was saved is restored. This includes models switched mid-session with `/model`. The `--model` CLI flag overrides the saved model.
