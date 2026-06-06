---
doc_meta:
  validated: 2026-06-06
  commit: fcddc2183
  status: validated
  testable_headless: false
  category: feature
  title: Classic Mode vs New TUI
  description: Differences between classic mode (V1) and the new TUI experience, including what changed, what's new, and how to switch
  keywords: [classic, tui, v1, v2, migration, legacy, differences, new, paste, chip, lite]
  related: [help, theme, spawn, feedback, lite-mode]
---

## Overview

Kiro CLI has three interfaces: the new TUI (default), lite mode, and classic mode. The new TUI is a React/Ink-based terminal interface with richer UI, lite mode is a minimal append-only chat that lives in your scrollback, and classic mode is the original Rust-based experience.

Switch to classic mode anytime with `kiro-cli --classic`. Switch to lite mode with `kiro-cli chat --lite` or `/lite` mid-session (requires the Lite rollout). See [Lite Mode](lite-mode.md) for full details.

## What's New in the TUI

### Themes and Customization

`/theme` lets you pick from Auto, Dark, Light themes or create a Custom theme with separate prompt style, response color, and diff color presets. Terminal background color is auto-detected.

### Overlay Panels

`/help`, `/context`, `/tools`, `/mcp`, `/knowledge`, `/code` all render as overlay panels — searchable, scrollable, dismissible with Esc. No more inline text dumps.

### Rich Tool Rendering

12+ specialized components for different tool types. Syntax-highlighted diffs with line numbers, collapsible output (Ctrl+O to toggle), and status icons per tool (spinner/✓/✗/⏸).

### Crew Monitor (Ctrl+G)

Visualize subagent activity in real-time. See what each agent in a multi-agent session is doing.

### Activity Tray (Ctrl+X)

Track task list progress and queued messages. Type your next message while the agent is still working — messages queue and process in order.

### Input Improvements

- Ctrl+R for reverse incremental history search
- Full kill ring with accumulation and rotation
- Undo with Ctrl+_ (100-entry stack)
- Shift+Enter for multi-line input (terminal-dependent)
- Segment-based input with file and paste chips
- Async @ file search with debounce

### Paste Chips

When you paste text longer than 10 lines, the TUI collapses it into a compact chip (e.g., `12 lines ▸`) instead of flooding your input. This keeps the prompt readable while preserving the full content.

**Expanding chips**: Press Tab when your cursor is on a paste chip to expand it back into inline editable text. A hint ("Press Tab to expand") appears after pasting.

**Undo**: Press Ctrl+_ after expanding to restore the collapsed chip.

**Multiple chips**: You can paste multiple times — each creates a separate chip. Typing between pastes inserts text between chips.

### New Commands

- `/spawn` — run parallel agent sessions with a task
- `/copy` — copy last response to clipboard
- `/transcript` — open full conversation in $PAGER
- `/theme` — customize terminal colors
- `/feedback` — submit feedback (replaces `/issue`)
- `/guide` — conversational help from the guide agent
- `/exit` — alias for `/quit`

### Approval Snackbar

Tool permission prompts appear as a snackbar above your input. y/n/t with granular trust options and drill-in feedback mode.

**Drill-in feedback**: When you reject a tool call, type a message explaining why. Your feedback is passed to the agent so it can adjust its approach, and displayed below the rejected tool in your conversation history.

## What Changed

### Shell Tool Behavior

This is the biggest behavioral change:

1. **Output is buffered, not streamed** — Commands like `npm install`, `cargo build` show no output until they complete. The command IS running — you just won't see progress.
2. **Interactive commands don't work** — `rm -i`, `npm init`, `sudo`, `ssh` host key prompts will not receive input. Use non-interactive alternatives: `npm init -y`, `rm` without `-i`, `ssh -o StrictHostKeyChecking=accept-new`.

### Help System

- Classic: `/help` launched a built-in help agent for interactive Q&A
- New TUI: `/help` shows a searchable command panel. Use `/guide` for conversational help from the guide agent.

### Commands Not Available in TUI

| Command | Notes |
|---------|-------|
| `/changelog` | Not ported |
| `/logdump` | Not ported |
| `/experiment` | No runtime experiment framework in TUI |
| `/issue` | Replaced by `/feedback` |
| `/tangent` | Was experiment-gated in classic |
| `/checkpoint` | Was experiment-gated in classic |

### Subcommands with Reduced Coverage

| Command | Missing in TUI |
|---------|---------------|
| `/agent` | generate, schema, set-default, delete |
| `/tools` | schema |
| `/prompts` | create, edit, remove, details (selection only) |
| `/knowledge` | fix |

### Settings

- Classic mode had 50+ settings with a `/settings` slash command
- The TUI consumes settings from the same backend (Session > Workspace > Global)
- No `/settings` slash command in TUI — use `kiro-cli settings` outside chat
- Some classic settings don't apply to the TUI (e.g., `chat.editMode`, `chat.diffTool`)

### External Diff Tools

Classic mode supported `chat.diffTool` with delta, difft, meld, VS Code, and icdiff. The TUI uses a built-in diff viewer with syntax highlighting, line numbers, and theming.

### Input Differences

- Vi edit mode (`chat.editMode`) not available in TUI
- Backslash continuation (`\` at end of line) not supported
- Triple backtick code block auto-detection not supported
- Inline hints and rotating tips not available

## Switching Between Modes

### Use Classic Mode

```bash
kiro-cli --classic
kiro-cli chat --legacy-mode
```

### Session Compatibility

Sessions saved in the TUI can be loaded in classic mode and vice versa via `/chat save` and `/chat load`. However, TUI sessions created during a TUI session are not available in classic mode's session picker.

## Related

- [Lite Mode](lite-mode.md) — The minimal append-only chat interface
- [/help](../slash-commands/help.md) — Command reference panel
- [/guide](../slash-commands/guide.md) — Conversational help
- [/theme](../slash-commands/theme.md) — Theme customization
- [/feedback](../slash-commands/feedback.md) — Submit feedback
