---
doc_meta:
  validated: 2026-01-26
  commit: 0f64f6a0
  status: validated
  testable_headless: true
  category: slash_command
  title: /model
  description: Select AI model for current conversation session
  keywords: [model, ai, claude, select, switch, autocomplete]
  related: [default-model, cmd-chat]
---

# /model

Select AI model for current conversation session.

## Overview

The `/model` command selects an AI model for the current session. You can either use the interactive picker or specify a model name directly. Changes apply immediately and persist for session duration.

## Usage

```
/model [model-name]
```

- Without arguments: Shows interactive picker
- With model name: Selects model directly

### Tab Completion

Type `/model ` and press Tab to autocomplete model names. Hints appear as you type.

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

### model-name (positional argument)

Select a model directly by name.

```
/model claude-sonnet-4
```

Supports partial matching and is case-insensitive.

### set-current-as-default

Save current model as default.

```
/model set-current-as-default
```

Equivalent to: `kiro-cli settings chat.defaultModel <current-model>`

## Examples

### Example 1: Interactive Selection

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

### Example 2: Direct Model Selection

```
/model claude-sonnet-4
```

**Output**:
```
 Using claude-sonnet-4
```

### Example 3: Model Not Found with Suggestion

```
/model claud-sonet
```

**Output**:
```
Model 'claud-sonet' not found. Did you mean claude-sonnet-4? Run /model to browse available models.
```

Fuzzy matching suggests similar model names when the exact name isn't found.

### Example 4: Set as Default

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

- Interactive picker not available in headless mode (use direct selection instead)
- Changes apply to current session only
- Available models depend on region

## Technical Details

**Model Selection**: Shows models available in current region.

**Direct Selection**: Matches against model name and model ID, case-insensitive.

**Fuzzy Matching**: Uses Jaro-Winkler similarity to suggest models when exact match not found.

**Tab Completion**: Model names are fetched dynamically and filtered by prefix as you type.

**Persistence**: Model selection persists for session, not saved to database.
