---
doc_meta:
  title: session
  description: Adjust session settings temporarily (in-memory only, cleared on exit)
  category: tool
  keywords: [session, settings, configuration, temporary, in-memory]
  related: [introspect, fs-write]
  validated: 2026-08-11
  commit: e339d50be
  status: validated
  testable_headless: true
---

# session

Adjust session settings temporarily (in-memory only, cleared on exit).

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool when you want to temporarily change a setting for the current chat session.

The session tool enables temporary, in-memory setting changes that are cleared when the chat exits. For permanent changes, the assistant uses fs_write to modify `~/.kiro/settings.json` (global) or `.kiro/settings.json` (workspace) instead.

## How It Works

1. User asks to change a setting (e.g., "disable markdown")
2. Assistant uses introspect tool to verify the setting name exists
3. Assistant uses session tool to apply the change temporarily
4. Setting reverts to default when the chat session ends

## Session vs Persistent Settings

- **session tool**: Temporary in-memory changes (cleared when chat exits) - Use for quick experiments or one-time adjustments
- **fs_write tool**: Permanent changes saved to disk - Use when user says "save", "persist", "permanently", or "always"
  - Global settings: `~/.kiro/settings.json`
  - Workspace settings: `.kiro/settings.json`

## Operations

### list

Show currently configured session settings (non-default values only).

```json
{
  "operation": "list"
}
```

### get

Get the current value of a specific setting.

```json
{
  "operation": "get",
  "key": "chat.disableMarkdownRendering"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | Yes | Setting key (e.g., 'chat.disableMarkdownRendering') |

### set

Change a setting value temporarily.

```json
{
  "operation": "set",
  "key": "chat.disableMarkdownRendering",
  "value": true
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | Yes | Setting key. MUST be verified with introspect tool first |
| `value` | any | Yes | Value to set. Type depends on the setting (boolean, string, or number) |

### reset

Clear session override for a specific setting, or all session overrides if no key provided.

```json
{
  "operation": "reset",
  "key": "chat.disableMarkdownRendering"
}
```

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | No | Setting key to reset. If omitted, resets all session overrides |

## Examples

### Example 1: Disable Markdown Rendering

```json
{
  "operation": "set",
  "key": "chat.disableMarkdownRendering",
  "value": true
}
```

### Example 2: Check Current Overrides

```json
{
  "operation": "list"
}
```

### Example 3: Reset All Overrides

```json
{
  "operation": "reset"
}
```

## Troubleshooting

### Issue: Setting Not Taking Effect

**Symptom**: Changed setting doesn't seem to apply  
**Cause**: Incorrect setting key name  
**Solution**: Use introspect tool first to verify the exact setting name and valid values.

### Issue: Setting Lost After Restart

**Symptom**: Setting reverted after closing chat  
**Cause**: Session settings are temporary by design  
**Solution**: Use fs_write to save to `~/.kiro/settings.json` for permanent changes.

## Related Features

- [introspect](introspect.md) - Verify setting names and understand their purpose
- [fs_write](fs-write.md) - Save permanent settings to disk

## Limitations

- All session settings are cleared when the chat exits
- Setting keys must be verified with introspect before use
- Only supports boolean, string, and number value types

## Technical Details

**Aliases**: `session`

**Persistence**: In-memory only. No disk writes performed by this tool.

**Required Workflow**: Always use introspect tool first to verify setting names exist before calling set.
