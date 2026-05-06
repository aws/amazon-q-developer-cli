---
doc_meta:
  title: /settings
  description: Open the settings menu to configure theme, keybindings, and other preferences
  category: slash_command
  keywords: [settings, preferences, config, theme, keybindings, configure]
  related: [theme]
  validated: 2026-05-06
  status: validated
  testable_headless: false
---

## Overview

The `/settings` command opens a menu for configuring Kiro's user-facing preferences. It is the single entry point for preference-style configuration inside the TUI.

Preference changes are persisted to disk and apply across all sessions.

## Usage

```
/settings
```

Opens the settings menu with the available subcommands as options. Select an entry (↑↓, Enter) to drill in. Esc from a subcommand returns to the `/settings` menu; Esc from the top-level menu closes the overlay.

You can also jump directly to a subcommand:

```
/settings <subcommand>
```

## Subcommands

| Subcommand    | Description                             | Details |
|---------------|-----------------------------------------|---------|
| `theme`       | Customize colors and styling            | See [/theme](theme.md) |
| `keybindings` | View configurable keyboard shortcuts    | Read-only; edit in `~/.kiro/settings.json` |

### theme

Opens the theme selection menu. Equivalent to `/theme`.

### keybindings

Shows a read-only view of the three configurable keyboard shortcuts: cancel streaming, dismiss overlay, and quit. Each row shows the current value and a `[default]` label when unchanged.

Editing is not available inside the TUI for this release — users remap bindings by editing `~/.kiro/settings.json` directly. See the CLI [`settings`](../commands/settings.md) documentation for the full list of `chat.keybindings.*` keys.

## Examples

### Open the settings menu

```
/settings
```

### Open the theme menu directly

```
/settings theme
```

### View current keybindings

```
/settings keybindings
```

## Related

- [/theme](theme.md) — Open the theme menu directly (also accessible via `/settings theme`)
- [Settings (CLI)](../commands/settings.md) — `kiro-cli settings` for configuration via CLI, including remapping `chat.keybindings.*`
