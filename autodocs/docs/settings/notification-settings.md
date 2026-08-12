---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableNotifications
  description: Configure terminal notifications when responses complete or tool approval is needed
  keywords: [setting, notification, bell, alert, sound, osc9, terminal]
  related: []
---

# Terminal Notification Settings

Configure terminal notifications to alert you when Kiro finishes a response or needs tool approval.

## Overview

Kiro CLI can send terminal notifications when:
- A response completes (turn ends without error)
- Tool approval is required

Two settings control this behavior:
- `chat.enableNotifications` - Enable or disable notifications
- `chat.notificationMethod` - Choose notification method (auto-detected by default)

## Settings

### chat.enableNotifications

**Type**: Boolean  
**Default**: `false`

Enable or disable terminal notifications.

```bash
# Enable notifications
kiro-cli settings chat.enableNotifications true

# Disable notifications
kiro-cli settings chat.enableNotifications false

# Check current value
kiro-cli settings chat.enableNotifications
```

### chat.notificationMethod

**Type**: String  
**Default**: Auto-detected based on terminal

Controls how notifications are sent. Values:
- `bel` - ASCII bell character (works in most terminals)
- `osc9` - OSC 9 escape sequence (richer notifications in supported terminals)
- *unset* - Auto-detect based on terminal

```bash
# Force bell notifications
kiro-cli settings chat.notificationMethod bel

# Force OSC 9 notifications
kiro-cli settings chat.notificationMethod osc9

# Reset to auto-detect
kiro-cli settings -d chat.notificationMethod
```

## Terminal Support

### OSC 9 (auto-detected)
- Ghostty
- iTerm2
- WezTerm
- Windows Terminal

### Bell (auto-detected)
- xterm / xterm-256color
- screen / tmux
- Alacritty
- Konsole
- GNOME Terminal
- Emacs eat

## Examples

### Example 1: Enable with Auto-Detection

```bash
kiro-cli settings chat.enableNotifications true
```

Notifications use the best method for your terminal.

### Example 2: Force Bell for tmux

```bash
kiro-cli settings chat.enableNotifications true
kiro-cli settings chat.notificationMethod bel
```

Useful if auto-detection doesn't work in your setup.

### Example 3: Disable Notifications

```bash
kiro-cli settings chat.enableNotifications false
```

## Troubleshooting

### No notification heard
- Verify `chat.enableNotifications` is `true`
- Check your terminal's notification/bell settings
- Try setting `chat.notificationMethod` explicitly to `bel`

### Notification works but no sound
- Some terminals show visual bell instead of audio
- Check terminal preferences for bell/alert settings

### OSC 9 not working
- Your terminal may not support OSC 9
- Set `chat.notificationMethod` to `bel` as fallback

## Technical Details

**Scope**: User-wide setting

**Events that trigger notifications**:
- Response completion (processing ends without error)
- Tool approval request (pending approval appears)
