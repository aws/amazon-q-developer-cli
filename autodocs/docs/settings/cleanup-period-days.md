---
doc_meta:
  validated: 2026-02-28
  commit: 564dc450
  status: validated
  testable_headless: true
  category: setting
  title: cleanup.periodDays
  description: Days after which old conversations, sessions, and knowledge bases are deleted
  keywords: [setting, cleanup, retention, delete, conversations, sessions, knowledge, storage]
  related: [session-management, knowledge-management]
---

# cleanup.periodDays

Days after which old conversations, sessions, and knowledge bases are automatically deleted.

## Overview

Controls automatic cleanup of old data to manage disk space. When set, Kiro deletes conversations, session files, and knowledge bases that haven't been modified within the specified number of days. Cleanup runs at startup before user interaction.

## Usage

### Set Cleanup Period

```bash
kiro-cli settings cleanup.periodDays 30
```

### Delete All Data Immediately on Next Startup

```bash
kiro-cli settings cleanup.periodDays 0
```

### Disable Cleanup

```bash
kiro-cli settings --reset cleanup.periodDays
```

### Check Current Value

```bash
kiro-cli settings cleanup.periodDays
```

## Value

**Type**: Integer (number of days)  
**Default**: Not set (no automatic cleanup)

## What Gets Cleaned Up

When enabled, the following data older than the specified period is deleted:

- **Conversations**: Chat history stored in the database
- **Session files**: `.json` and `.jsonl` files in the CLI sessions directory (default `~/.kiro/sessions/cli/`)
- **Knowledge bases**: Directories in the global knowledge bases folder

## Examples

### Example 1: Keep 30 Days of Data

```bash
kiro-cli settings cleanup.periodDays 30
```

Deletes data older than 30 days on each startup.

### Example 2: Keep 90 Days of Data

```bash
kiro-cli settings cleanup.periodDays 90
```

More conservative retention for users who reference older sessions.

### Example 3: Delete All Data Immediately

```bash
kiro-cli settings cleanup.periodDays 0
```

Setting to 0 deletes all data on next startup. To disable cleanup, unset the value:

```bash
kiro-cli settings --reset cleanup.periodDays
```

## Related Features

- [Session Management](../features/session-management.md) - Managing chat sessions
- [Knowledge Management](../features/knowledge-management.md) - Knowledge base storage

## Limitations

- Cleanup runs sequentially before user interaction (may add slight delay)
- Uses file modification time, not creation time
- Cannot recover deleted data
- Does not clean up workspace-specific data

## Technical Details

**Scope**: Global only (cannot be overridden at workspace level)

**Cleanup timing**: Runs once at application startup, before the chat interface loads

**Data locations cleaned**:
- Database conversations table
- CLI sessions directory (default `~/.kiro/sessions/cli/`): `*.json` and `*.jsonl`
- Global knowledge bases directory
