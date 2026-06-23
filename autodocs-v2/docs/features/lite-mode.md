---
doc_meta:
  title: lite-mode
  description: Lightweight scrollback-friendly TUI mode with configurable tool output verbosity
  category: feature
  keywords: [lite, lightweight, scrollback, ui mode, verbosity, density, minimal, lean, classic]
  related: [classic-vs-tui, settings]
  validated: 2026-06-23
  commit: bfba631a4
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

### Slash command (mid-session)

```
/lite
```

Switches immediately. Use `/tui` to switch back. Both commands re-render the full history in the target mode's format.

### Environment variable

```bash
KIRO_UI_MODE=lite kiro-cli chat
```

### Settings file

```json
{
  "chat.ui.mode": "lite"
}
```

Set via `/settings → display → Default UI`, or edit `~/.kiro/settings/cli.json` directly.

The mode is resolved highest-priority first:

1. `KIRO_UI_MODE` environment variable
2. `chat.ui.mode` setting in `cli.json`
3. Default (`tui`)

## Configuring Density

Density presets (`minimal`, `lean`, `default`, `full`) control how much tool detail appears in scrollback. Configure with `/verbosity`:

```
/verbosity minimal    # tool names only, no args or output
/verbosity lean       # inline args, no reasoning
/verbosity default    # block args, shell output, reasoning
/verbosity full       # everything, no caps
```

Individual knobs (tool args mode, reasoning, elapsed time, output filters, truncation) are adjustable via the `/verbosity` interactive menu or CLI subcommands. See [/verbosity](../slash-commands/verbosity.md) for full details.

## Troubleshooting

### "Lite mode is not available in this build"

The feature is gated to internal nightly builds. Stable releases do not include lite mode yet.

### Tool output not showing

Check `/verbosity status`. If output filters are empty, no tool output bars render. Use `/verbosity on` to enable all, or `/verbosity +shell` to add specific categories.

### `/clear` behavior

In lite mode, `/clear` clears the conversation context without wiping your terminal scrollback. This is intentional — lite mode never emits terminal clear sequences so your pre-session shell history remains intact.

## Related

- [Classic vs TUI](classic-vs-tui.md) — UI mode comparison
- [/verbosity](../slash-commands/verbosity.md) — Density and filter configuration
- [/settings](../slash-commands/settings.md) — Settings management
- [/theme](../slash-commands/theme.md) — Theme customization
