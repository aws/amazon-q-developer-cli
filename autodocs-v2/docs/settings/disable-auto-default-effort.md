---
doc_meta:
  title: chat.disableAutoDefaultEffort
  description: Disable automatically saving effort level as a per-model default
  category: setting
  keywords: [setting, effort, default, auto-save, persist, sticky, disable, per-model]
  related: [slash-effort, default-model, disable-auto-default-model]
  validated: 2026-06-26
  commit: f49c99c49
  status: validated
  testable_headless: true
---

# chat.disableAutoDefaultEffort

Disable automatically saving the selected effort level as a per-model default.

## Overview

By default, using `/effort` to change the reasoning effort level automatically persists that level as a per-model default under `chat.modelDefaults`. The confirmation message shows `(saved for <model>)` when this happens. On subsequent sessions, the saved effort level is automatically applied when that model is active.

When `chat.disableAutoDefaultEffort` is set to `true`:
- `/effort` changes still take effect for the current session
- The chosen level is **not** persisted to `chat.modelDefaults`

Previously saved per-model effort defaults in `chat.modelDefaults` are still applied on session start — this setting only prevents new values from being automatically written.

## Default

`false` (effort level is automatically saved per model on each `/effort` change)

**Type**: Boolean  
**Scope**: Global (session-safe — can be set mid-session)  
**Key**: `chat.disableAutoDefaultEffort`

## Usage

### Disable auto-save via CLI

```bash
kiro-cli settings chat.disableAutoDefaultEffort true
```

### Disable via in-chat command

```
/settings set chat.disableAutoDefaultEffort true
```

### Check current value

```bash
kiro-cli settings chat.disableAutoDefaultEffort
```

### Re-enable auto-save (restore default behavior)

```bash
kiro-cli settings chat.disableAutoDefaultEffort false
```

Or delete the setting entirely:

```bash
kiro-cli settings --delete chat.disableAutoDefaultEffort
```

## Examples

### Example 1: Change Effort Without Auto-Saving

With the setting enabled:

```
/effort low
```

**Output**:
```
Effort set to low
```

Note: no `(saved for <model>)` suffix — the change applies only to this session.

### Example 2: Behavior Comparison

With `chat.disableAutoDefaultEffort` set to `false` (default):

```
/effort low
→ Effort set to low (saved for Claude Opus 4.7)
```

With `chat.disableAutoDefaultEffort` set to `true`:

```
/effort low
→ Effort set to low
```

### Example 3: Still Manually Set Effort Defaults

Even with auto-save disabled, you can manually edit `~/.kiro/settings/cli.json` to set effort defaults:

```json
{
  "chat.modelDefaults": {
    "claude-opus-4.7": {
      "output_config": {
        "effort": "low"
      }
    }
  }
}
```

However, with `chat.disableAutoDefaultEffort` set to `true`, future `/effort` changes will not overwrite this manual entry — the setting only disables the automatic write, not the read/apply.

### Example 4: Explicit --effort Flag Is Unaffected

The `--effort` CLI flag always takes precedence regardless of this setting:

```bash
kiro-cli chat --effort high
```

This applies `high` effort for the session. The flag is not affected by `chat.disableAutoDefaultEffort`.

## When to Use

- **Frequent effort switching**: When you often adjust effort for specific tasks but don't want each change saved permanently
- **Prefer built-in defaults**: When you want models to always start at their built-in default effort rather than a previously chosen level
- **Explicit control**: When you prefer to set effort defaults deliberately by editing `cli.json` rather than having them change automatically

## Troubleshooting

### Issue: Effort Default Not Being Saved

**Symptom**: After using `/effort`, future sessions don't use the chosen effort level  
**Cause**: `chat.disableAutoDefaultEffort` is set to `true`  
**Solution**: Set it to `false`, or manually add the default to `chat.modelDefaults` in `cli.json` (manual entries are always applied regardless of this setting)

### Issue: Saved Effort Default Not Applied

**Symptom**: `chat.modelDefaults` has an effort entry but new sessions don't use it  
**Cause**: The model may have changed families (effort path mismatch), or the saved level is not in the model's available options  
**Solution**: Verify the entry uses the correct schema path (`output_config.effort` for Claude, `reasoning.effort` for GPT)

### Issue: --effort Flag Conflicts

**Symptom**: Using `--effort` flag, wondering if auto-save fires  
**Cause**: The `--effort` flag prevents auto-apply of saved defaults (flag wins) but doesn't prevent `/effort` from auto-saving later in the session  
**Solution**: The flag and auto-save are independent. Use `chat.disableAutoDefaultEffort` if you want to prevent auto-saving entirely.

## Related

- [/effort](../slash-commands/effort.md) — Set effort level in session
- [chat.defaultModel](default-model.md) — Default model setting
- [chat.disableAutoDefaultModel](disable-auto-default-model.md) — Companion setting for model auto-save
