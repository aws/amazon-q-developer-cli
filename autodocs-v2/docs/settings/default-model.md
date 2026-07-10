---
doc_meta:
  validated: 2026-06-26
  commit: f49c99c49
  status: validated
  testable_headless: true
  category: setting
  title: chat.defaultModel
  description: Set default AI model for new chat sessions
  keywords: [setting, model, default, ai, switch]
  related: [slash-model, cmd-chat, disable-auto-default-model]
---

# chat.defaultModel

Set default AI model for new chat sessions.

## Overview

The `chat.defaultModel` setting specifies which AI model to use when starting new chat sessions. Without this setting, the system default model is used.

## Usage

### Set Default Model

```bash
kiro-cli settings chat.defaultModel <model-id>
```

### Get Current Value

```bash
kiro-cli settings chat.defaultModel
```

### Delete Setting

```bash
kiro-cli settings --delete chat.defaultModel
```

## Value

**Type**: String  
**Default**: None (uses system default)  
**Example**: `anthropic.claude-3-5-sonnet-20241022-v2:0`

## Model Selection Priority

When starting a session, the model is selected in this order:

1. `--model` CLI flag (highest priority)
2. Resumed session's saved model (when using `--resume`)
3. `chat.defaultModel` setting
4. System default model

## Examples

### Example 1: Set Claude 3.5 Sonnet

```bash
kiro-cli settings chat.defaultModel anthropic.claude-3-5-sonnet-20241022-v2:0
```

### Example 2: Check Current Model

```bash
kiro-cli settings chat.defaultModel
```

**Output**: `anthropic.claude-3-5-sonnet-20241022-v2:0`

### Example 3: Clear Default

```bash
kiro-cli settings --delete chat.defaultModel
```

### Example 4: Override Default at Startup

```bash
kiro-cli chat --model claude-sonnet-4
```

The `--model` flag overrides the default for this session.

### Example 5: Find Available Models

Use `/model` in a chat session to see available models:

```
/model
```

Shows all models with their IDs, which you can use with this setting.

## Related

- [/model](../slash-commands/model.md) - Switch models mid-session
- [chat.disableAutoDefaultModel](disable-auto-default-model.md) - Disable auto-saving model on switch
- [kiro-cli chat --model](../commands/chat.md) - Start with specific model
- [/usage](../slash-commands/usage.md) - Check model usage

## Troubleshooting

### Issue: Model Not Available

**Symptom**: Error "The model 'X' is not available. Please use '/model' to select a different model and try again." when sending a message.

**Cause**: The configured default model is not available in the current region or has been removed.

**Solution**: Update the setting with a valid model ID, or delete the setting to use the system default. Use `/model` in a chat session to see available models.

### Issue: Setting Not Applied

**Symptom**: Different model used  
**Cause**: Session already started, model specified in command, or resuming saved session  
**Solution**: The `--model` flag and resumed sessions take priority. Start a fresh session without flags.

### Issue: Don't Know Model ID

**Symptom**: Unsure what value to use  
**Solution**: Run `/model` in a chat session to see available models and their IDs
