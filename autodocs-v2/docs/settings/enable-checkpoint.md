---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableCheckpoint
  description: Enable checkpoint feature for creating workspace snapshots
  keywords: [setting, checkpoint, experimental, snapshot]
  related: [slash-checkpoint]
---

# chat.enableCheckpoint

Enable checkpoint feature for creating workspace snapshots.

## Overview

Enables the experimental checkpoint feature for creating and managing workspace snapshots. Allows saving workspace state and restoring later.

## Usage

### Enable Checkpoint

```bash
kiro-cli settings chat.enableCheckpoint true
```

### Disable Checkpoint

```bash
kiro-cli settings chat.enableCheckpoint false
```

## Value

**Type**: Boolean  
**Default**: `false`

## Examples

### Example 1: Enable Feature

```bash
kiro-cli settings chat.enableCheckpoint true
```

Enables `/checkpoint` commands.

### Example 2: Check Status

```bash
kiro-cli settings chat.enableCheckpoint
```

## Related Features

- [/checkpoint](../slash-commands/checkpoint.md) - Checkpoint commands
- [/experiment](../slash-commands/experiment.md) - Alternative enable method

## Limitations

- Experimental feature (may change or be removed)
- Conflicts with tangent mode
- Requires explicit enablement

## Technical Details

**Scope**: User-wide setting

**Conflicts**: Disabled when in tangent mode
