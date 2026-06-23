---
doc_meta:
  title: /lite
  description: Switch to lite mode - a lightweight scrollback-friendly chat interface
  category: slash_command
  keywords: [lite, mode, switch, ui, lightweight, scrollback, minimal]
  related: [tui, verbosity, settings, lite-mode]
  validated: 2026-06-05
  commit: 8a53bd9d9
  status: validated
  testable_headless: false
---

# /lite

Switch to lite mode.

## Overview

The `/lite` command switches the current session from the standard TUI to lite mode — a lightweight, scrollback-friendly chat interface. The conversation history is re-rendered in lite format immediately.

Lite mode requires the lite rollout to be enabled in your build (currently internal nightly only). If unavailable, the command shows "Lite mode is not available in this build."

## Usage

```
/lite
```

No arguments. Switches immediately (`System: Switched to lite mode`). On builds where the rollout is off, it reports `System: Lite mode is not available in this build`.

## Behavior

- The full conversation history is re-rendered in lite formatting (`You:` / `Agent:` headers); overlay panels and menus are dismissed.
- Terminal scrollback is preserved — no CSI 2J/3J clear is issued.
- `/verbosity` settings take effect immediately on the re-rendered history.

Use `/tui` to switch back. To make lite the default, see [Lite Mode → Enabling](../features/lite-mode.md#enabling-lite-mode).

## Related

- [/tui](tui.md) — Switch back to TUI mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
