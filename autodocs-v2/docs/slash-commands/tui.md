---
doc_meta:
  title: /tui
  description: Switch to the full TUI mode from lite mode, or show TUI info panel
  category: slash_command
  keywords: [tui, mode, switch, ui, panel, full]
  related: [lite, verbosity, settings, lite-mode]
  validated: 2026-06-05
  commit: 8a53bd9d9
  status: validated
  testable_headless: false
---

# /tui

Switch to TUI mode or show TUI info.

Context-dependent: **from lite mode** it switches to the full panel-based TUI (re-rendering the conversation); **from TUI mode** it opens the TUI info panel.

## Usage

```
/tui
```

No arguments (`System: Switched to TUI mode`). The switch re-renders the full conversation, preserves queued messages and session state, and destroys no scrollback.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [Classic vs TUI](../features/classic-vs-tui.md) — UI mode comparison
