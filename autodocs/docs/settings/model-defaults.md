---
doc_meta:
  validated: 2026-05-26
  commit: pending
  status: validated
  testable_headless: true
  category: setting
  title: chat.modelDefaults
  description: Per-model additional field defaults like effort level
  keywords: [setting, model, defaults, effort, output_config, reasoning, per-model]
  related: [default-model, slash-model]
---

# chat.modelDefaults

Per-model additional field defaults (object of model ID → overrides).

## Overview

The `chat.modelDefaults` setting lets you configure default additional fields for specific models. This is useful for setting per-model defaults like effort level that apply automatically when using that model.

The exact JSON path under each model mirrors that model's request schema:

| Model family | Effort path | Allowed values |
|---|---|---|
| Claude (e.g. `claude-opus-4.7`, `claude-opus-4.7-messages`) | `output_config.effort` | `low`, `medium`, `high`, `xhigh`, `max` |
| GPT (e.g. `openai-gpt-5.4`, `openai-gpt-5.4-1p`, `openai-gpt-5.5-1p`) | `reasoning.effort` | `low`, `medium`, `high`, `xhigh` |

Not every model exposes an effort field. For example, `claude-sonnet-4.5`, `minimax-*`, `glm-*`, `kimi-k2.5`, `nemotron-super-3-120b`, `openai-gpt-oss-20b`, and `gemini-2.5-pro` declare no `additionalModelRequestFieldsSchema` and ignore effort overrides.

Unknown paths are ignored at session bootstrap with a `tracing::warn!` — the rest of the model's defaults still apply.

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

### Example 2: Set Effort Level for a GPT Model

GPT models declare effort under `reasoning.effort` instead of `output_config.effort`. Note that GPT models do not support the `max` level:

```bash
kiro-cli settings chat.modelDefaults '{"openai-gpt-5.4": {"reasoning": {"effort": "high"}}}'
```

### Example 3: Configure Multiple Models Across Families

```bash
kiro-cli settings chat.modelDefaults '{
  "claude-opus-4.7": {"output_config": {"effort": "low"}},
  "claude-opus-4.7-messages": {"output_config": {"effort": "medium"}},
  "openai-gpt-5.4": {"reasoning": {"effort": "high"}},
  "openai-gpt-5.5-1p": {"reasoning": {"effort": "xhigh"}}
}'
```

### Example 4: Check Current Defaults

```bash
kiro-cli settings chat.modelDefaults
```

**Output**:
```json
{"claude-opus-4.7":{"output_config":{"effort":"low"}}}
```

### Example 5: Clear Model Defaults

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
