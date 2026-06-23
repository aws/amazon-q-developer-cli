---
doc_meta:
  title: /lite
  description: Switch to lite mode - a lightweight scrollback-friendly chat interface
  category: slash_command
  keywords: [lite, mode, switch, ui, lightweight, scrollback, minimal]
  related: [tui, verbosity, settings, lite-mode]
  validated: 2026-06-23
  commit: f6731f45a
  status: validated
  testable_headless: false
---

# /lite

Switch to lite mode.

## Overview

Switches the session from the standard TUI to lite mode — a lightweight, scrollback-friendly chat interface. The conversation history is re-rendered in lite format immediately.

See [Lite Mode](../features/lite-mode.md) for full behavior and configuration.

## Usage

```
/lite
```

No arguments. The command switches immediately.

## Examples

### Example 1: Switch to Lite Mode

```
/lite
```

**Output**:
```
System: Switched to lite mode
```

### Example 2: Feature Not Available

On builds where the lite rollout is disabled (currently internal nightly only):

```
/lite
```

**Output**:
```
System: Lite mode is not available in this build
```

### Example 3: Switch Back to TUI

After switching to lite, return to the full TUI:

```
/tui
```

## Behavior

- Re-renders the full conversation history in lite format
- Preserves queued messages and session state
- Does not destroy terminal scrollback
- The inverse command is `/tui`
- To make lite the default, use `/settings → display → Default UI` or set `KIRO_UI_MODE=lite`

## Related

- [/tui](tui.md) — Switch back to TUI mode
- [/verbosity](verbosity.md) — Configure lite-mode tool output rendering
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
