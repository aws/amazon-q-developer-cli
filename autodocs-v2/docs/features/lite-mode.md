---
doc_meta:
  title: lite-mode
  description: Lightweight scrollback-friendly TUI mode with configurable tool output verbosity
  category: feature
  keywords: [lite, lightweight, scrollback, ui mode, verbosity, density, minimal, lean, classic]
  related: [classic-vs-tui, settings]
  validated: 2026-06-05
  commit: 8a53bd9d9
  status: validated
  testable_headless: true
---

# Lite Mode

A lightweight, scrollback-friendly chat interface.

## Overview

Lite mode is an alternative rendering mode for the TUI that prioritizes a clean, text-based scrollback experience. Instead of the full React/Ink panel-based interface, lite mode renders conversation directly to the terminal using simple `You:` / `Agent:` headers, making the output easy to scroll through and copy.

Key differences from the standard TUI:

- No overlay panels — commands render inline in scrollback
- Tool calls show as compact one-line summaries with configurable detail
- Output stays in terminal scrollback (scrollable with your terminal's own scroll)
- Configurable density presets control how much tool detail appears
- `/verbosity` command lets you fine-tune what renders

Lite mode is currently available on internal nightly builds. First-time eligible users are prompted to pick their default mode at launch.

## Enabling Lite Mode

### Mid-Session Switch

```
/lite
```

Switches from TUI to lite mode. The conversation is re-rendered in lite format.

### Environment Variable

```bash
export KIRO_UI_MODE=lite
kiro-cli chat
```

### Persistent Default

Set your default via `/settings → display → Default UI` or write directly to `~/.kiro/settings/cli.json`:

```json
{
  "chat.ui.mode": "lite"
}
```

### Resolution Order

The UI mode is resolved with this priority (highest first):

1. `KIRO_UI_MODE` environment variable
2. `chat.ui.mode` setting in `cli.json`
3. Default (`tui`)

## Switching Between Modes

You can switch modes at any time during a session:

```
/lite       Switch to lite mode
/tui        Switch back to TUI mode
```

Both commands re-render the full conversation history in the target mode's format.

## Density Presets

Lite mode includes four density presets that control how much tool information appears in scrollback:

| Preset | Tool Args | Reasoning | Elapsed | Output Bar | Thinking |
|--------|-----------|-----------|---------|------------|----------|
| minimal | hidden | hidden | hidden | none | hidden |
| lean | inline | hidden | shown | none | hidden |
| default | block | shown | shown | shell only | shown |
| full | block (no caps) | shown | shown | all tools | shown |

Set a preset with:

```
/verbosity density minimal
/verbosity density lean
/verbosity density default
/verbosity density full
```

## Verbosity Configuration

Beyond presets, individual knobs can be tuned:

### Tool Call Display

- **Args mode**: `off` (hidden), `inline` (one-line chip), `block` (full key:value tree)
- **Reasoning**: Show/hide the model's per-tool-call explanation
- **Elapsed time**: Show/hide execution duration

### Output Filters

Control which tools show an output bar below their call:

```
/verbosity on         Show output for all tools
/verbosity off        Hide all tool output
/verbosity shell      Show only shell tool output
/verbosity +web       Add web tools to current filters
/verbosity -shell     Remove shell from current filters
```

Available filter categories: `shell`, `read`, `web`, `grep`, `glob`, `code`, `introspect`, `task`, `subagent`, `mcp`

### Truncation Caps

Limit how much detail appears per tool:

- **argsMaxLines** — max lines of args below tool name
- **argsMaxChars** — max chars per individual arg value
- **outputMaxLines** — max lines of tool output
- **outputMaxChars** — max chars per output row

### Interactive Menu

Running `/verbosity` with no arguments opens an interactive menu where you can adjust all settings visually.

## Settings Reference

Lite mode verbosity settings are persisted in `~/.kiro/settings/cli.json`:

| Key | Type | Description |
|-----|------|-------------|
| `chat.ui.mode` | `"lite"` \| `"tui"` | Default UI mode |
| `chat.tools.filters` | string[] | Tool output filter tokens |
| `chat.tools.showReasoning` | boolean | Show per-tool reasoning |
| `chat.tools.argsMode` | `"off"` \| `"inline"` \| `"block"` | Tool args display mode |
| `chat.tools.showElapsed` | boolean | Show tool execution time |
| `chat.tools.argsMaxLines` | number \| null | Args line cap |
| `chat.tools.argsMaxChars` | number \| null | Args char cap |
| `chat.tools.outputMaxLines` | number \| null | Output line cap |
| `chat.tools.outputMaxChars` | number \| null | Output char cap |
| `chat.tools.showWriteDiffs` | boolean | Show write tool diff bodies |
| `chat.showTasks` | boolean | Show task tray |
| `chat.subagent.showPipeline` | boolean | Show subagent pipeline tree |
| `chat.subagent.showPrompts` | boolean | Show subagent prompts |
| `chat.subagent.showRoles` | boolean | Show subagent roles |
| `chat.subagent.showDeps` | boolean | Show subagent dependencies |
| `chat.subagent.showResponses` | boolean | Show subagent responses |

## Examples

### Example 1: Start in Lite Mode

Set lite as your default (persists across sessions):

```bash
kiro-cli settings chat.ui.mode lite
kiro-cli chat
```

### Example 2: Switch Mid-Session

```
/lite
```

Output:
```
System: Switched to lite mode
```

### Example 3: Set Minimal Density

```
/verbosity density minimal
```

Tool calls now render as a single line with just the tool name and status.

### Example 4: Enable Shell Output Only

```
/verbosity off
/verbosity +shell
```

Only shell tool calls (`execute_bash`, etc.) show their output.

## Troubleshooting

### "Lite mode is not available in this build"

Lite mode requires the feature to be enabled in your build. It is currently gated to internal nightly builds.

### Tool output not showing

Check your verbosity filters with `/verbosity status`. If filters are empty (`[]`), no output bars render. Use `/verbosity on` to enable all output.

### Switching back to TUI

Type `/tui` to return to the full panel-based TUI interface.

## Related

- [Classic vs TUI](classic-vs-tui.md) — UI mode comparison
- [/settings](../slash-commands/settings.md) — Settings management
- [/theme](../slash-commands/theme.md) — Theme customization
