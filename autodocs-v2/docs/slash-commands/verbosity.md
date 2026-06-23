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
  validated: 2026-06-05
  commit: 8a53bd9d9
  status: validated
  testable_headless: false
---

# /verbosity

Configure lite-mode rendering.

## Overview

The `/verbosity` command controls how tool calls, subagent activity, and output bars render in lite mode. It provides both an interactive menu and direct CLI-style subcommands for power users.

This command is only available in lite mode. Running it in TUI mode shows: "/verbosity is only available in lite mode."

`/verbose` is accepted as an alias.

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

```
/verbosity minimal     # density preset → "verbosity: density set to minimal"
/verbosity on          # show output for all tools
/verbosity off         # hide all tool output
/verbosity +shell      # add a category to the output filters
/verbosity -shell      # remove a category from the output filters
/verbosity status      # "verbosity · filters: shell · density: default"
```

Running `/verbosity` with no arguments opens the interactive menu (below). MCP-prefixed tools use the `mcp` category (`/verbosity +mcp`).

## Interactive Menu

The menu has these sections:

- **Density** — Quick preset selection (minimal, lean, default, full)
- **Tool calls** — Toggle reasoning, args mode (off/inline/block), elapsed time
- **Subagent** — Toggle pipeline tree, prompts, roles, dependencies, responses
- **Output bar** — Per-category filter toggles
- **Truncation** — Set character and line caps for args and output
- **Reset** — Return to default configuration

Press Esc to back out of submenus. The menu supports live preview showing how the current settings would render.

## Persistence

Settings are persisted in `~/.kiro/settings/cli.json` under keys like:

- `chat.tools.filters` — filter token list
- `chat.tools.showReasoning` — boolean
- `chat.tools.argsMode` — `"off"` | `"inline"` | `"block"`
- `chat.tools.showElapsed` — boolean
- `chat.tools.argsMaxLines` — number or null
- `chat.tools.outputMaxLines` — number or null
- `chat.tools.argsMaxChars` — number or null
- `chat.tools.outputMaxChars` — number or null

Changes take effect immediately and persist across sessions.

## Troubleshooting

- **"/verbosity is only available in lite mode"** — switch with `/lite` first.
- **Filters set but no output** — confirm the category matches (MCP tools need `mcp`); verify with `/verbosity status`.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/settings](settings.md) — Settings management
- [Lite Mode](../features/lite-mode.md) — Full feature documentation
