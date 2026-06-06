---
doc_meta:
  title: /lite
  description: Switch to lite mode - a lightweight scrollback-friendly chat interface
  category: slash_command
  keywords: [lite, mode, switch, ui, lightweight, scrollback, minimal]
  related: [tui, verbosity, settings, lite-mode]
  validated: 2026-06-06
  commit: 80052afab
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

No arguments. Switches immediately.

## Examples

### Example 1: Switch from TUI to Lite

```
/lite
```

**Output:**
```
System: Switched to lite mode
```

### Example 2: Already in Lite Mode

Running `/lite` while already in lite mode re-renders the conversation but produces no additional feedback beyond the mode announcement.

### Example 3: Unavailable Build

```
/lite
```

**Output (stable builds):**
```
System: Lite mode is not available in this build
```

## Behavior

- The full conversation history is re-rendered using lite formatting (simple `You:` / `Agent:` headers)
- All overlay panels and menus are dismissed
- The terminal scrollback is preserved — no CSI 2J/3J clear is issued
- Verbosity settings from `/verbosity` take effect immediately on the re-rendered history
- The mode switch is persisted for the session and reported in telemetry

## Switching Back

Use `/tui` to return to the full panel-based TUI interface.

## Setting Lite as Default

To make lite mode your default so it activates on every new session:

1. `/settings → display → Default UI → lite`
2. Or set `"chat.ui.mode": "lite"` in `~/.kiro/settings/cli.json`
3. Or pass `--lite` on the command line

## Troubleshooting

### "Lite mode is not available in this build"

The lite feature is gated behind a rollout flag. It requires an internal nightly build with `KIRO_LITE_ROLLOUT_ENABLED=1` set by the Rust launcher.

### Tool output not visible after switching

Lite mode starts with default verbosity (shell output only). Use `/verbosity on` to show all tool output or `/verbosity` to open the interactive menu.

## Related

- [/tui](tui.md) — Switch back to TUI mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
