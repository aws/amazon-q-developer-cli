---
doc_meta:
  title: lite-mode
  description: Lightweight scrollback-friendly TUI mode with configurable tool output verbosity
  category: feature
  keywords:
    [
      lite,
      lightweight,
      scrollback,
      ui mode,
      verbosity,
      density,
      minimal,
      lean,
      classic,
    ]
  related: [classic-vs-tui, settings]
  validated: 2026-06-05
  commit: 8a53bd9d9
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

- `/lite` (and `/tui` to switch back) mid-session — re-renders the full history in the target mode's format.
- `KIRO_UI_MODE=lite` environment variable.
- `chat.ui.mode: "lite"` in `~/.kiro/settings/cli.json` (or `/settings → display → Default UI`).

The mode is resolved highest-priority first:

1. `KIRO_UI_MODE` environment variable
2. `chat.ui.mode` setting in `cli.json`
3. Default (`tui`)

Density presets (`minimal`, `lean`, `default`, `full`) and the individual tool-output knobs are documented in [/verbosity](../slash-commands/verbosity.md).

## Settings

The default mode persists under `chat.ui.mode` (`"lite"` | `"tui"`) in `~/.kiro/settings/cli.json`; all verbosity keys (`chat.tools.*`, `chat.subagent.*`) are documented in [/verbosity](../slash-commands/verbosity.md#persistence).

## Troubleshooting

- **"Lite mode is not available in this build"** — the feature is gated to internal nightly builds.
- **Tool output not showing** — check `/verbosity status`; empty filters render no output bars (`/verbosity on` enables all).

## Related

- [Classic vs TUI](classic-vs-tui.md) — UI mode comparison
- [/settings](../slash-commands/settings.md) — Settings management
- [/theme](../slash-commands/theme.md) — Theme customization
