---
doc_meta:
  title: chat.terminalTitle
  description: Update terminal window title with session info
  category: setting
  keywords: [setting, terminal, title, window, tab, display, OSC]
  related: [title, settings]
  validated: 2026-06-05
  commit: 500b3c044
  status: validated
  testable_headless: true
---

# chat.terminalTitle

Update the terminal window title with session info.

## Overview

The `chat.terminalTitle` setting controls whether Kiro updates your terminal's window/tab title to reflect the current session. When enabled, the title shows `kiro: <session-info>` where session-info is derived from the session topic or workspace path.

This setting is **global-only** — it applies across all workspaces and cannot be overridden per-project.

## Default

`false` (disabled)

**Type**: Boolean  
**Scope**: Global only  
**Key**: `chat.terminalTitle`

## Usage

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

## Examples

### Example 1: Enable Terminal Title

```bash
kiro-cli settings chat.terminalTitle true
```

Your terminal tab will now show `kiro: <workspace-name>` or the session topic once derived.

### Example 2: Disable Terminal Title

```bash
kiro-cli settings chat.terminalTitle false
```

The terminal title reverts to its default (set by your shell or terminal emulator).

### Example 3: Combine with /title for Manual Override

After enabling the setting:

```
/title debugging auth flow
```

The terminal tab shows `kiro: debugging auth flow`. Use `/title --clear` to revert to automatic derivation.

## Title Precedence

When enabled, the title is derived in this order:

1. **User override** — set via `/title <text>`, cleared via `/title --clear`
2. **Session topic** — derived from the conversation (e.g., first prompt summary)
3. **Workspace path** — shortened cwd (e.g., `my-project`)

## Notes

- Changes take effect immediately — enabling shows the title, disabling clears it.
- Titles are prefixed with `kiro: ` followed by the title text.
- Some terminals ignore OSC 0 escape sequences for security reasons.
- Under tmux, requires `set-titles on` for outer terminal updates.
- Title resets on exit (behavior varies by terminal).

## Troubleshooting

### Issue: Title Not Appearing

**Symptom**: Setting is enabled but no title shows in the terminal tab  
**Cause**: Your terminal may not support OSC 0 escape sequences  
**Solution**: Try a different terminal emulator. Most modern terminals (iTerm2, Windows Terminal, Alacritty, kitty) support OSC 0.

### Issue: Title Doesn't Update in tmux

**Symptom**: Title works outside tmux but not inside  
**Cause**: tmux doesn't forward OSC 0 by default  
**Solution**: Add to `~/.tmux.conf`:

```
set -g set-titles on
set -g set-titles-string "#{pane_title}"
```

Then reload: `tmux source-file ~/.tmux.conf`.

### Issue: Title Persists After Exiting Kiro

**Symptom**: Terminal tab still shows the Kiro title after quitting  
**Cause**: Some terminals don't reset the title when a process exits  
**Solution**: Run `printf '\033]0;\007'` to clear the title, or open a new tab.

## Related

- [/title](../slash-commands/title.md) — Manually set or clear the terminal title
- [kiro-cli settings](../commands/settings.md) — Manage all settings
