---
doc_meta:
  title: /theme
  description: Select and customize the terminal color theme
  category: slash_command
  keywords: [theme, colors, dark, light, auto, custom, appearance, prompt, diff]
  related: [settings]
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: false
---

## Overview

The `/theme` command lets you select and customize the terminal color theme. The terminal background color is auto-detected on startup.

Theme preferences are saved to `~/.kiro/settings/kiro_cli_theme.json` and persist across sessions.

## Usage

```
/theme
```

Opens the theme selection menu with four options:

- **Auto** — Uses auto-detected theme based on terminal background
- **Dark Theme** — Optimized for dark terminal backgrounds
- **Light Theme** — Optimized for light terminal backgrounds
- **Custom** — Configure prompt, response, and diff colors separately

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

## Troubleshooting

### Colors look wrong

Try `/theme bundled:default` to reset to auto-detected theme.

### Theme not persisting

Check file permissions on `~/.kiro/settings/kiro_cli_theme.json`.

## Related

- [Settings](../commands/settings.md) — Other configuration options
