---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /checkpoint
  description: Manage workspace checkpoints with init, list, restore, expand, diff, and clean operations
  keywords: [checkpoint, workspace, snapshot, restore, backup]
  related: [enable-checkpoint]
---

# /checkpoint

Manage workspace checkpoints with init, list, restore, expand, diff, and clean operations.

## Overview

The `/checkpoint` command manages workspace snapshots using Git. Creates shadow bare git repo to track file changes, list checkpoints with file stats, restore to previous states, view differences, and clean shadow repo. Auto-enables in git repositories. Experimental feature.

## How It Works

- **Shadow Repo**: Creates bare git repo to snapshot file changes
- **Turn-Level**: Checkpoints created per conversation turn
- **Tool-Level**: Sub-checkpoints for each tool use
- **Conversation Unwind**: History reverts when restoring
- **Auto-Enable**: Automatically enabled in git repos (ephemeral)
- **Manual Init**: Available for non-git directories

## Usage

```
/checkpoint <subcommand>
```

## Subcommands

### init

Initialize new checkpoint.

```
/checkpoint init
```

Creates snapshot of current workspace state.

### list

List all checkpoints.

```
/checkpoint list
```

Shows available checkpoints with timestamps.

### restore

Restore workspace to checkpoint.

```
/checkpoint restore [tag] [--hard]
```

**Options**:
- `tag`: Checkpoint tag (e.g., 2 or 2.1). Omit for interactive picker
- `--hard`: Exactly match checkpoint state (removes newer files)

**Default mode**:
- Reverts tracked changes and deletions
- Keeps files created after checkpoint

**Hard mode** (`--hard`):
- Exactly matches checkpoint state
- Deletes files created after checkpoint

### expand

Expand checkpoint details.

```
/checkpoint expand
```

Shows detailed checkpoint information.

### diff

Show differences between checkpoints.

```
/checkpoint diff
```

Compares current state with checkpoint.

### clean

Remove old checkpoints.

```
/checkpoint clean
```

Deletes unused checkpoints.

## Configuration

Enable checkpoint feature:

```bash
kiro-cli settings chat.enableCheckpoint true
```

## Examples

### Example 1: Create Checkpoint

```
/checkpoint init
```

**Output**:
```
✔ Created checkpoint: checkpoint-2025-12-19-230000
```

### Example 2: List Checkpoints

```
/checkpoint list
```

**Output**:
```
[0] 2025-09-18 14:00:00 - Initial checkpoint
[1] 2025-09-18 14:05:31 - add two_sum.py (+1 file)
[2] 2025-09-18 14:07:10 - add tests (modified 1)
```

### Example 3: Expand Checkpoint

```
/checkpoint expand 2
```

**Output**:
```
[2] 2025-09-18 14:07:10 - add tests
 └─ [2.1] fs_write: Add minimal test cases to two_sum.py (modified 1)
```

Shows tool-level sub-checkpoints.

### Example 3: Restore

```
/checkpoint restore
```

Shows picker to select checkpoint.

## Troubleshooting

### Issue: Feature Not Enabled

**Symptom**: "Checkpoint is disabled" error  
**Cause**: Feature not enabled  
**Solution**: `kiro-cli settings chat.enableCheckpoint true`

### Issue: Conflicts with Tangent Mode

**Symptom**: Warning about tangent mode  
**Cause**: Checkpoint disabled in tangent mode  
**Solution**: Exit tangent mode first

## Related Features

- [chat.enableCheckpoint](../settings/enable-checkpoint.md) - Enable setting
- [/tangent](tangent.md) - Conversation checkpoints (different feature)

## Limitations

- Experimental feature
- Conflicts with tangent mode
- Workspace-specific (not global)
- No cloud sync

## Technical Details

**Storage**: Checkpoints stored in workspace

**Scope**: Per-workspace, not global

**Conflicts**: Disabled when in tangent mode
