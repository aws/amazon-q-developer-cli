---
doc_meta:
  validated: 2026-06-26
  commit: f49c99c49
  status: validated
  testable_headless: true
  category: slash_command
  title: /effort
  description: Set reasoning effort level for the current model
  keywords: [effort, reasoning, low, medium, high, model, performance, output_config, reasoning.effort, sticky, default]
  related: [slash-model, default-model, disable-auto-default-effort]
---

# /effort

Set reasoning effort level for the current model.

## Overview

The `/effort` command controls how much reasoning effort the model applies to responses. Lower effort means faster, cheaper responses; higher effort means more thorough reasoning. Changes apply immediately and are automatically saved as your per-model default for future sessions (disable with `chat.disableAutoDefaultEffort`).

## Usage

```
/effort [level]
```

- Without arguments: Shows available effort levels
- With level: Sets effort directly

## Available Levels

| Level | Description | Supported by |
|-------|-------------|--------------|
| low | Minimal reasoning, fastest responses | Claude, GPT |
| medium | Balanced reasoning | Claude, GPT |
| high | Thorough reasoning | Claude, GPT |
| xhigh | Extended reasoning | Claude, GPT |
| max | Maximum reasoning effort | Claude only |

Available levels depend on the model. Not all models support effort configuration — for example `claude-sonnet-4.5`, `minimax-*`, `glm-*`, `kimi-k2.5`, `nemotron-super-3-120b`, `openai-gpt-oss-20b`, and `gemini-2.5-pro` do not expose an effort field.

## Examples

### Example 1: Show Available Levels

```
/effort
```

**Output** (Claude models):
```
Available effort levels: low, medium, high, xhigh, max
```

**Output** (GPT models — no `max`):
```
Available effort levels: low, medium, high, xhigh
```

### Example 2: Set Effort Level

```
/effort low
```

**Output**:
```
Effort set to low (saved for Claude Opus 4.7; disable with kiro-cli settings chat.disableAutoDefaultEffort true)
```

The effort level is automatically persisted as a per-model default in your settings. To disable this auto-save behavior, set `chat.disableAutoDefaultEffort` to `true`.

### Example 3: Model Without Effort Support

```
/effort
```

**Output**:
```
Effort configuration is currently not available on claude-sonnet-4.5. Select a /model that supports effort (like claude-opus-4.7) to configure.
```

## Persistent Defaults via Settings

You can set a default effort level per model in `~/.kiro/settings/cli.json` so it applies automatically to every new session.

The exact JSON shape under each model mirrors that model's request schema:

| Model family | Effort path | Allowed values | Example |
|---|---|---|---|
| Claude (e.g. `claude-opus-4.7`, `claude-opus-4.7-messages`) | `output_config.effort` | `low`, `medium`, `high`, `xhigh`, `max` | `{"output_config": {"effort": "low"}}` |
| GPT (e.g. `openai-gpt-5.4`, `openai-gpt-5.4-1p`, `openai-gpt-5.5-1p`) | `reasoning.effort` | `low`, `medium`, `high`, `xhigh` | `{"reasoning": {"effort": "high"}}` |

Use the path that matches the model. Unknown paths are ignored at session bootstrap and logged as a `tracing::warn!`.

```json
{
  "chat.defaultModel": "claude-opus-4.7",
  "chat.modelDefaults": {
    "claude-opus-4.7": {
      "output_config": {
        "effort": "low"
      }
    },
    "claude-opus-4.7-messages": {
      "output_config": {
        "effort": "medium"
      }
    },
    "openai-gpt-5.4": {
      "reasoning": {
        "effort": "high"
      }
    },
    "openai-gpt-5.5-1p": {
      "reasoning": {
        "effort": "xhigh"
      }
    }
  }
}
```

Per-model defaults live under the `chat.modelDefaults` key, keyed by exact model ID.

### Precedence

When a session starts, effort is resolved in this order (highest priority first):

1. **CLI flag** (`--effort`) — applied when starting a new session
2. **Loaded session** (`/chat load`) — restores the session's saved effort
3. **User defaults** from `cli.json` — applied on new session or model switch
4. **Built-in defaults** — hardcoded per model (e.g. `xhigh` for claude-opus-4.7)

Using `/effort` within a session always overrides whatever was set at startup.

You can also set effort at launch without editing settings:

```bash
kiro-cli chat --effort low "Quick question"
```

### Workspace Overrides

Workspace-level settings (`.kiro/settings/cli.json`) override global settings, so teams can share effort preferences per project.

## Related

- [/model](model.md) - Switch models in session
- [chat.defaultModel](../settings/default-model.md) - Set default model
- [chat.disableAutoDefaultEffort](../settings/disable-auto-default-effort.md) - Disable auto-saving effort
- [kiro-cli chat](../commands/chat.md) - Start session with `--effort` flag

## Limitations

- Not all models support effort configuration
- Available levels are determined by the model's schema
