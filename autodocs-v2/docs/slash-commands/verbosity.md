---
doc_meta:
  title: /verbosity
  description: Configure lite-mode output rendering — density presets, tool filters, and display toggles
  category: slash_command
  keywords: [verbosity, verbose, density, filters, output, tool, args, reasoning, elapsed, subagent, minimal, lean, full, lite]
  related: [lite, settings, classic-vs-tui]
  validated: 2026-06-07
  commit: eaabfbb9e
  status: validated
  testable_headless: false
---

## Overview

The `/verbosity` command opens a configuration menu for lite mode's scrollback rendering. It controls how much detail appears for tool calls, reasoning, subagent pipelines, and output bars.

This command is lite-mode only. In TUI mode it is not surfaced in autocomplete; attempting to run it displays an error alert.

Also reachable via `/settings → verbosity` or the alias `/verbose`.

## Usage

```
/verbosity
```

Opens the top-level verbosity menu with these sections:

- **Density** — Apply a preset (minimal, lean, default, full) or customize individual settings
- **Show output** — Toggle which tool categories display output bars
- **Tool args** — Control how tool arguments render (off/inline/block)
- **Reasoning** — Show or hide per-tool-call reasoning text
- **Elapsed time** — Show or hide execution duration
- **Thinking** — Show or hide model thinking content
- **Write diffs** — Show or hide diff bodies for file writes
- **Tasks** — Show or hide the task tray
- **Subagent** — Toggle individual subagent sections (pipeline, prompts, roles, deps, responses)
- **Truncation** — Set max lines/chars for tool args and output bars

Navigate with arrow keys, Enter to select, Esc to go back one level.

## Density Presets

Presets apply a coordinated set of all display toggles and output filters at once:

| Preset | Tool Args | Reasoning | Elapsed | Thinking | Write Diffs | Tasks | Output Filters | Output Max Lines |
|--------|-----------|-----------|---------|----------|-------------|-------|---------------|-----------------|
| **minimal** | off | off | off | off | off | off | none | 5 |
| **lean** | inline | off | on | off | off | on | none | 10 |
| **default** | block | on | on | on | on | on | shell only | 5 |
| **full** | block | on | on | on | on | on | all | unlimited |

Selecting a preset resets both the display settings and the output filter list to match. Custom filter lists are only preserved when using the individual toggle menus (not when picking a preset).

## Output Filters

The "Show output" submenu controls which tool categories display an output bar below the tool-call line. Available categories:

| Category | Tools Covered |
|----------|--------------|
| `shell` | execute_bash, shell |
| `read` | fs_read, read |
| `web` | web_search, web_fetch |
| `grep` | grep, ripgrep |
| `glob` | glob, find_files |
| `code` | code intelligence tools |
| `introspect` | introspect |
| `task` | todo_list, task |
| `subagent` | subagent pipeline tools |
| `mcp` | Any tool prefixed with `mcp__` |
| `all` | Toggle all categories on/off |

An empty filter list means no output bars render for any tool. The `all` row acts as a master toggle.

## Examples

### Apply a density preset

```
/verbosity
```

Select "Density" → pick "lean" for a compact view with inline tool args and no reasoning.

### Show output for shell commands only

```
/verbosity
```

Select "Show output" → enable only "shell". Other tool categories won't show output bars.

### Enable all output

```
/verbosity
```

Select "Show output" → select "all" to enable output bars for every tool.

### Hide subagent pipeline details

```
/verbosity
```

Select "Subagent" → toggle off "prompts", "roles", and "deps" to show only the pipeline tree and final responses.

### Set truncation limits

```
/verbosity
```

Select "Truncation" → set output max lines to 10 to cap tool output bars at 10 visible rows (older lines show a "+N more lines above" marker).

## Configuration Storage

Verbosity settings are saved to two locations:

- `~/.kiro/settings/lite_verbose.json` — legacy mirror, full verbose config
- `~/.kiro/settings/cli.json` — canonical settings (shared with TUI mode)

Both are written on every change so settings stay in sync. The `cli.json` keys used:

| Setting Key | Controls |
|-------------|----------|
| `chat.tools.filters` | Output filter list |
| `chat.tools.showReasoning` | Per-tool reasoning visibility |
| `chat.tools.argsMode` | Tool args mode (off/inline/block) |
| `chat.tools.showElapsed` | Elapsed time display |
| `chat.tools.argsMaxLines` | Args block truncation (lines) |
| `chat.tools.outputMaxLines` | Output bar truncation (lines) |
| `chat.tools.argsMaxChars` | Args value truncation (chars) |
| `chat.tools.outputMaxChars` | Output line truncation (chars) |
| `chat.tools.showWriteDiffs` | Write tool diff bodies |
| `chat.showTasks` | Task tray visibility |
| `chat.showThinking` | Model thinking content |
| `chat.subagent.showPipeline` | Subagent pipeline tree |
| `chat.subagent.showPrompts` | Subagent prompt sections |
| `chat.subagent.showRoles` | Subagent role annotations |
| `chat.subagent.showDeps` | Subagent dependency arrows |
| `chat.subagent.showResponses` | Subagent per-stage responses |

## Troubleshooting

### /verbosity shows "lite only" error

**Cause**: You're in TUI mode, not lite mode.  
**Solution**: Switch to lite mode with `/lite` first, then use `/verbosity`.

### Changes don't seem to take effect

**Cause**: Verbosity settings apply to new messages only. Already-rendered scrollback is immutable (append-only rendering).  
**Solution**: Changes apply to the next tool call or message. Use `/clear` to start fresh if needed.

### Output bar not showing for a tool

**Cause**: The tool's category isn't in the active filter list.  
**Solution**: `/verbosity` → Show output → enable the relevant category. Check if you're on a density preset that clears filters (minimal, lean, default only shows shell).

### Saved config not loading

**Cause**: File permissions or corruption in `~/.kiro/settings/lite_verbose.json`.  
**Solution**: Delete the file to reset to defaults. The next `/verbosity` change recreates it.

## Related

- [/lite](lite.md) — Switch to lite mode
- [/settings](settings.md) — Top-level settings menu (verbosity is a submenu in lite mode)
- [Classic vs TUI](../features/classic-vs-tui.md) — Mode comparison and switching
