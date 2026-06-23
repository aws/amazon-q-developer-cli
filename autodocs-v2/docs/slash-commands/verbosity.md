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
  commit: f6731f45a
  status: validated
  testable_headless: false
---

# /verbosity

Configure lite-mode rendering.

Controls how tool calls, subagent activity, and output bars render in lite mode (alias: `/verbose`). Only available in lite mode — in TUI mode it shows "/verbosity is only available in lite mode."

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

With no arguments the menu exposes the same knobs as the tables above with live preview (Esc backs out). Settings persist in `~/.kiro/settings/cli.json` under the `chat.tools.*` keys, take effect immediately, and survive across sessions.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
