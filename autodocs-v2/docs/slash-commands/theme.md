---
doc_meta:
  title: /theme
  description: Select and customize the terminal color theme
  category: slash_command
  keywords: [theme, colors, dark, light, safe, auto, custom, appearance, prompt, diff, NO_COLOR, KIRO_TERMINAL_THEME]
  related: [settings]
  validated: 2026-07-13
  commit: d7e570fd4
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
- **Dark theme** — Switches to dark base theme with optimized colors
- **Light theme** — Switches to light base theme with optimized colors
- **Custom** — Configure prompt, response, and diff colors through a step-by-step wizard

Selecting Auto, Dark theme, or Light theme applies immediately and closes the overlay. Selecting Custom enters a 3-step wizard:

1. **Prompt style** — Default, Purple, Ocean, Forest, Paper
2. **Response text color** — Default, Light, Dark
3. **Code diff colors** — Default, Dark, Light, Accessible Dark, Accessible Light

Each step persists on Enter and auto-advances to the next. After Step 3 you see a `Theme updated. ✓` confirmation. Esc walks back one step at a time (Step 3 → Step 2 → Step 1 → top-level → close), so you can revise an earlier choice without restarting the wizard.

A live preview block underneath the menu shows what each option will look like in the conversation — coloured `▌` bars match the user/agent message rendering, and the diff sample matches the code-diff styling.

When you select Dark theme or Light theme, the entire base theme switches (not just accent colors), ensuring all UI elements render correctly for your terminal background.

## Examples

### Select a bundled theme

```
/theme
```

Select "Dark theme" or "Light theme" from the menu.

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

### KIRO_TERMINAL_THEME

Forces the base theme, bypassing auto-detection entirely (including the OSC-11 terminal query). Useful when your terminal defeats background-color detection.

Accepted values: `dark`, `light`, `safe` (case-insensitive).

```bash
# Force dark theme
KIRO_TERMINAL_THEME=dark kiro-cli chat

# Force light theme
KIRO_TERMINAL_THEME=light kiro-cli chat

# Use safe theme (ANSI named colors that adapt to any background)
KIRO_TERMINAL_THEME=safe kiro-cli chat
```

The `safe` theme uses only ANSI named colors mapped by your terminal emulator, so it adapts naturally to both light and dark backgrounds. It is the best choice for SSH sessions or terminals where neither `dark` nor `light` renders correctly.

This variable takes precedence over the persisted theme preference in `~/.kiro/settings/kiro_cli_theme.json`.

### NO_COLOR

The TUI respects the `NO_COLOR` environment variable. When set, color output is disabled regardless of theme settings.

```bash
NO_COLOR=1 kiro-cli chat
```

## Troubleshooting

### Colors look wrong

Try `/theme bundled:default` to reset to auto-detected theme.

If colors render as black or are unreadable on terminals with remapped ANSI palettes, select a bundled theme (Dark or Light) explicitly rather than relying on auto-detection.

If your terminal defeats auto-detection entirely (e.g., multiplexers, SSH, or non-standard emulators), set `KIRO_TERMINAL_THEME=dark` or `KIRO_TERMINAL_THEME=light` in your shell profile to force the correct base theme on every launch.

### Theme not persisting

Check file permissions on `~/.kiro/settings/kiro_cli_theme.json`.

Note: `KIRO_TERMINAL_THEME` overrides the persisted setting. If the env var is set, the saved preference is ignored.

## Related

- [/settings](settings.md) — Settings menu entry; `/settings theme` opens this command
- [Settings](../commands/settings.md) — Other configuration options
