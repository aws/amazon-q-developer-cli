---
doc_meta:
  title: chat.disableAutoDefaultModel
  description: Disable automatically saving the selected model as the default
  category: setting
  keywords: [setting, model, default, auto-save, persist, sticky, disable]
  related: [default-model, slash-model, disable-auto-default-effort]
  validated: 2026-06-26
  commit: f49c99c49
  status: validated
  testable_headless: true
---

# chat.disableAutoDefaultModel

Disable automatically saving the selected model as the default.

## Overview

By default, switching models via `/model` automatically persists that model as your default for future sessions (the "sticky default" behavior). The confirmation message shows `(saved as default)` when this happens.

When `chat.disableAutoDefaultModel` is set to `true`, model selection no longer auto-persists. The `/model` switch still takes effect for the current session, but future sessions start with whatever default was previously saved (or the system default if none was saved).

You can still explicitly save a model as default using `/model set-current-as-default` or `kiro-cli settings chat.defaultModel <id>` — this setting only disables the *automatic* save on every switch.

## Default

`false` (model switches are automatically saved as default)

**Type**: Boolean  
**Scope**: Global (session-safe — can be set mid-session)  
**Key**: `chat.disableAutoDefaultModel`

## Usage

### Disable auto-save via CLI

```bash
kiro-cli settings chat.disableAutoDefaultModel true
```

### Disable via in-chat command

```
/settings set chat.disableAutoDefaultModel true
```

### Check current value

```bash
kiro-cli settings chat.disableAutoDefaultModel
```

### Re-enable auto-save (restore default behavior)

```bash
kiro-cli settings chat.disableAutoDefaultModel false
```

Or delete the setting entirely:

```bash
kiro-cli settings --delete chat.disableAutoDefaultModel
```

## Examples

### Example 1: Switch Model Without Auto-Saving

With the setting enabled:

```
/model claude-opus-4.7
```

**Output**:
```
Using claude-opus-4.7
```

Note: no `(saved as default)` suffix — the switch applies only to this session.

### Example 2: Still Explicitly Save When Needed

Even with auto-save disabled, you can explicitly persist a model default:

```
/model set-current-as-default
```

**Output**:
```
Set Claude Opus 4.7 as default model
```

Or via settings:

```bash
kiro-cli settings chat.defaultModel claude-opus-4.7
```

### Example 3: Behavior Comparison

With `chat.disableAutoDefaultModel` set to `false` (default):

```
/model claude-opus-4.7
→ Using claude-opus-4.7 (saved as default)
```

With `chat.disableAutoDefaultModel` set to `true`:

```
/model claude-opus-4.7
→ Using claude-opus-4.7
```

## When to Use

- **Frequent model switching**: When you often switch models for specific tasks but don't want each switch to overwrite your default
- **Team environments**: When workspace settings define a default model and you don't want individual exploratory switches to override it
- **Explicit control**: When you prefer to set the default model deliberately rather than it changing on every switch

## Troubleshooting

### Issue: Model Default Not Being Saved

**Symptom**: After switching models with `/model`, future sessions don't use the new model  
**Cause**: `chat.disableAutoDefaultModel` is set to `true`  
**Solution**: Either set it to `false`, or explicitly save the default with `/model set-current-as-default`

### Issue: Setting Doesn't Prevent Startup Default

**Symptom**: A previously saved default still applies on new sessions  
**Cause**: This setting only prevents *new* auto-saves; it does not clear the existing `chat.defaultModel` value  
**Solution**: Delete the saved default with `kiro-cli settings --delete chat.defaultModel`

## Related

- [chat.defaultModel](default-model.md) — The saved default model value
- [/model](../slash-commands/model.md) — Switch models in session
- [chat.disableAutoDefaultEffort](disable-auto-default-effort.md) — Companion setting for effort levels
