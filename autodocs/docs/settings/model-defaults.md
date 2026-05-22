---
doc_meta:
  validated: 2026-05-21
  commit: a679c833
  status: validated
  testable_headless: true
  category: setting
  title: chat.modelDefaults
  description: Per-model additional field defaults like effort level
  keywords: [setting, model, defaults, effort, output_config, per-model]
  related: [default-model, slash-model]
---

# chat.modelDefaults

Per-model additional field defaults (object of model ID → overrides).

## Overview

The `chat.modelDefaults` setting lets you configure default additional fields for specific models. This is useful for setting per-model defaults like effort level that apply automatically when using that model.

## Usage

### Set Model Defaults

```bash
kiro-cli settings chat.modelDefaults '{"model-id": {"output_config": {"effort": "low"}}}'
```

### Get Current Value

```bash
kiro-cli settings chat.modelDefaults
```

### Delete Setting

```bash
kiro-cli settings --delete chat.modelDefaults
```

## Value

**Type**: Object (JSON)  
**Default**: None  
**Structure**: `{ "<model-id>": { <additional-field-overrides> } }`

The object keys are exact model IDs. Values are objects containing additional field overrides that the model supports.

## Examples

### Example 1: Set Effort Level for Claude Opus

```bash
kiro-cli settings chat.modelDefaults '{"claude-opus-4.7": {"output_config": {"effort": "low"}}}'
```

### Example 2: Configure Multiple Models

```bash
kiro-cli settings chat.modelDefaults '{
  "claude-opus-4.7": {"output_config": {"effort": "low"}},
  "claude-sonnet-4.6": {"output_config": {"effort": "medium"}}
}'
```

### Example 3: Check Current Defaults

```bash
kiro-cli settings chat.modelDefaults
```

**Output**:
```json
{"claude-opus-4.7":{"output_config":{"effort":"low"}}}
```

### Example 4: Clear Model Defaults

```bash
kiro-cli settings --delete chat.modelDefaults
```

## Precedence

When a session starts, model defaults are resolved in this order (highest priority first):

1. **Loaded session** (`/chat load`) — restores the session's saved settings
2. **User defaults** from `chat.modelDefaults` — applied on new session or model switch
3. **Built-in defaults** — hardcoded per model

## Workspace Overrides

Workspace-level settings (`.kiro/settings/cli.json`) override global settings, so teams can share model preferences per project.

## Related

- [chat.defaultModel](default-model.md) - Set default model for new sessions
- [/model](../slash-commands/model.md) - Switch models in session

## Troubleshooting

### Issue: Override Not Applied

**Symptom**: Model defaults not taking effect  
**Cause**: Model ID doesn't match exactly  
**Solution**: Use exact model ID as shown by `/model`

### Issue: Invalid JSON

**Symptom**: Error when setting value  
**Cause**: Malformed JSON syntax  
**Solution**: Ensure valid JSON with proper quoting

### Issue: Unknown Field Ignored

**Symptom**: Warning in logs about ignored override  
**Cause**: Field not supported by model  
**Solution**: Check model's supported additional fields
