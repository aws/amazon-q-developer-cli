---
doc_meta:
  validated: 2026-05-27
  commit: fada18425
  status: validated
  testable_headless: true
  category: setting
  title: chat.terminalTitle
  description: Update terminal window title with session info
  keywords: [setting, terminal, title, window, tab, display]
  related: [title]
---

# chat.terminalTitle

Update the terminal window title with session info.

## Overview

Controls whether Kiro updates your terminal's window/tab title to reflect the current session. When enabled, the title shows `kiro: <session-info>` where session-info is derived from the session title or workspace path.

This setting is **global-only** — it applies across all workspaces.

## Default

`false` (disabled)

## Examples

### Enable via CLI

```bash
kiro-cli settings chat.terminalTitle true
```

### Enable via in-chat command

```
/settings set chat.terminalTitle true
```

### Enable via Display Settings panel

```
/settings display
```

Then toggle "Terminal title" on.

### Check current value

```bash
kiro-cli settings chat.terminalTitle
```

### Disable

```bash
kiro-cli settings chat.terminalTitle false
```

## Title Precedence

When enabled, the title is derived in this order:

1. **User override** — set via `/title <text>`, cleared via `/title --clear`
2. **Session title** — from the backend (e.g., first prompt summary)
3. **Workspace path** — shortened cwd (e.g., `~/src/my-project`)

## Notes

- Changes take effect immediately — enabling shows the title, disabling clears it.
- Titles are truncated to 60 characters.
- Some terminals ignore OSC 0 escape sequences for security reasons.
- Under tmux, requires `set-titles on` for outer terminal updates.
- Title resets on exit (behavior varies by terminal).

## Related

- `/title` command — manually set or clear the terminal title
