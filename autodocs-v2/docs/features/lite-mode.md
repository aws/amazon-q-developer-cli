---
doc_meta:
  title: Lite UI Mode
  description: Minimal append-only chat interface that lives in your terminal scrollback
  category: feature
  keywords: [lite, minimal, append-only, tui, mode, classic, scrollback, ui, interface, switch]
  related: [classic-vs-tui, settings]
  validated: 2026-06-06
  commit: 925dfcd04
  status: validated
  testable_headless: true
---

## Overview

Lite mode is a minimal, append-only chat interface that renders directly into your terminal scrollback rather than taking over the full screen like the TUI. It's designed for users who prefer a classic terminal experience — output stays in your scroll history, and the interface uses no alternate screen buffer.

Lite mode is available to internal and nightly users. It is gated behind the `Feature::Lite` rollout.

## Availability

Lite mode requires the rollout gate to be enabled. It's available when:

- Running a nightly build, **or**
- Running from the toolbox `insider` channel, **or**
- Using an internal (Amazon) identity

If the gate is not enabled, the `--lite` flag is accepted but has no effect, and `/lite` is not available.

## Usage

### Launch directly into Lite mode

```bash
kiro-cli chat --lite
```

### Switch modes mid-session

From the TUI, type:

```
/lite
```

From Lite mode, type:

```
/tui
```

Both commands switch immediately. The conversation history is preserved — only the rendering changes. Switching from TUI to Lite skips re-emitting messages already visible on screen; switching from Lite to TUI re-renders the full conversation in the TUI's panel layout.

### First-launch UI mode picker

On first launch (when no `chat.ui.mode` setting exists), users in the rollout see a picker:

```
Pick the UI Kiro CLI launches into by default.

  Full TUI   Chrome, panels, status bar — the standard experience
  Lite       Minimal append-only chat that lives in your scrollback
```

The choice persists to `~/.kiro/settings/cli.json` as `chat.ui.mode`. You can change it later via `/settings` → display, or override per-launch with `--lite`.

## How Lite Mode Differs from the TUI

| Aspect | Lite | Full TUI |
|--------|------|----------|
| Screen buffer | Normal scrollback | Alternate screen (full-screen) |
| Output | Append-only, stays in history | Re-rendered React/Ink components |
| Tool output | Configurable via `/verbosity` | Fixed rich rendering with collapse |
| Panels/overlays | None | `/help`, `/context`, `/tools` panels |
| Crew monitor | Inline pipeline summary | Ctrl+G overlay |
| Input | Single-line prompt | Segment-based with chips |

## Settings

### `chat.ui.mode`

Controls which mode launches by default. Values: `tui`, `lite`.

Written by the first-launch picker and by `/settings` → display. Also accepts the alias key `chat.uiMode`.

Persisted at `~/.kiro/settings/cli.json`.

### Environment variables

| Variable | Effect |
|----------|--------|
| `KIRO_LITE_ROLLOUT_ENABLED` | Set by the CLI launcher when the Lite rollout is active; the TUI reads this to decide whether to honor lite-mode requests |
| `KIRO_LITE_VERBOSE` | When `1` and no saved verbosity config exists, seeds output filters to `['all']` (show all tool output) |

## Examples

### Start a lite session with a question

```bash
kiro-cli chat --lite "explain this codebase"
```

### Switch to lite mid-session

```
/lite
```

Output:
```
ℹ Switched to lite mode
```

### Switch back to TUI

```
/tui
```

### Override the default mode per-launch

```bash
# Default is lite, but launch in TUI for this session
kiro-cli chat
# (launches in whatever chat.ui.mode is set to)

# Explicit override regardless of setting
kiro-cli chat --lite
```

## Troubleshooting

### `/lite` says "not available"

The Lite rollout gate is not enabled for your build. Check that you're running a nightly or insider build.

### Output looks garbled after switching modes

Some terminals don't fully clear the alternate screen buffer on exit. Try `clear` or open a new terminal tab.

### First-launch picker doesn't appear

The picker only shows when:
1. No `chat.ui.mode` value exists in `~/.kiro/settings/cli.json`
2. The Lite rollout is enabled for your build

If you've already launched before, the setting was persisted (defaulting to `tui`). Delete the `chat.ui.mode` key from `~/.kiro/settings/cli.json` to see the picker again.

## Related

- [Classic Mode vs New TUI](classic-vs-tui.md) — Comparison of V1 classic mode and the full TUI
- [/settings](../slash-commands/settings.md) — Change the default UI mode via display settings
- [/verbosity](../slash-commands/verbosity.md) — Configure lite-mode rendering density
