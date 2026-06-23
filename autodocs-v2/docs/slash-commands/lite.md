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

Switches the session from the standard TUI to lite mode — a lightweight, scrollback-friendly chat interface. See [Lite Mode](../features/lite-mode.md) for behavior.

## Usage

```
/lite
```

No arguments. Switches immediately (`System: Switched to lite mode`). On builds where the lite rollout is off (currently internal nightly only), it reports `System: Lite mode is not available in this build`.

Use `/tui` to switch back. To make lite the default, see [Lite Mode → Enabling](../features/lite-mode.md#enabling-lite-mode).

## Related

- [/tui](tui.md) — Switch back to TUI mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
