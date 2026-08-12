---
doc_meta:
  validated: 2026-08-12
  commit: 20bc37f97
  status: validated
  testable_headless: false
  category: settings-group
  title: Notification Settings
  description: Settings for terminal notifications when responses complete or approval is needed
  keywords: [settings, notifications, bell, osc9, terminal, alert]
  related: [telemetry-privacy-settings]
---

# Notification Settings

Configure terminal notifications for Kiro CLI events.

## chat.enableNotifications

Enable or disable terminal notifications.

### Overview

Controls whether Kiro CLI sends terminal notifications when:
- A response completes (turn ends without error)
- Tool approval is requested (permission required)
- Input is required (question prompt)

Notifications only fire when the terminal is **unfocused**. If you are actively viewing the terminal, no notification is sent.

> **Note:** Focus tracking is unavailable on Windows and when `/dev/tty` cannot be opened. On these platforms, notifications will not fire because the terminal is assumed to be focused.

### Usage

```bash
kiro-cli settings chat.enableNotifications true
```

**Type**: Boolean  
**Default**: `false` (disabled)  
**Scope**: Workspace-overridable (settable globally or per-workspace)

### Examples

```bash
# Enable notifications
kiro-cli settings chat.enableNotifications true

# Disable notifications
kiro-cli settings chat.enableNotifications false

# Check current value
kiro-cli settings chat.enableNotifications
```

---

## chat.notificationMethod

Set the notification method used by the terminal.

### Overview

Controls how notifications are delivered. When not set, Kiro CLI auto-detects the best method based on your terminal.

**Available methods**:
- `auto` — Auto-detect based on terminal (equivalent to leaving the setting unset)
- `bel` — ASCII bell character (works in most terminals)
- `osc9` — OSC 9 escape sequence (richer notifications in supported terminals)

**Auto-detection logic**:
- Terminals like Ghostty, iTerm, WezTerm, and Windows Terminal use `osc9`
- Standard terminals (xterm, tmux, Alacritty, Konsole, GNOME Terminal, etc.) use `bel`
- Unknown terminals receive no notification

### Usage

```bash
kiro-cli settings chat.notificationMethod <method>
```

**Type**: String (`auto` | `bel` | `osc9`)  
**Default**: Auto-detected based on terminal  
**Scope**: Workspace-overridable (settable globally or per-workspace)

### Examples

```bash
# Force bell notifications
kiro-cli settings chat.notificationMethod bel

# Force OSC 9 notifications (shows message in supported terminals)
kiro-cli settings chat.notificationMethod osc9

# Use auto-detection explicitly
kiro-cli settings chat.notificationMethod auto

# Clear to use auto-detection
kiro-cli settings --delete chat.notificationMethod

# Check current value
kiro-cli settings chat.notificationMethod
```

### Terminal Compatibility

| Terminal | Auto-detected Method |
|----------|---------------------|
| Ghostty | osc9 |
| iTerm2 | osc9 |
| WezTerm | osc9 |
| Windows Terminal | osc9 |
| Alacritty | bel |
| GNOME Terminal | bel |
| Konsole | bel |
| tmux | bel |
| xterm | bel |

> **Note:** While Windows Terminal is listed as supporting `osc9`, focus tracking is unavailable on Windows. Notifications require the terminal to be unfocused, so they will not fire on Windows regardless of the configured method.

---

## Troubleshooting

### Issue: No Notification When Terminal Is Focused

**Symptom**: Notifications enabled but nothing happens while you're looking at the terminal  
**Cause**: This is by design. Notifications only fire when the terminal is **unfocused** (you've switched to another window).  
**Solution**: Switch to another application and wait for a turn to complete.

### Issue: No Notifications on Windows

**Symptom**: Notifications never fire on Windows  
**Cause**: Focus tracking relies on `/dev/tty` and DECSET mode 1004, which are unavailable on Windows. Without focus tracking, the terminal is assumed focused and notifications are suppressed.  
**Solution**: There is no workaround at this time. Focus tracking on Windows is not currently supported.

### Issue: No Notification Heard (macOS/Linux)

**Symptom**: Notifications enabled, terminal is unfocused, but no sound or alert  
**Causes**:
- Terminal not recognized (auto-detection returns nothing)
- `/dev/tty` cannot be opened (focus tracking disabled, assumed focused)
- System sound muted
- Terminal notifications disabled in OS settings

**Solutions**:
```bash
# Explicitly set a method
kiro-cli settings chat.notificationMethod bel

# Test bell works in your terminal
echo -e '\a'
```

### Issue: OSC 9 Not Working

**Symptom**: Set to `osc9` but no notification appears  
**Cause**: Terminal doesn't support OSC 9 escape sequences  
**Solution**: Switch to `bel` method or use a supported terminal

```bash
kiro-cli settings chat.notificationMethod bel
```

### Issue: Too Many Notifications

**Symptom**: Notifications are disruptive  
**Solution**: Disable notifications entirely

```bash
kiro-cli settings chat.enableNotifications false
```

## Related

- [Telemetry and Privacy Settings](telemetry-privacy-settings.md) - Other global settings
