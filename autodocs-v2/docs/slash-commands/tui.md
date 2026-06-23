---
doc_meta:
  title: /tui
  description: Switch to the full TUI mode from lite mode, or show TUI info panel
  category: slash_command
  keywords: [tui, mode, switch, ui, panel, full]
  related: [lite, verbosity, settings, lite-mode]
  validated: 2026-06-23
  commit: f6731f45a
  status: validated
  testable_headless: false
---

# /tui

Switch to TUI mode or show TUI info.

## Overview

Context-dependent behavior:

- **From lite mode**: switches to the full panel-based TUI, re-rendering the conversation.
- **From TUI mode**: opens the TUI info panel (version, build, and environment details).

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

**Output**:
```
System: Switched to TUI mode
```

The full TUI interface renders with panels, status bar, and prompt chrome.

### Example 2: Show TUI Info Panel

When already in TUI mode:

```
/tui
```

**Output**: Opens the TUI information panel showing version, build hash, and runtime details.

### Example 3: Round-Trip Mode Switch

Switch to lite then back to TUI without losing history:

```
/lite
/tui
```

Both switches re-render the full conversation in the target mode's format.

## Behavior

- Preserves queued messages and session state across the switch
- Does not destroy terminal scrollback
- The inverse command (TUI → lite) is `/lite`
- Sessions are mode-agnostic — switching modes does not affect saved session data

## Related

- [/lite](lite.md) — Switch to lite mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [Classic vs TUI](../features/classic-vs-tui.md) — UI mode comparison
