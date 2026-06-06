---
doc_meta:
  title: /settings
  description: Open the settings menu to configure theme, keybindings, terminal, display, and verbosity
  category: slash_command
  keywords: [settings, preferences, config, theme, keybindings, terminal, history, configure, multi-line, shift-enter, tmux, title, verbosity, display, ui-mode]
  related: [theme, title, verbosity, lite-mode]
  validated: 2026-06-06
  commit: 925dfcd04
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

| Subcommand    | Description                                                      | Details |
|---------------|------------------------------------------------------------------|---------|
| `display`     | Default UI at startup, animations, ASCII art, icons, and thinking | Toggle display preferences |
| `verbosity`   | Tool args, reasoning, output filters, density (lite mode only)   | See [/verbosity](verbosity.md) |
| `theme`       | Colors, prompt style, diff styling                               | See [/theme](theme.md) |
| `keybindings` | View configurable keyboard shortcuts                             | Read-only; edit in `~/.kiro/settings.json` |
| `terminal`    | Shift+Enter / Option+Enter for newlines                          | Configures your terminal app |
| `history`     | Prompt history scope (session or global)                         | Choose between per-session or shared history |

### display

Opens a toggle panel for display settings. Use ↑↓ to navigate, ←→ to toggle, Enter to apply and close, Esc to go back.

| Setting | Key | Default | Effect |
|---------|-----|---------|--------|
| Default UI mode | `chat.ui.mode` | tui | Choose between `tui` (full TUI) and `lite` (minimal append-only) at startup. Only shown when the Lite rollout is enabled |
| Animations | `chat.allowAnimations` | on | When off, spinners, progress bars, and loading effects show static frames |
| ASCII art | `chat.allowAsciiArt` | on (Unicode) | When off, replaces decorative text art including table lines with plain ASCII |
| Icons | `chat.allowIcons` | on | When off, hides symbols for status, actions, and labels |
| Show thinking | `chat.showThinking` | on | When off, suppresses the model's freeform thinking content. Shared with lite mode's verbosity setting |
| Terminal title | `chat.terminalTitle` | off | When on, updates the terminal window title with session info via OSC 0 sequences |

Changes take effect immediately without restart. Settings persist to `~/.kiro/settings/cli.json`.

**Environment variable override**: `KIRO_ASCII_MODE=1` forces ASCII mode regardless of the setting.

### verbosity

Opens the lite-mode verbosity configuration menu. Equivalent to `/verbosity`. Only available when the UI is in lite mode — in TUI mode, the entry is hidden from the menu.

See [/verbosity](verbosity.md) for full documentation.

### theme

Opens the theme selection menu. Equivalent to `/theme`.

### keybindings

Shows a read-only view of the three configurable keyboard shortcuts: cancel streaming, dismiss overlay, and quit. Each row shows the current value and a `[default]` label when unchanged.

Editing is not available inside the TUI for this release — users remap bindings by editing `~/.kiro/settings.json` directly. See the CLI [`settings`](../commands/settings.md) documentation for the full list of `chat.keybindings.*` keys.

### terminal

Configures your terminal application so Shift+Enter (or Option+Enter on macOS Terminal) inserts a newline in the prompt instead of submitting.

Behavior depends on the detected terminal:

- **Native support** (iTerm2, WezTerm, Ghostty, Kitty, Warp): nothing to install — shows a confirmation.
- **Needs config** (VS Code, Cursor, Windsurf, Alacritty, Zed, macOS Terminal): automatically writes the key binding to the terminal's config file (with a `.bak` backup). You may need to restart the terminal for changes to take effect.
- **Not supported** (Windows Terminal, gnome-terminal, JetBrains IDEs, etc.): shows the workaround (use `Ctrl+J` or `\` followed by Enter).

**tmux users**: if you run Kiro inside tmux under a native-support terminal (iTerm2, Kitty, etc.) and Shift+Enter doesn't work, tmux is probably filtering the extended-key sequence. Add the following to your `~/.tmux.conf`:

```
set -s extended-keys on
set -as terminal-features 'xterm*:extkeys'
```

Then reload tmux with `tmux source-file ~/.tmux.conf`. `/settings terminal` surfaces this reminder automatically when it detects you're in tmux.

The result is shown as a transient notification; the settings overlay closes automatically.

#### What gets changed

Before modifying anything, the command writes a `.bak` of the file it's about to edit (for Apple Terminal, it exports the full `com.apple.Terminal` plist). Restoring is just a file copy.

| Terminal | What's written | Where |
|----------|----------------|-------|
| VS Code / Cursor / Windsurf | Appends a keybinding: `shift+enter` → `workbench.action.terminal.sendSequence` with `\u001b\r` (Esc + CR) | `~/Library/Application Support/<app>/User/keybindings.json` (Linux / Windows paths differ) |
| Alacritty | Appends `[[keyboard.bindings]]` entry mapping Shift+Return to `\u001B\r` | `~/.config/alacritty/alacritty.toml` (or `$XDG_CONFIG_HOME`) |
| Zed | Appends a `Terminal`-scoped `shift-enter` binding sending `\u001b\r` | `~/.config/zed/keymap.json` |
| Apple Terminal | Sets `useOptionAsMetaKey=true` **and** `Bell=false` on the default and startup profiles, then `killall cfprefsd` to flush the cache | `~/Library/Preferences/com.apple.Terminal.plist` |
| iTerm2 / WezTerm / Ghostty / Kitty / Warp | Nothing — they already support Shift+Enter natively | — |
| Windows Terminal, gnome-terminal, JetBrains IDEs, etc. | Nothing — unsupported; use `Ctrl+J` as the workaround | — |

**Why Apple Terminal also flips the bell**: enabling `useOptionAsMetaKey` causes Option+<char> sequences to be delivered as escape sequences, which also trip the audio bell on some keypresses. Switching to visual bell avoids the terminal beeping on every Option+Enter. If you'd rather keep the audio bell, you can flip it back in Terminal.app → Settings → <profile> → Advanced → "Audible bell".

### history

Choose where prompt history is stored. Two options:

- **Session** (default) — Each session has its own prompt history; ↑ in the prompt only recalls inputs typed in the current session.
- **Global** — All sessions share one prompt history; ↑ recalls inputs from any session.

The active choice is marked with `●` next to the row label. Changes take effect on the next session.

The `chat.historyMode` setting persists to `~/.kiro/settings/cli.json`.

## Examples

### Open the settings menu

```
/settings
```

### Configure display accessibility

```
/settings display
```

### Open the theme menu directly

```
/settings theme
```

### View current keybindings

```
/settings keybindings
```

### Set up multi-line input

```
/settings terminal
```

### Choose prompt history scope

```
/settings history
```

### Open verbosity configuration (lite mode)

```
/settings verbosity
```

## Related

- [/theme](theme.md) — Open the theme menu directly (also accessible via `/settings theme`)
- [/verbosity](verbosity.md) — Configure lite-mode rendering density (also accessible via `/settings verbosity`)
- [Settings (CLI)](../commands/settings.md) — `kiro-cli settings` for configuration via CLI, including remapping `chat.keybindings.*`
