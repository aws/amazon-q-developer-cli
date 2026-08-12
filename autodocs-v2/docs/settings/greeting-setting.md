---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: chat.greeting.enabled
  description: Control whether the welcome screen displays when starting a chat session
  keywords: [setting, greeting, welcome, startup, banner]
  related: [cmd-chat, classic-vs-tui]
---

# chat.greeting.enabled

Control whether the welcome screen displays when starting a chat session.

## Overview

The `chat.greeting.enabled` setting controls whether the TUI displays the welcome screen when you start a new chat session. By default, the welcome screen is shown. Set this to `false` to skip the welcome banner and go directly to the chat prompt.

## Usage

### Disable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled false
```

### Enable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled true
```

### Get Current Value

```bash
kiro-cli settings chat.greeting.enabled
```

### Delete Setting (Restore Default)

```bash
kiro-cli settings --delete chat.greeting.enabled
```

## Value

**Type**: Boolean  
**Default**: `true` (welcome screen shown)  
**Valid values**: `true`, `false`

## Examples

### Example 1: Disable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled false
```

After this, starting a new chat session skips the welcome banner:

```bash
kiro-cli chat
```

The TUI opens directly to the input prompt without displaying "Welcome to the new Kiro CLI".

### Example 2: Re-enable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled true
```

### Example 3: Check Current Setting

```bash
kiro-cli settings chat.greeting.enabled
```

**Output**: `true` or `false`

## Configuration File

You can also set this directly in your settings file at `~/.kiro/settings/cli.json`:

```json
{
  "chat.greeting.enabled": false
}
```

## Related

- [kiro-cli chat](../commands/chat.md) - Start a chat session
- [Classic vs TUI](../features/classic-vs-tui.md) - Interface options

## Troubleshooting

### Issue: Setting Not Taking Effect

**Symptom**: Welcome screen still appears after disabling  
**Cause**: Setting not saved or session already running  
**Solution**: Verify the setting is saved, then start a new session:

```bash
kiro-cli settings chat.greeting.enabled
# Should output: false

# Start fresh session
kiro-cli chat
```

### Issue: Invalid Value Error

**Symptom**: Error when setting value  
**Cause**: Using invalid value (not true/false)  
**Solution**: Use boolean values only:

```bash
# Correct
kiro-cli settings chat.greeting.enabled false

# Incorrect
kiro-cli settings chat.greeting.enabled no
```
