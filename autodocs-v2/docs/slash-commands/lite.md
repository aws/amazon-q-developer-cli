---
doc_meta:
  title: /lite
  description: Switch to lite mode — a minimal append-only scrollback UI within the TUI
  category: slash_command
  keywords: [lite, lite-mode, scrollback, minimal, classic, switch, ui, mode]
  related: [tui, verbosity, classic-vs-tui]
  validated: 2026-06-08
  commit: 08bdfea87
  status: validated
  testable_headless: false
---

## Overview

The `/lite` command switches the current session from the full TUI to lite mode — a minimal, append-only scrollback interface that lives in your terminal history. Prior messages are re-rendered in lite style so the conversation appears consistent.

Lite mode requires the lite rollout to be enabled for your account. If the rollout is not active, the command shows an error.

The inverse command is `/tui`, which switches back to the full TUI.

## Usage

```
/lite
```

Switches immediately. The screen clears and the entire conversation is re-rendered using lite mode formatting (plain `You:` / `Kiro:` headers, inline tool output bars).

## Examples

### Switch to lite mode mid-session

```
/lite
```

**Output**:
The terminal clears and the session is re-displayed in lite scrollback format. A system message confirms the switch.

### Switch back to TUI

```
/tui
```

Returns to the full TUI with chrome, panels, and status bar.

### Start a session in lite mode from the CLI

```bash
kiro-cli chat --lite
```

Launches directly into lite mode without needing `/lite` after startup.

## Behavior

- **Screen clear**: Both `/lite` and `/tui` clear the terminal and re-render all messages in the destination mode's style. No half-and-half mixing occurs.
- **Scrollback preserved**: The conversation content is preserved — only the rendering changes.
- **Same-mode no-op**: Running `/lite` when already in lite mode does nothing (no screen flash or re-render).
- **Verbosity**: Lite mode uses `/verbosity` settings to control output density (tool args, reasoning, filters, truncation).
- **Rollout gated**: Requires `KIRO_LITE_ROLLOUT_ENABLED` (set automatically for eligible accounts). Internal and insider-channel users have access.

## Troubleshooting

### /lite shows an error

**Cause**: The lite mode rollout is not enabled for your account.
**Solution**: Lite mode is being rolled out progressively. Internal users and insider-channel installs have immediate access.

### Screen looks blank after switching

**Cause**: If the session has no messages yet, the re-render produces only the welcome banner.
**Solution**: This is expected. Start chatting and messages will appear in lite format.

### Output too verbose or too compact

**Cause**: Lite mode rendering is controlled by verbosity settings.
**Solution**: Use `/verbosity` to adjust density, tool arg display, output filters, and truncation limits.

## Related

- [/tui](tui.md) — Switch back to the full TUI
- [/verbosity](verbosity.md) — Configure lite-mode output density
- [Classic vs TUI](../features/classic-vs-tui.md) — Mode comparison and switching guide
