---
doc_meta:
  title: lite-mode
  description: Lightweight scrollback-friendly TUI mode with configurable tool output verbosity
  category: feature
  keywords: [lite, lightweight, scrollback, ui mode, verbosity, density, minimal, lean, classic]
  related: [classic-vs-tui, settings]
  validated: 2026-06-23
  commit: f6731f45a
  status: validated
  testable_headless: true
---

# Lite Mode

A lightweight, scrollback-friendly chat interface.

## Overview

Lite mode is an alternative rendering mode for the TUI that prioritizes a clean, text-based scrollback experience. Instead of the full React/Ink panel-based interface, lite mode renders conversation directly to the terminal using simple `You:` / `Agent:` headers, making the output easy to scroll through and copy.

Key differences from the standard TUI:

- No overlay panels — commands render inline in scrollback
- Tool calls show as compact one-line summaries with configurable detail
- Output stays in terminal scrollback (scrollable with your terminal's own scroll)
- Configurable density presets control how much tool detail appears
- `/verbosity` command lets you fine-tune what renders

Lite mode is currently available on internal nightly builds. First-time eligible users are prompted to pick their default mode at launch.

## Enabling Lite Mode

Three ways to enter lite mode:

### Mid-Session Switch

```
/lite
```

Use `/tui` to switch back. Both commands re-render the full conversation in the target mode's format.

### Environment Variable

```bash
KIRO_UI_MODE=lite kiro-cli chat
```

### Settings File

Set in `~/.kiro/settings/cli.json` (or use `/settings → display → Default UI`):

```json
{
  "chat.ui.mode": "lite"
}
```

### Priority Order

The mode is resolved highest-priority first:

1. `KIRO_UI_MODE` environment variable
2. `chat.ui.mode` setting in `cli.json`
3. Default (`tui`)

## Configuring Output

Density presets (`minimal`, `lean`, `default`, `full`) and the individual tool-output knobs are documented in [/verbosity](../slash-commands/verbosity.md).

Quick preset examples:

```
/verbosity minimal    # hide tool args, reasoning, elapsed, output
/verbosity full       # show everything uncapped
/verbosity +shell     # add shell output to filters
/verbosity -read      # remove read output from filters
```

## Troubleshooting

- **"Lite mode is not available in this build"** — the feature is gated to internal nightly builds.
- **Tool output not showing** — check `/verbosity status`; empty filters render no output bars (`/verbosity on` enables all).
- **Switching modes loses nothing** — lite and TUI share the same session format. Switching mid-session preserves all history.

## Related

- [Classic vs TUI](classic-vs-tui.md) — UI mode comparison
- [/settings](../slash-commands/settings.md) — Settings management
- [/verbosity](../slash-commands/verbosity.md) — Tool output configuration
- [/theme](../slash-commands/theme.md) — Theme customization
