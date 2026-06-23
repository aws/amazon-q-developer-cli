---
doc_meta:
  title: /verbosity
  description: Configure lite-mode tool output rendering - density presets, filters, and display knobs
  category: slash_command
  keywords:
    [
      verbosity,
      verbose,
      density,
      filter,
      tools,
      output,
      minimal,
      lean,
      full,
      lite,
      rendering,
    ]
  related: [lite, settings, lite-mode]
  validated: 2026-06-23
  commit: bfba631a4
  status: validated
  testable_headless: false
---

# /verbosity

Configure lite-mode rendering.

## Overview

Controls how tool calls, subagent activity, and output bars render in lite mode (alias: `/verbose`). Only available in lite mode — in TUI mode it shows "/verbosity is only available in lite mode."

Settings persist in `~/.kiro/settings/cli.json` under the `chat.tools.*` keys, take effect immediately, and survive across sessions.

## Usage

```
/verbosity [subcommand]
```

Without arguments, opens the interactive configuration menu. With arguments, applies the change directly.

## Subcommands

| Subcommand         | Description                             |
| ------------------ | --------------------------------------- |
| (none)             | Open interactive menu                   |
| `on`               | Show output for all tools               |
| `off`              | Hide all tool output                    |
| `all`              | Reset filters to show all output        |
| `status`           | Show current configuration              |
| `reset`            | Reset to default configuration          |
| `<preset>`         | Apply density preset directly           |
| `density <preset>` | Apply density preset (alternative form) |
| `+<category>`      | Add a category to output filters        |
| `-<category>`      | Remove a category from output filters   |

### Density Presets

| Preset    | Tool Args       | Reasoning | Elapsed | Output Filters | Thinking |
| --------- | --------------- | --------- | ------- | -------------- | -------- |
| `minimal` | hidden          | hidden    | hidden  | none           | hidden   |
| `lean`    | inline          | hidden    | shown   | none           | hidden   |
| `default` | block           | shown     | shown   | shell only     | shown    |
| `full`    | block (no caps) | shown     | shown   | all tools      | shown    |

### Filter Categories

| Category     | Tools covered                            |
| ------------ | ---------------------------------------- |
| `shell`      | execute_bash and similar                 |
| `read`       | fs_read and similar                      |
| `web`        | web_search, web_fetch                    |
| `grep`       | grep tools                               |
| `glob`       | glob tools                               |
| `code`       | code intelligence tools                  |
| `introspect` | introspection tools                      |
| `task`       | task tools                               |
| `subagent`   | session_management, subagent, agent_crew |
| `mcp`        | All MCP tools (prefix `mcp__`)           |

You can also use exact tool names as filter tokens.

## Examples

### Apply a density preset

```
/verbosity minimal
```

Hides all tool args, reasoning, elapsed time, and output. Only tool names appear.

### Show current configuration

```
/verbosity status
```

Prints the active preset, filter list, and truncation settings.

### Add a filter category

```
/verbosity +shell
```

Adds shell tool output to the visible filter list. Only categories in the filter list show output bars.

### Remove a filter category

```
/verbosity -web
```

Removes web tool output from the visible filter list.

### Reset to defaults

```
/verbosity reset
```

Restores the `default` density preset (block args, reasoning shown, shell output only).

### Open the interactive menu

```
/verbosity
```

Opens a drill-down menu where you can adjust density, individual knobs (tool args mode, reasoning, elapsed, thinking), filters, and truncation limits with live preview.

## Troubleshooting

### "/verbosity is only available in lite mode"

You're in TUI mode. Switch to lite mode first with `/lite`, then use `/verbosity`.

### Tool output not showing

Check `/verbosity status`. If the output filters list is empty, no tool output bars render. Use `/verbosity on` to enable all output, or add specific categories (e.g., `/verbosity +shell`).

### Changes not persisting

Settings write to `~/.kiro/settings/cli.json`. Verify the file is writable and not locked by another process.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
