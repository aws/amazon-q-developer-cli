---
doc_meta:
  title: /verbosity
  description: Configure tool output verbosity, density presets, and display options in lite mode
  category: slash_command
  keywords: [verbose, verbosity, tools, output, density, filters, minimal, lean, full, reasoning, args, truncation, lite]
  related: [lite, settings, tools]
  validated: 2026-06-06
  commit: 89110a61d
  status: validated
  testable_headless: false
---

## Overview

The `/verbosity` command (alias `/verbose`) opens a menu for configuring how tool calls render in lite mode's scrollback. Controls include density presets, per-tool output filters, argument display mode, truncation limits, and subagent detail toggles.

Only available in lite mode. In TUI mode it shows an error: "/verbosity is only available in lite mode".

Also reachable via `/settings verbosity`.

## Usage

```
/verbosity
```

Opens the density preset menu. Select a preset or drill into individual settings.

```
/verbosity <subcommand>
```

Jump directly to a configuration area.

## Density Presets

Presets apply a coherent set of display defaults. Selecting a preset replaces both the display configuration and the filter list.

| Preset | Tool Args | Reasoning | Elapsed | Thinking | Write Diffs | Tasks | Output Filters |
|--------|-----------|-----------|---------|----------|-------------|-------|----------------|
| `minimal` | off | off | off | off | off | off | none |
| `lean` | inline | off | on | off | off | on | none |
| `default` | block | on | on | on | on | on | shell |
| `full` | block | on | on | on | on | on | all |

### Applying a preset

```
/verbosity density minimal
/verbosity density default
```

Or select from the menu:

```
/verbosity
```

## Filter Tokens

Filters control which tools show an output bar in scrollback. A tool's output renders if its name or its category appears in the filter list.

### Categories

| Category | Tools |
|----------|-------|
| `shell` | execute_bash, shell, etc. |
| `read` | fs_read, read_file, etc. |
| `web` | web_search, web_fetch |
| `grep` | grep, ripgrep |
| `glob` | glob, find_files |
| `code` | code intelligence tools |
| `introspect` | introspection tools |
| `task` | task/todo list tools |
| `subagent` | sub-agent pipeline tools |
| `mcp` | any tool prefixed with `mcp__` |

### Special tokens

- `all` — show output for every tool
- Individual tool names (e.g., `execute_bash`, `mcp__my-server__tool`)

### Managing filters

```
/verbosity on              # set filters to ['all']
/verbosity off             # clear all filters
/verbosity add shell       # add 'shell' category
/verbosity remove shell    # remove 'shell' category
/verbosity add grep,web    # add multiple tokens
```

## Display Options

Individual toggles available via the menu or direct commands:

### Tool arguments mode

Controls how tool arguments render below the tool name line.

| Mode | Description |
|------|-------------|
| `off` | No args shown — just tool name + reasoning |
| `inline` | Single-line chip: `tool [command...]` |
| `block` | Full key:value tree under the tool name |

```
/verbosity set:argsMode off
/verbosity set:argsMode inline
/verbosity set:argsMode block
```

### Boolean toggles

| Toggle | Setting key | Default | Effect |
|--------|-------------|---------|--------|
| Show reasoning | `chat.tools.showReasoning` | on | Per-tool "why" text |
| Show elapsed | `chat.tools.showElapsed` | on | Duration after tool completes |
| Show thinking | `chat.showThinking` | on | Model's freeform thinking content |
| Show write diffs | `chat.tools.showWriteDiffs` | on | Diff body for file writes |
| Show tasks | `chat.showTasks` | on | Task tray above input |

### Truncation limits

Cap the visual footprint of tool args and output bars:

| Setting | Key | Default | Description |
|---------|-----|---------|-------------|
| Args max lines | `chat.tools.argsMaxLines` | unlimited | Max rows in block-args |
| Args max chars | `chat.tools.argsMaxChars` | unlimited | Max chars per arg value |
| Output max lines | `chat.tools.outputMaxLines` | 5 | Max rows in output bar |
| Output max chars | `chat.tools.outputMaxChars` | unlimited | Max chars per output line |

```
/verbosity set:outputLines 10
/verbosity set:argsChars 80
```

Set to `0` or `null` for unlimited.

### Subagent sections

Control which parts of the subagent final-result block render:

| Section | Default | What it shows |
|---------|---------|---------------|
| pipeline | on | Stage tree visualization |
| prompts | on | Per-stage prompt text |
| roles | on | Agent role labels |
| deps | on | Dependency arrows |
| responses | on | Per-stage responses |

## Configuration Storage

Verbosity settings are persisted to `~/.kiro/settings/cli.json` under the `chat.tools.*` and `chat.subagent.*` keys. The legacy file `~/.kiro/settings/lite_verbose.json` is also maintained for backward compatibility but `cli.json` takes precedence.

Changes apply immediately to new tool calls. Already-rendered scrollback is not retroactively updated (lite renders into the terminal's static buffer).

## Environment Variables

| Variable | Effect |
|----------|--------|
| `KIRO_LITE_VERBOSE=1` | Seeds filters to `['all']` when no saved config exists (first-run hint) |

## Examples

### Switch to minimal density

```
/verbosity density minimal
```

### Show all tool output

```
/verbosity on
```

### Show only shell output

```
/verbosity off
/verbosity add shell
```

### Increase output window

```
/verbosity set:outputLines 20
```

### Reset to defaults

```
/verbosity density default
```

## Troubleshooting

### Issue: "/verbosity is only available in lite mode"

**Symptom**: Error when typing `/verbosity` or `/verbose`
**Cause**: You're in TUI mode, not lite mode.
**Solution**: Switch to lite mode first with `/lite`, then use `/verbosity`.

### Issue: Changes don't affect existing output

**Symptom**: Scrollback above current position unchanged after adjusting settings
**Cause**: Lite mode renders tool output once into the terminal's static buffer.
**Solution**: This is expected. New tool calls use the updated settings. Use `/clear` to re-render the full conversation with current settings.

### Issue: Custom filter not working for MCP tool

**Symptom**: MCP tool output not showing despite adding it to filters
**Cause**: MCP tools use the `mcp__<server>__<tool>` naming convention.
**Solution**: Add the `mcp` category to catch all MCP tools, or add the exact tool name (e.g., `mcp__my-server__my-tool`).

## Related

- [/lite](lite.md) — Switch to lite mode where verbosity applies
- [/settings](settings.md) — Parent settings menu (includes verbosity entry)
- [/tools](tools.md) — View available tools
