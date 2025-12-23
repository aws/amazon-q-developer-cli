---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /model
  description: Select AI model for current conversation session
  keywords: [model, ai, claude, select, switch]
  related: [default-model, cmd-chat]
---

# /model

Select AI model for current conversation session.

## Overview

The `/model` command shows interactive picker to select AI model for current session. Changes apply immediately and persist for session duration.

## Usage

```
/model
```

Shows picker with available models.

### Set Current as Default

```
/model set-current-as-default
```

Sets currently selected model as default for new sessions.

## Subcommands

### (no subcommand)

Interactive model picker.

```
/model
```

### set-current-as-default

Save current model as default.

```
/model set-current-as-default
```

Equivalent to: `kiro-cli settings chat.defaultModel <current-model>`

## Examples

### Example 1: Select Model

```
/model
```

**Output**:
```
Select model:
  Claude 3.5 Sonnet (current) | 1.0x credit
  Claude 3 Opus | 3.0x credit
  Claude 3 Haiku | 0.3x credit
```

Shows current model, rate multipliers, and descriptions.

### Example 2: Set as Default

```
/model set-current-as-default
```

**Output**:
```
✓ Set Claude 3.5 Sonnet as default model
```

## Related

- [chat.defaultModel](../settings/default-model.md) - Set default model
- [kiro-cli chat --model](../commands/chat.md) - Start with specific model

## Limitations

- Interactive picker only (not available in headless mode)
- Changes apply to current session only
- Available models depend on region

## Technical Details

**Model Selection**: Shows models available in current region.

**Context Window**: Different models have different context window sizes.

**Persistence**: Model selection persists for session, not saved to database.
