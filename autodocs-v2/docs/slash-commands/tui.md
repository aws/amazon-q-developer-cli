---
doc_meta:
  title: /tui
  description: Switch to the full TUI mode from lite mode, or show TUI info panel
  category: slash_command
  keywords: [tui, mode, switch, ui, panel, full]
  related: [lite, verbosity, settings, lite-mode]
  validated: 2026-06-23
  commit: bfba631a4
  status: validated
  testable_headless: false
---

# /tui

Switch to TUI mode or show TUI info.

## Overview

Context-dependent behavior:

- **From lite mode**: switches to the full panel-based TUI, re-rendering the conversation in TUI format.
- **From TUI mode**: opens the TUI info panel (existing behavior).

The switch preserves all session state, queued messages, and conversation history.

## Usage

```
/tui
```

No arguments.

## Examples

### Switch from lite to TUI

```
/tui
```

**Output**: `System: Switched to TUI mode`

### Open TUI info panel (already in TUI mode)

```
/tui
```

**Output**: Opens the TUI information panel with version and feature details.

### Round-trip between modes

```
/lite
/tui
```

Each switch re-renders the full history in the target mode's format. No content is lost.

## Troubleshooting

### Nothing happens after /tui

If you're already in TUI mode, `/tui` opens the info panel rather than switching. Check whether you see the panel-based interface (status bar, overlay panels) — if so, you're already in TUI mode.

### History looks different after switching

Expected. TUI mode uses its own rich rendering (colored panels, tool detail views). The underlying conversation data is unchanged.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [Classic vs TUI](../features/classic-vs-tui.md) — UI mode comparison
