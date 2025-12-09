# Session Management

## Overview

Kiro CLI automatically saves all chat sessions on every conversation turn. You can resume from any previous chat session at any time.

## Managing Sessions

### From the Command Line

You can manage and resume sessions directly when starting Kiro CLI:

```bash
# Resume the most recent chat session
kiro-cli chat --resume

# Interactively pick a chat session to resume
kiro-cli chat --resume-picker

# List all saved chat sessions for the current directory
kiro-cli chat --list-sessions

# Delete a saved chat session
kiro-cli chat --delete-session <SESSION_ID>
```

### From Within a Chat Session

Your current chat is automatically saved as you interact. You can manage the current session using the `/chat` command:

```bash
# Resume a chat session (interactive selector)
/chat resume

# Save current session to a file
/chat save <FILE_PATH>

# Load a session from a file
/chat load <FILE_PATH>
```

The `.json` extension is optional when loading - Kiro will try both with and without the extension.

## Custom Chat Session Storage

You can use custom scripts to control where chat sessions are saved to and loaded from. This allows you to store sessions in version control systems, cloud storage, databases, or any custom location.

### Save Chat Session via Script

```bash
/chat save-via-script <SCRIPT_PATH>
```

Your script will receive the chat session JSON via stdin.

**Example: Save to Git Notes**

```bash
#!/bin/bash
set -ex

# Get current commit SHA
COMMIT=$(git rev-parse HEAD)

# Read JSON from stdin and save to temp file
TEMP=$(mktemp)
cat > "$TEMP"

# Add to git notes in kiro/notes namespace
git notes --ref=kiro/notes add -F "$TEMP" "$COMMIT" --force

# Clean up
rm "$TEMP"

# Show success
echo "Saved to commit ${COMMIT:0:8}" >&2
```

### Load Chat Session via Script

```bash
/chat load-via-script <SCRIPT_PATH>
```

Your script should output the chat session JSON to stdout.

**Example: Load from Git Notes**
```bash
#!/bin/bash
set -ex

# Get current commit SHA
COMMIT=$(git rev-parse HEAD)

# Load from git notes and output to stdout
git notes --ref=kiro/notes show "$COMMIT"
```

## Tips

- Session IDs are UUIDs that uniquely identify each chat session
- Sessions are stored per directory, so each project has its own set of sessions
- The most recently updated sessions appear first in the list