---
doc_meta:
  validated: 2025-12-22
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: Session Management
  description: Automatic session saving, resumption, and custom storage via scripts
  keywords: [session, save, load, resume, auto-save, storage]
  related: [chat-save, chat-load, cmd-chat]
---

# Session Management

Automatic session saving, resumption, and custom storage via scripts.

## Overview

Kiro CLI automatically saves all chat sessions on every conversation turn. Sessions are stored per-directory in the database. Resume from any previous session, export to files, or use custom scripts for version control/cloud storage integration.

## Auto-Save

**Automatic**: Every conversation turn saved to database  
**Scope**: Per-directory (each project has own sessions)  
**Storage**: Local database (`~/.kiro/`)  
**Session ID**: UUID for each session

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
# Resume session (interactive)
/chat resume

# Save to file
/chat save <path>

# Load from file
/chat load <path>
```

**Note**: `.json` extension optional when loading.

## Custom Storage via Scripts

Use custom scripts to save/load sessions from version control, cloud storage, or databases.

### Save via Script

```bash
/chat save-via-script <script-path>
```

Script receives session JSON via stdin.

**Example: Save to Git Notes**
```bash
#!/bin/bash
COMMIT=$(git rev-parse HEAD)
TEMP=$(mktemp)
cat > "$TEMP"
git notes --ref=kiro/notes add -F "$TEMP" "$COMMIT" --force
rm "$TEMP"
echo "Saved to commit ${COMMIT:0:8}" >&2
```

### Load via Script

```bash
/chat load-via-script <script-path>
```

Script outputs session JSON to stdout.

**Example: Load from Git Notes**
```bash
#!/bin/bash
COMMIT=$(git rev-parse HEAD)
git notes --ref=kiro/notes show "$COMMIT"
```

## Session Storage

**Database**: Sessions auto-saved per-directory  
**Files**: Manual export via `/chat save`  
**Custom**: Script-based integration

**Session ID**: UUID format (e.g., `f2946a26-3735-4b08-8d05-c928010302d5`)

## Examples

### Example 1: Resume Last Session

```bash
kiro-cli chat --resume
```

Continues most recent conversation.

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

### Example 4: Version Control Integration

```bash
# Save to git notes
/chat save-via-script ./scripts/save-to-git.sh

# Load from git notes
/chat load-via-script ./scripts/load-from-git.sh
```

## Troubleshooting

### Issue: No Sessions to Resume

**Symptom**: "No saved chat sessions"  
**Cause**: No sessions in current directory  
**Solution**: Sessions are per-directory. Navigate to correct directory.

### Issue: Script Save Fails

**Symptom**: Script exits with error  
**Cause**: Script returned non-zero exit code  
**Solution**: Test script manually. Ensure it exits 0 on success.

### Issue: Script Load Fails

**Symptom**: Can't load session  
**Cause**: Script didn't output valid JSON  
**Solution**: Test script outputs valid session JSON to stdout.

## Related

- [/chat save](../slash-commands/chat-save.md) - Save command
- [/chat load](../slash-commands/chat-load.md) - Load command
- [kiro-cli chat](../commands/chat.md) - CLI options

## Limitations

- Sessions stored per-directory
- Auto-save to database only (not files)
- Session IDs are UUIDs (not human-readable)
- No cloud sync (use scripts for custom storage)
- No session search by content

## Technical Details

**Storage**: SQLite database in `~/.kiro/`

**Scope**: Sessions keyed by directory path

**Auto-Save**: After every conversation turn

**Script Interface**: 
- Save: JSON via stdin, exit 0 on success
- Load: JSON via stdout, exit 0 on success
