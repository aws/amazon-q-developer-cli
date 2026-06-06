---
doc_meta:
  title: /verbosity
  description: Configure lite-mode rendering density, tool output filters, args display, and truncation
  category: slash_command
  keywords: [verbosity, verbose, density, filters, tools, output, minimal, lean, full, truncation, lite]
  related: [settings, lite-mode]
  validated: 2026-06-06
  commit: fcddc2183
  status: validated
  testable_headless: false
---

## Overview

The `/verbosity` command configures how lite mode renders tool calls, agent output, and subagent activity in your scrollback. It controls which tools show output, how arguments display, truncation limits, and overall information density.

This command is only available in lite mode. In TUI mode, the entry appears in `/settings` but routes to a "lite mode only" notice.

An alias `/verbose` is also accepted.

## Usage

```
/verbosity
```

Opens the top-level density menu with preset options and a custom configuration path.

You can also access it via:

```
/settings verbosity
```

## Density Presets

Presets are the fastest way to configure verbosity. Each preset sets all display options at once:

| Preset | Tool Reasoning | Args Mode | Elapsed | Thinking | Write Diffs | Tasks | Output Filters |
|--------|---------------|-----------|---------|----------|-------------|-------|----------------|
| `minimal` | off | off | off | off | off | off | none |
| `lean` | off | inline | on | off | off | on | none |
| `default` | on | block | on | on | on | on | shell only |
| `full` | on | block | on | on | on | on | all tools |

Selecting a preset replaces both the display configuration and the filter list.

## Display Options

These control how tool calls render in scrollback:

### Tool reasoning (`chat.tools.showReasoning`)

Show the model's per-tool "why" explanation (the purple reasoning line above each tool call). Default: on.

### Args mode (`chat.tools.argsMode`)

How tool arguments display:

- `off` — Only the tool name, no arguments shown
- `inline` — Single-line chip (shell-style: `tool [arg]`)
- `block` — Full key:value tree under the tool name (default)

### Elapsed time (`chat.tools.showElapsed`)

Show duration next to completed tool calls. Default: on.

### Thinking content (`chat.showThinking`)

Show the model's freeform thinking blocks. Shared with the TUI's "Show thinking" setting — both modes read and write the same `chat.showThinking` key. Default: on.

### Write diffs (`chat.tools.showWriteDiffs`)

Show diff bodies for file-write tool calls. When off, only the tool header (name, status, elapsed) renders — diffs are suppressed. Default: on.

### Tasks (`chat.showTasks`)

Show the task tray above the input. The tray surfaces `todo_list`/`task` tool state. Default: on.

## Output Filters

Filters control which tools show their output content below the tool-call header. The filter list is the single source of truth — `filters: []` means no output renders for any tool; `filters: ['all']` shows output for every tool.

### Filter tokens

| Token | Tools included |
|-------|---------------|
| `all` | Every tool (overrides other tokens) |
| `shell` | `execute_bash`, `bash` |
| `read` | `fs_read` |
| `web` | `web_search`, `web_fetch` |
| `grep` | `grep` |
| `glob` | `glob` |
| `code` | `code` |
| `introspect` | `introspect` |
| `task` | `task`, `todo_list` |
| `subagent` | Subagent tools |
| `mcp` | Any tool starting with `mcp__` |

You can also use exact tool names as filter tokens.

Default filters: `['shell']` (only shell tool output is shown).

## Subagent Display

Controls what renders in the subagent summary block:

| Setting | Key | Default | Description |
|---------|-----|---------|-------------|
| Pipeline | `chat.subagent.showPipeline` | on | Show the pipeline/stage tree |
| Prompts | `chat.subagent.showPrompts` | on | Show stage prompts |
| Roles | `chat.subagent.showRoles` | on | Show agent roles |
| Dependencies | `chat.subagent.showDeps` | on | Show dependency arrows |
| Responses | `chat.subagent.showResponses` | on | Show per-stage responses |

## Truncation

Caps how many lines/characters render for tool arguments and output:

| Setting | Key | Default |
|---------|-----|---------|
| Args max lines | `chat.tools.argsMaxLines` | unlimited |
| Output max lines | `chat.tools.outputMaxLines` | 5 |
| Args max chars | `chat.tools.argsMaxChars` | unlimited |
| Output max chars | `chat.tools.outputMaxChars` | unlimited |

When a limit is hit, content is truncated with a marker. Set to `null` or `0` for unlimited.

## Persistence

Configuration is persisted in two locations:

- `~/.kiro/settings/lite_verbose.json` — Legacy file, still read as fallback
- `~/.kiro/settings/cli.json` — Canonical source; individual keys like `chat.tools.filters`, `chat.tools.argsMode`, etc.

The `/verbosity` menu writes to both locations. Resolution order: `cli.json` > `lite_verbose.json` > built-in defaults.

## Examples

### Open the verbosity menu

```
/verbosity
```

### Access via settings

```
/settings verbosity
```

### Quick density switch

Select "Density" from the `/verbosity` menu, then choose `minimal`, `lean`, `default`, or `full`.

### Enable all tool output

Select "Filters" from the menu and add `all` to the filter list. Or select the `full` density preset.

### Show only shell output (default)

Filters: `['shell']` — only `execute_bash` and `bash` tool output renders.

## Troubleshooting

### `/verbosity` shows "lite mode only" error

This command only works in lite mode. Switch to lite mode with `/lite` first.

### Changes don't seem to take effect

Verbosity settings apply to newly rendered tool calls. Already-rendered content in scrollback is not re-rendered. Send a new message to see the updated rendering.

### Setting was changed in TUI but lite doesn't reflect it

The `chat.showThinking` setting is shared between both modes. Other verbosity settings are lite-specific. If you edit `cli.json` manually, changes take effect on the next tool call render.

## Related

- [/settings](settings.md) — Parent settings menu (includes verbosity as a subcommand)
- [Lite Mode](../features/lite-mode.md) — The lite UI mode where verbosity applies
