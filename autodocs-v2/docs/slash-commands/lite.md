---
doc_meta:
  title: /lite
  description: Switch to the lite (classic-style scrollback) UI within the TUI
  category: slash_command
  keywords: [lite, classic, scrollback, mode, switch, tui, ui, terminal]
  related: [classic-vs-tui, verbosity, settings, theme]
  validated: 2026-06-07
  commit: c0b814f63
  status: validated
  testable_headless: false
---

## Overview

The `/lite` command switches the active session from the full TUI to lite mode — a streamlined, classic-style scrollback interface. Lite mode renders assistant messages, tool calls, and subagent activity as plain scrollback text rather than overlay panels.

Lite mode requires the lite rollout to be enabled for your account. If the rollout is not active, the command has no effect.

The symmetric counterpart is `/tui`, which switches back to the full TUI. Scrollback is preserved when switching in either direction.

## Usage

```
/lite
```

Switches immediately to lite mode. Prior messages stay on screen above the transition point.

### Start in lite mode from the CLI

```bash
kiro-cli chat --lite
```

Launches the session directly in lite mode (rollout-gated).

### Switch back to TUI

```
/tui
```

Returns to the full TUI interface.

## Examples

### Switch to lite mid-session

```
/lite
```

The prompt style changes to a single-line input. Tool output renders as scrollback lines with configurable density (see `/verbosity`).

### Round-trip between modes

```
/lite
# work in lite mode...
/tui
# back to full TUI — panels, overlays, crew monitor available again
/lite
# scrollback from both modes is preserved
```

### Launch lite from the command line

```bash
kiro-cli chat --lite "Explain this codebase"
```

Starts a session in lite mode with an initial query.

## Behavior

- **Scrollback preserved**: Messages rendered before the switch remain on screen. New messages render in the destination mode's style.
- **Settings shared**: Both modes read from the same settings backend (`~/.kiro/settings/cli.json`). Changes made in `/settings` apply regardless of which mode you're in.
- **Verbosity**: Lite mode surfaces a `/verbosity` menu (also reachable via `/settings → verbosity`) that controls output density — tool args, reasoning, elapsed time, output filters, and subagent detail. See [/verbosity](verbosity.md).
- **Daily tip**: Lite mode shows a rotating tip below the welcome banner on session start to surface features.
- **Rollout-gated**: The `--lite` flag and `/lite` command require the lite mode rollout. Internal and nightly users have access; others depend on their rollout segment.

## Troubleshooting

### /lite does nothing

**Cause**: The lite rollout is not enabled for your account.  
**Solution**: Internal users on nightly or insider toolbox channels have access automatically. For others, the feature gates progressively.

### Scrollback looks different after switching

**Cause**: Each mode has its own renderer. Messages rendered in TUI mode keep their TUI formatting above the switch point; new messages below render in lite style.  
**Solution**: This is expected. Use `/clear` if you want a clean slate.

### Tool output not visible in lite mode

**Cause**: Output filters are configured to hide that tool category.  
**Solution**: Use `/verbosity` → Show output to enable the relevant category (shell, read, web, grep, glob, code, introspect, task, subagent, mcp) or select "all".

## Related

- [Classic vs TUI](../features/classic-vs-tui.md) — Full comparison of all UI modes
- [/verbosity](verbosity.md) — Configure lite-mode output density
- [/settings](settings.md) — Settings menu (includes verbosity entry in lite mode)
- [/theme](theme.md) — Theme applies to both modes
