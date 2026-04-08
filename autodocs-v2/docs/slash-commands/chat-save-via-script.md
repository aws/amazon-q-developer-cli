---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat save-via-script
  description: Save the current chat session using a custom script that receives conversation JSON via stdin
  keywords: [chat, save, script, export, conversation, json]
  related: [chat-save, chat-load, chat-export]
---

# /chat save-via-script

Save the current chat session using a custom script that receives conversation JSON via stdin.

## Overview

The `/chat save-via-script` command exports the current conversation as JSON and pipes it to a custom script for processing and saving.

## Usage

```
/chat save-via-script <SCRIPT>
```

## Arguments

- `<SCRIPT>` - Path to script (should exit 0 on success)

## Options

- `-h, --help` - Print help

## Examples

### Example 1: Save to Database

```
/chat save-via-script ./scripts/save-to-db.sh
```

Pipes conversation JSON to database save script.

### Example 2: Custom Format Export

```
/chat save-via-script ~/bin/export-markdown.py
```

Converts conversation to markdown format using Python script.

### Example 3: Cloud Storage

```
/chat save-via-script ./upload-to-s3.sh
```

Uploads conversation to cloud storage via custom script.

## Script Requirements

- **Input**: Receives conversation JSON via stdin
- **Exit Code**: Must exit with code 0 on success
- **Permissions**: Script must be executable
- **Format**: JSON contains messages, metadata, and timestamps

## JSON Structure

```json
{
  "conversation_id": "uuid",
  "created_at": "timestamp",
  "messages": [
    {
      "role": "user|assistant",
      "content": "message text",
      "timestamp": "timestamp"
    }
  ],
  "metadata": {
    "agent": "agent_name",
    "model": "model_name"
  }
}
```

## Error Handling

- Script not found: Shows file path error
- Non-executable: Shows permission error
- Non-zero exit: Shows script failure message

## Related Commands

- [/chat save](chat-save.md) - Save chat to file
- [/chat load](chat-load.md) - Load saved chat
- [/chat export](chat-export.md) - Export in various formats

## Technical Details

**Data Flow**: Current conversation → JSON → stdin → custom script

**Validation**: Checks script exists and is executable before processing.