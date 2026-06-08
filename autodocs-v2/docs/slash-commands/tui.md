---
doc_meta:
  title: /tui
  description: Switch back to the full TUI from lite mode
  category: slash_command
  keywords: [tui, full, switch, ui, mode, panels, chrome]
  related: [lite, verbosity, classic-vs-tui]
  validated: 2026-06-08
  commit: 08bdfea87
  status: validated
  testable_headless: false
---

## Overview

The `/tui` command switches the current session from lite mode back to the full TUI — the panel-based interface with chrome, status bar, and interactive overlays.

The terminal clears and the entire conversation is re-rendered using TUI card-style formatting. This is the inverse of `/lite`.

## Usage

```
/tui
```

Switches immediately. The screen clears and the full TUI interface takes over with all messages rendered in card format.

## Examples

### Switch from lite to full TUI

```
/tui
```

**Output**:
The terminal clears and the TUI renders with the status bar, message cards, and tool-use panels.

### Round-trip between modes

```
/lite
```

Work in lite mode for a while, then:

```
/tui
```

Returns to the full TUI. The conversation content is identical — only the rendering style changes.

### No-op when already in TUI

Running `/tui` while already in TUI mode does nothing. No screen flash or re-render occurs.

## Behavior

- **Screen clear**: The terminal clears and all messages re-render in TUI card format.
- **Scrollback preserved**: Conversation content is preserved across the switch.
- **Same-mode no-op**: Running `/tui` when already in TUI mode is a silent no-op.
- **Always available**: Unlike `/lite` (which requires a rollout gate), `/tui` is always available since it returns to the default mode.

## Troubleshooting

### /tui doesn't seem to do anything

**Cause**: You're already in TUI mode.
**Solution**: This is expected. The command is a no-op when the current mode matches the target.

### Messages look different after switching back

**Cause**: TUI mode uses card-style rendering with borders, status badges, and collapsible tool panels. Lite mode uses plain text headers.
**Solution**: This is the expected visual difference between modes. Content is identical.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/verbosity](verbosity.md) — Configure lite-mode output density
- [Classic vs TUI](../features/classic-vs-tui.md) — Mode comparison and switching guide
