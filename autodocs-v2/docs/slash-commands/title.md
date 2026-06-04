---
doc_meta:
  title: /title
  description: Set, clear, or show the terminal window title for the current session
  category: slash_command
  keywords: [title, terminal, window, OSC, session, workspace, display]
  related: [settings]
  validated: 2026-06-03
  commit: 28e17b5ed
  status: validated
  testable_headless: false
---

## Overview

The `/title` command controls the terminal window title. You can set a custom title, clear it to revert to the automatic title, or show the current title. The terminal title is updated via OSC 0 escape sequences.

Titles auto-derive from the session topic (once a conversation starts) or the workspace directory name. Setting a manual title overrides the automatic derivation until cleared.

This feature requires the `chat.terminalTitle` setting to be enabled. When disabled (the default), `/title` and automatic title updates are no-ops.

## Usage

```
/title [text | --clear]
```

| Argument | Description |
|----------|-------------|
| (none) | Show the current terminal title |
| `<text>` | Set a custom terminal title |
| `--clear` | Clear the manual override and revert to the auto-derived title |

## Enabling the Feature

Enable terminal titles via `/settings display` (toggle "Terminal title") or the CLI:

```bash
kiro-cli settings chat.terminalTitle true
```

The setting persists to `~/.kiro/settings/cli.json`.

## Examples

### Show the current title

```
/title
```

**Output**:
```
Current title: kiro: my-project
```

### Set a custom title

```
/title debugging auth flow
```

**Output**:
```
Title set: kiro: debugging auth flow
```

### Clear the manual override

```
/title --clear
```

**Output**:
```
Title cleared (reverted to automatic)
```

The title reverts to the auto-derived value (session topic or workspace path).

## Title Format

All titles are prefixed with `kiro: ` followed by the title text:

- **Workspace-derived**: `kiro: <directory-name>` (e.g., `kiro: my-project`)
- **Session-derived**: `kiro: <session-topic>` (once the AI derives a topic from the conversation)
- **Manual override**: `kiro: <your text>`

## Troubleshooting

### Title not appearing

Ensure the setting is enabled:

```bash
kiro-cli settings chat.terminalTitle true
```

Or toggle it via `/settings display`.

### Title resets when switching tabs

Terminal titles are per-process. Each Kiro session sets its own title. When you switch terminal tabs, each tab shows the title of its own running process.

### Title doesn't work in tmux

tmux may not forward OSC 0 sequences by default. Add to `~/.tmux.conf`:

```
set -g set-titles on
set -g set-titles-string "#{pane_title}"
```

Then reload: `tmux source-file ~/.tmux.conf`.

## Related

- [/settings](settings.md) — Configure display preferences including terminal title
