---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: chat.greeting.enabled
  description: Show or hide the welcome screen when starting chat sessions
  keywords: [setting, greeting, welcome, message, welcome screen]
---

# chat.greeting.enabled

Show or hide the welcome screen when starting chat sessions.

## Overview

Controls whether Kiro CLI displays the welcome screen when starting new chat sessions. When disabled, the chat interface loads directly without showing the "Welcome to the new Kiro CLI" message.

## Usage

### Disable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled false
```

### Enable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled true
```

### Check Current Value

```bash
kiro-cli settings chat.greeting.enabled
```

## Value

**Type**: Boolean  
**Default**: `true`

## Examples

### Example 1: Disable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled false
```

The chat interface loads directly without the welcome screen.

### Example 2: Re-enable Welcome Screen

```bash
kiro-cli settings chat.greeting.enabled true
```

The welcome screen appears when starting new sessions.

### Example 3: Check Status

```bash
kiro-cli settings chat.greeting.enabled
# Output: true
```

## Technical Details

**Scope**: User-wide setting  
**Effect**: Applies to all new chat sessions  
**Storage**: `~/.kiro/settings/cli.json`

## Related

- [chat-interface-settings](chat-interface-settings.md)
