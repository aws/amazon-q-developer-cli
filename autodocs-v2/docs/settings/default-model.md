---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.defaultModel
  description: Set default AI model for new chat sessions
  keywords: [setting, model, default, ai]
  related: [slash-model, cmd-chat]
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

## Related

- [/model](../slash-commands/model.md) - Switch models in session
- [kiro-cli chat --model](../commands/chat.md) - Start with specific model

## Troubleshooting

### Issue: Model Not Available

**Symptom**: Error using model  
**Cause**: Model not available in region  
**Solution**: Check available models with `/model`

### Issue: Setting Not Applied

**Symptom**: Different model used  
**Cause**: Session already started or model specified in command  
**Solution**: Restart session or remove `--model` flag
