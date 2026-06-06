---
doc_meta:
  title: /tui
  description: Switch to the full TUI mode from lite mode, or show TUI info panel
  category: slash_command
  keywords: [tui, mode, switch, ui, panel, full]
  related: [lite, verbosity, settings, lite-mode]
  validated: 2026-06-06
  commit: 80052afab
  status: validated
  testable_headless: false
---

# /tui

Switch to TUI mode or show TUI info.

## Overview

The `/tui` command has context-dependent behavior:

- **From lite mode**: Switches to the full panel-based TUI. The conversation is re-rendered using TUI components.
- **From TUI mode**: Opens the TUI information panel showing what's new in the TUI experience.

## Usage

```
/tui
```

No arguments.

## Examples

### Example 1: Switch from Lite to TUI

When in lite mode:

```
/tui
```

**Output:**
```
System: Switched to TUI mode
```

The interface transitions to the full React/Ink TUI with overlay panels, rich tool rendering, and the activity tray.

### Example 2: From TUI Mode

When already in TUI mode:

```
/tui
```

Opens the TUI information panel describing new features and differences from classic mode.

### Example 3: Round-trip

```
/lite
```
```
System: Switched to lite mode
```

```
/tui
```
```
System: Switched to TUI mode
```

Session state and conversation history are preserved across mode switches.

## Behavior

- Mode switches re-render the full conversation in the target format
- All pending messages in the queue are preserved
- The mode change is reported in telemetry
- No terminal scrollback is destroyed during the switch

## Troubleshooting

### Nothing happens in TUI mode

When already in TUI mode, `/tui` opens the info panel. If the panel doesn't appear, try pressing Esc first to dismiss any active overlay.

### Lite mode features disappear after /tui

Commands like `/verbosity` are lite-mode only. After switching to TUI, use `/settings` for configuration instead.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [Classic vs TUI](../features/classic-vs-tui.md) — UI mode comparison
