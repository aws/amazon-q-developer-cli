---
doc_meta:
  validated: 2026-05-22
  commit: cdba9a0f8
  status: validated
  testable_headless: true
  category: slash_command
  title: /effort
  description: Set reasoning effort level for the current model
  keywords: [effort, reasoning, low, medium, high, model, performance]
  related: [slash-model, default-model]
---

# /effort

Set reasoning effort level for the current model.

## Overview

The `/effort` command controls how much reasoning effort the model applies to responses. Lower effort means faster, cheaper responses; higher effort means more thorough reasoning. Changes apply immediately and persist for the session duration.

## Usage

```
/effort [level]
```

- Without arguments: Shows available effort levels
- With level: Sets effort directly

## Available Levels

| Level | Description |
|-------|-------------|
| low | Minimal reasoning, fastest responses |
| medium | Balanced reasoning |
| high | Thorough reasoning |
| xhigh | Extended reasoning |
| max | Maximum reasoning effort |

Available levels depend on the model. Not all models support effort configuration.

## Examples

### Example 1: Show Available Levels

```
/effort
```

**Output**:
```
Available effort levels: low, medium, high, xhigh, max
```

### Example 2: Set Effort Level

```
/effort low
```

**Output**:
```
Effort set to low
```

### Example 3: Model Without Effort Support

```
/effort
```

**Output**:
```
Effort configuration is currently not available on Amazon Nova Pro. Select a /model that supports effort (like claude-opus-4.7) to configure.
```

## Persistent Defaults via Settings

You can set a default effort level per model in `~/.kiro/settings/cli.json` so it applies automatically to every new session:

```json
{
  "chat.defaultModel": "claude-opus-4.7",
  "chat.modelDefaults": {
    "claude-opus-4.7": {
      "output_config": {
        "effort": "low"
      }
    },
    "claude-sonnet-4.6": {
      "output_config": {
        "effort": "medium"
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
- [kiro-cli chat](../commands/chat.md) - Start session with `--effort` flag

## Limitations

- Not all models support effort configuration
- Changes are session-scoped; use settings file for persistent defaults
- Available levels are determined by the model's schema
