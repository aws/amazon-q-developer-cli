---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: app.disableAutoupdates
  description: Disable automatic update checks on startup
  keywords: [setting, update, auto-update, disable, autoupdate]
  related: []
---

# app.disableAutoupdates

Disable automatic update checks on startup.

## Overview

The `app.disableAutoupdates` setting controls whether Kiro CLI automatically checks for and installs updates when starting a chat session. When enabled (set to `true`), auto-updates are disabled.

This is a global-only setting that applies across all workspaces.

## Usage

### Disable Auto-Updates

```bash
kiro-cli settings app.disableAutoupdates true
```

### Enable Auto-Updates

```bash
kiro-cli settings app.disableAutoupdates false
```

### Check Status

```bash
kiro-cli settings app.disableAutoupdates
```

## Value

**Type**: Boolean  
**Default**: `false` (auto-updates enabled)  
**Scope**: Global only  
**Values**: `true` or `false`

## Examples

### Example 1: Disable Auto-Updates

```bash
kiro-cli settings app.disableAutoupdates true
```

Prevents background update checks on startup.

### Example 2: Check Current Setting

```bash
kiro-cli settings app.disableAutoupdates
```

**Output**: `true` or `false`

### Example 3: Re-enable Auto-Updates

```bash
kiro-cli settings app.disableAutoupdates false
```

## Environment Variable Alternative

You can also disable auto-updates via environment variable:

```bash
export KIRO_NO_AUTO_UPDATE=1
```

The environment variable takes precedence over the setting.

## Related

- `kiro-cli update` - Manual update command (run from terminal)

## Troubleshooting

### Issue: Updates Still Happening

**Symptom**: Updates occur despite setting being `true`  
**Cause**: Setting not saved or environment variable not set  
**Solution**: Verify with `kiro-cli settings app.disableAutoupdates`

### Issue: Setting Not Persisting

**Symptom**: Setting resets after restart  
**Cause**: File write issue  
**Solution**: Check file permissions on settings file (~/.kiro/settings/cli.json)
