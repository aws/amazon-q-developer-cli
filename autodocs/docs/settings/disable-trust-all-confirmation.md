---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: chat.disableTrustAllConfirmation
  description: Skip the trust-all-tools confirmation gate on startup
  keywords: [setting, trust, tools, confirmation, gate, startup, security]
  related: [chat-interface-settings]
---

# chat.disableTrustAllConfirmation

Skip the trust-all-tools confirmation gate on startup.

## Overview

The `chat.disableTrustAllConfirmation` setting controls whether the trust-all-tools confirmation prompt appears when starting a chat session with `--trust-all-tools`. When enabled, the confirmation gate is skipped automatically.

This setting is typically set by choosing "Yes, and don't ask again" at the trust gate prompt, but can also be configured manually.

## Usage

### Skip Confirmation Gate

```bash
kiro-cli settings chat.disableTrustAllConfirmation true
```

### Re-enable Confirmation Gate

```bash
kiro-cli settings chat.disableTrustAllConfirmation false
```

### Check Status

```bash
kiro-cli settings chat.disableTrustAllConfirmation
```

## Value

**Type**: Boolean  
**Default**: `false`  
**Scope**: Global only  
**Values**: `true` or `false`

## Examples

### Example 1: Set via Trust Gate Prompt

When starting with `--trust-all-tools`, the trust gate shows a confirmation prompt. Selecting the option to permanently skip the confirmation sets this setting to `true`.

> **Note**: The exact UX of the trust gate prompt differs between V1 CLI (text-based confirmation) and V2 TUI (interactive menu). The setting behaves identically in both.

### Example 2: Manually Disable Confirmation

```bash
kiro-cli settings chat.disableTrustAllConfirmation true
```

Future sessions with `--trust-all-tools` will skip the confirmation prompt.

### Example 3: Re-enable Confirmation

```bash
kiro-cli settings chat.disableTrustAllConfirmation false
```

The trust gate prompt will appear again on startup.

## Related

- [Chat Interface Settings](chat-interface-settings.md) - Other chat settings

## Troubleshooting

### Issue: Confirmation Still Appears

**Symptom**: Trust gate shows despite setting being `true`  
**Cause**: Setting not saved correctly  
**Solution**: Verify with `kiro-cli settings chat.disableTrustAllConfirmation`

### Issue: Want to See Confirmation Again

**Symptom**: Previously chose "don't ask again" but want to review  
**Solution**: Run `kiro-cli settings chat.disableTrustAllConfirmation false`
