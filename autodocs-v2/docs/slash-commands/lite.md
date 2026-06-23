---
doc_meta:
  title: /lite
  description: Switch to lite mode - a lightweight scrollback-friendly chat interface
  category: slash_command
  keywords: [lite, mode, switch, ui, lightweight, scrollback, minimal]
  related: [tui, verbosity, settings, lite-mode]
  validated: 2026-06-23
  commit: bfba631a4
  status: validated
  testable_headless: false
---

# /lite

Switch to lite mode.

## Overview

Switches the session from the standard TUI to lite mode — a lightweight, scrollback-friendly chat interface. The switch re-renders the full conversation history in lite format (simple `You:` / `Agent:` headers with compact tool summaries).

This command is only available on builds where the lite rollout is enabled (currently internal nightly). On other builds it reports an error.

## Usage

```
/lite
```

No arguments. The switch is immediate and preserves all session state.

## Examples

### Switch to lite mode

```
/lite
```

**Output**: `System: Switched to lite mode`

### Switch on unsupported build

```
/lite
```

**Output**: `System: Lite mode is not available in this build`

### Round-trip between modes

```
/lite
/tui
```

Switching back and forth preserves queued messages, session state, and conversation history. Each switch re-renders the full history in the target mode's format.

## Troubleshooting

### "Lite mode is not available in this build"

The feature is gated to internal nightly builds. Stable releases do not include lite mode yet.

### History looks different after switching

This is expected. Lite mode re-renders the entire conversation using its own format (compact tool summaries, `You:`/`Agent:` headers). No content is lost — only the visual presentation changes.

## Related

- [/tui](tui.md) — Switch back to TUI mode
- [/verbosity](verbosity.md) — Configure lite-mode rendering density
- [/settings](settings.md) — Settings management (set default UI mode)
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
