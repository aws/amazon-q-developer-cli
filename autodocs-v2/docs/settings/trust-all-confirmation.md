---
doc_meta:
  validated: 2026-08-12
  commit: 20bc37f97
  status: validated
  testable_headless: true
  category: setting
  title: chat.disableTrustAllConfirmation
  description: Skip the trust-all-tools confirmation gate on startup
  keywords: [setting, trust, tools, confirmation, gate, startup]
  related: [cmd-chat]
---

# chat.disableTrustAllConfirmation

Skip the trust-all-tools confirmation gate on startup.

## Overview

The `chat.disableTrustAllConfirmation` setting controls whether Kiro CLI shows the trust-all-tools confirmation prompt when starting a chat session with `--trust-all-tools`. When enabled, the confirmation gate is bypassed automatically.

This setting is typically set by choosing "Yes, and don't ask again" at the trust gate prompt, but can also be configured manually.

This is a **global-only** setting and cannot be overridden per-workspace.

## Usage

### Set Value

```bash
kiro-cli settings chat.disableTrustAllConfirmation true
```

### Get Current Value

```bash
kiro-cli settings chat.disableTrustAllConfirmation
```

### Delete Setting

```bash
kiro-cli settings --delete chat.disableTrustAllConfirmation
```

## Value

**Type**: Boolean  
**Default**: `false` (confirmation gate shown)  
**Scope**: Global only

## Examples

### Example 1: Disable Confirmation Gate

```bash
kiro-cli settings chat.disableTrustAllConfirmation true
```

After this, `kiro-cli chat --trust-all-tools` will start immediately without the confirmation prompt.

### Example 2: Re-enable Confirmation Gate

```bash
kiro-cli settings chat.disableTrustAllConfirmation false
```

Or delete the setting:

```bash
kiro-cli settings --delete chat.disableTrustAllConfirmation
```

### Example 3: Check Current Status

```bash
kiro-cli settings chat.disableTrustAllConfirmation
```

**Output**: `true` or `false`

## Trust Gate Behavior

When starting a chat with `--trust-all-tools`, Kiro CLI presents a confirmation gate with three options:

1. **No, exit** - Exit without starting the session
2. **Yes, I accept** - Accept for this session only
3. **Yes, and don't ask again** - Accept and set `chat.disableTrustAllConfirmation` to `true`

Choosing option 3 persists your preference to `~/.kiro/settings/cli.json`.

## Related

- [kiro-cli chat](../commands/chat.md) - Start chat sessions

## Troubleshooting

### Issue: Confirmation Still Appears

**Symptom**: Trust gate shows despite setting being true  
**Cause**: Setting not saved correctly or different config file  
**Solution**: Verify with `kiro-cli settings chat.disableTrustAllConfirmation`

### Issue: Want to See Confirmation Again

**Symptom**: Previously chose "don't ask again" but want the prompt back  
**Cause**: Setting is persisted  
**Solution**: Run `kiro-cli settings chat.disableTrustAllConfirmation false`
