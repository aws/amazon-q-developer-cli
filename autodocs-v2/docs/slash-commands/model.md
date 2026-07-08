---
doc_meta:
  validated: 2026-07-07
  commit: 4869f860d
  status: validated
  testable_headless: true
  category: slash_command
  title: /model
  description: Select AI model for current conversation session
  keywords: [model, ai, claude, select, switch, autocomplete, default]
  related: [default-model, cmd-chat]
---

# /model

Select AI model for current conversation session.

## Overview

The `/model` command selects an AI model for the current session. You can either use the interactive picker or specify a model name directly. Changes apply immediately and are automatically saved as your default for future sessions.

## Usage

```
/model [model-name|set-current-as-default]
```

- Without arguments: Shows interactive picker
- With model name: Selects model directly

### Tab Completion

Type `/model ` and press Tab to autocomplete model names. Hints appear as you type.

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

Save the current session's model as the default for new sessions. This is generally unnecessary since `/model` now auto-persists your selection, but remains available for explicit control.

```
/model set-current-as-default
```

This persists the setting to disk, so future sessions (including new process starts) will use this model.

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
 Using claude-sonnet-4 (saved as default)
```

The model is automatically persisted as your default for new sessions.

### Example 3: Model Not Found with Suggestion

```
/model claud-sonet
```

**Output**:
```
Model 'claud-sonet' not found. Did you mean claude-sonnet-4? Run /model to browse available models.
```

Fuzzy matching suggests similar model names when the exact name isn't found.

### Example 4: Set Default Model via Settings

```bash
kiro-cli settings chat.defaultModel claude-sonnet-4
```

Sets the default model for new sessions.

### Example 5: Save Current Model as Default

```
/model set-current-as-default
```

**Output**:
```
Set Claude Sonnet 4 as default model
```

Persists the current session's model as the default for all future sessions.

### Example 6: Start Session with Specific Model

```bash
kiro-cli chat --model claude-sonnet-4
```

Starts a new session with the specified model.

## FAQ

### How do I switch models mid-session?

Use `/model` to open the picker or `/model <name>` to switch directly. The change takes effect immediately for subsequent messages.

### How do I set a default model?

Use the settings command:

```bash
kiro-cli settings chat.defaultModel <model-id>
```

Or start sessions with `--model`:

```bash
kiro-cli chat --model <model-id>
```

### Does the model persist when I resume a session?

Yes. When you resume a session with `--resume`, the model active when the session was saved is restored. Use `--model` to override.

### How do I see available models?

Run `/model` without arguments to see the interactive picker with all available models, their credit multipliers, and descriptions.

## Related

- [chat.defaultModel](../settings/default-model.md) - Set default model
- [kiro-cli chat --model](../commands/chat.md) - Start with specific model
- [/usage](usage.md) - Check account usage

## Limitations

- Interactive picker not available in headless mode (use direct selection instead)
- Available models depend on region

## Troubleshooting

### Issue: Model Not Available

**Symptom**: Error "The model 'X' is not available. Please use '/model' to select a different model and try again."

**Cause**: The specified model ID is not available in the current region or has been removed.

**Solution**: Use `/model` to see available models and select a valid one.

### Issue: Model Refusal or Content-Filtered Response

**Symptom**: Error alert stating the model cannot continue the conversation, or a provider-specific content policy explanation.

**Cause**: The model's content policy was triggered by the conversation context or your message.

**Solution**: Switch to a different model with `/model`, use `/rewind` to remove the flagged content, or start a new conversation with `/chat new`. See [Model Refusal Alerts](../features/model-refusal-alerts.md) for details.

## Technical Details

**Model Selection**: Shows models available in current region.

**Direct Selection**: Matches against model name and model ID, case-insensitive.

**Fuzzy Matching**: Uses Jaro-Winkler similarity to suggest models when exact match not found.

**Tab Completion**: Model names are fetched dynamically and filtered by prefix as you type.

**Persistence**: Model selection is automatically saved as your default. When resuming, the saved model is restored.
