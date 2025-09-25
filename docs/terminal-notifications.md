# Terminal Notifications

Amazon Q CLI supports terminal notifications to alert users when attention is required or when tasks are completed. The notification system uses modern terminal escape sequences with fallback support for older terminals.

## OSC 9 Notifications

The CLI uses OSC 9 (Operating System Command 9) escape sequences to send notifications directly to supported terminal applications. This provides native desktop notifications without requiring external notification systems.

### Supported Terminals

OSC 9 notifications are supported by:
- **iTerm2** (macOS)
- **Ghostty** (cross-platform)

The CLI automatically detects terminal support by checking the `TERM_PROGRAM` and `TERM` environment variables.

### Fallback Behavior

For terminals that don't support OSC 9, the CLI falls back to the ASCII bell character (`\x07`) for compatible terminals including:
- xterm variants (xterm, xterm-256color)
- screen/tmux sessions
- rxvt/urxvt
- Linux console
- KDE Konsole
- GNOME Terminal
- Alacritty

## When Notifications Occur

Notifications are triggered in two scenarios:

### 1. Tool Confirmation Required
When Amazon Q suggests using a tool that requires user confirmation, a notification is sent with the message:
> "Amazon Q needs your attention"

This occurs when:
- File operations are suggested
- External commands need approval
- Any action requiring explicit user consent

### 2. Task Completion
When Amazon Q completes a response without requiring further user input, a notification is sent with the message:
> "Amazon Q has completed its work"

This occurs when:
- A conversation response is finished
- No additional tools or confirmations are needed

## Configuration

Terminal notifications can be enabled/disabled through the CLI settings:

```bash
q settings chat.enableNotifications true   # Enable notifications
q settings chat.enableNotifications false  # Disable notifications
```

When disabled, no notifications (OSC 9 or bell) will be sent regardless of terminal support.

## Technical Implementation

The notification system:
1. Checks if notifications are enabled in user settings
2. Determines terminal support for OSC 9 via environment variables
3. Sends appropriate notification:
   - OSC 9 sequence: `\x1b]9;{message}\x07`
   - Bell fallback: `\x07`
4. Flushes stdout to ensure immediate delivery

This approach ensures notifications work across different terminal environments while respecting user preferences.
