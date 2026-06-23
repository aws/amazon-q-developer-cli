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

### Mid-Session Switch

```
/lite       Switch to lite mode
/tui        Switch back to TUI mode
```

Either command re-renders the full conversation history in the target mode's format.

### Environment Variable

```bash
export KIRO_UI_MODE=lite
kiro-cli chat
```

### Persistent Default

Set your default via `/settings → display → Default UI` or write directly to `~/.kiro/settings/cli.json`:

```json
{
  "chat.ui.mode": "lite"
}
```

### Resolution Order

The UI mode is resolved with this priority (highest first):

1. `KIRO_UI_MODE` environment variable
2. `chat.ui.mode` setting in `cli.json`
3. Default (`tui`)

## Density Presets

Lite mode ships four density presets (`minimal`, `lean`, `default`, `full`) that control how much tool detail renders in scrollback. Set one with `/verbosity density <preset>`. See [/verbosity](../slash-commands/verbosity.md) for the per-preset table and the individual knobs (args mode, output filters, truncation caps, interactive menu).

## Settings

The default UI mode persists in `~/.kiro/settings/cli.json`:

| Key            | Type                | Description     |
| -------------- | ------------------- | --------------- |
| `chat.ui.mode` | `"lite"` \| `"tui"` | Default UI mode |

All lite-mode verbosity keys (`chat.tools.*`, `chat.subagent.*`) are documented in [/verbosity](../slash-commands/verbosity.md#persistence).

## Troubleshooting

- **"Lite mode is not available in this build"** — the feature is gated to internal nightly builds.
- **Tool output not showing** — check `/verbosity status`; empty filters render no output bars (`/verbosity on` enables all).

## Related

- [Classic vs TUI](classic-vs-tui.md) — UI mode comparison
- [/settings](../slash-commands/settings.md) — Settings management
- [/theme](../slash-commands/theme.md) — Theme customization
