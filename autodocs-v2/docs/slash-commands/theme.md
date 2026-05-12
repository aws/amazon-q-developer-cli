---
doc_meta:
  title: /theme
  description: Select and customize the terminal color theme
  category: slash_command
  keywords: [theme, colors, dark, light, auto, custom, appearance, prompt, diff, NO_COLOR]
  related: [settings]
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
---

## Overview

The `/theme` command lets you select and customize the terminal color theme. The terminal background color is auto-detected on startup.

Theme preferences are saved to `~/.kiro/settings/kiro_cli_theme.json` and persist across sessions.

`/theme` can also be reached as `/settings theme` — both entry points open the same menu.

## Usage

```
/theme
```

Opens the theme selection menu with four options:

- **Auto** — Uses auto-detected theme based on terminal background
- **Dark Theme** — Switches to dark base theme with optimized colors
- **Light Theme** — Switches to light base theme with optimized colors
- **Custom** — Configure prompt, response, and diff colors separately

When you select Dark Theme or Light Theme, the entire base theme switches (not just accent colors), ensuring all UI elements render correctly for your terminal background.

## Examples

### Select a bundled theme

```
/theme
```

Select "Dark Theme" or "Light Theme" from the menu.

### Reset to auto-detected theme

```
/theme bundled:default
```

Resets all customizations and returns to auto-detected theme.

### Customize individual elements

```
/theme
```

Select "Custom" to configure:

- **Prompt style** — Default, Purple, Ocean, Forest, Paper
- **Response text color** — Default, Light, Dark
- **Code diff colors** — Default, Dark, Light, Accessible Dark, Accessible Light

## Environment Variables

The TUI respects the `NO_COLOR` environment variable. When set, color output is disabled regardless of theme settings.

```bash
NO_COLOR=1 kiro-cli chat
```

## Troubleshooting

### Colors look wrong

Try `/theme bundled:default` to reset to auto-detected theme.

If colors render as black or are unreadable on terminals with remapped ANSI palettes, select a bundled theme (Dark or Light) explicitly rather than relying on auto-detection.

### Theme not persisting

Check file permissions on `~/.kiro/settings/kiro_cli_theme.json`.

## Related

- [/settings](settings.md) — Settings menu entry; `/settings theme` opens this command
- [Settings](../commands/settings.md) — Other configuration options
