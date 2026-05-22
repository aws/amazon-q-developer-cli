---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli settings
  description: Configure Kiro CLI behavior with get, set, list, open, and delete operations
  keywords: [settings, config, configure, preferences]
  related: [agent-configuration]
---

# kiro-cli settings

Configure Kiro CLI behavior with get, set, list, open, and delete operations.

## Overview

The settings command manages Kiro CLI configuration. List all settings, get current values, set new values, delete settings, or open the settings file. Settings persist across sessions and control features like default agent/model and knowledge base configuration.

## Usage

```bash
kiro-cli settings [KEY] [VALUE]
kiro-cli settings <COMMAND>
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--delete` | `-d` | Delete a key (no value needed) |
| `--format <FORMAT>` | `-f` | Output format: plain, json, json-pretty (default: plain) |
| `--help` | `-h` | Print help information |

## Commands

### list

List settings with values and descriptions.

```bash
kiro-cli settings list [OPTIONS]
```

**Options**:
- `--all`: Show all available settings with descriptions
- `--format <FORMAT>`: Output format (plain, json, json-pretty)

Without `--all`, shows only configured settings. With `--all`, shows all available settings including unset ones.

### open

Open settings file in default editor.

```bash
kiro-cli settings open
```

### get

Get current value of a setting.

```bash
kiro-cli settings <KEY>
```

### set

Set a value for a setting.

```bash
kiro-cli settings <KEY> <VALUE>
```

**Value types**: Boolean (`true`/`false`), String, Number, Array (JSON format).

### delete

Delete a setting. Supports glob patterns to remove multiple settings.

```bash
kiro-cli settings --delete <KEY>
kiro-cli settings --delete "chat.*"
```

## Settings Scopes

Settings are resolved in priority order: Session > Workspace > Global.

- **Global**: `~/.kiro/settings/cli.json` — applies to all workspaces
- **Workspace**: `.kiro/settings/cli.json` — overrides global for current directory

The CLI writes to the global file by default. To set workspace-specific overrides, edit `.kiro/settings/cli.json` directly or use `kiro-cli settings open`.

Some settings are global-only and cannot be overridden at workspace level (e.g., `telemetry.enabled`, `api.codewhisperer.service`).

## Key Settings

| Setting | Type | Description |
|---------|------|-------------|
| `chat.defaultAgent` | string | Default agent for new sessions |
| `chat.defaultModel` | string | Default AI model |
| `chat.enableThinking` | boolean | Enable thinking tool |
| `chat.showThinking` | boolean | Show thinking/reasoning blocks in chat output (default: false; startup-only) |
| `chat.enableKnowledge` | boolean | Enable knowledge base |
| `chat.enableCodeIntelligence` | boolean | Enable code intelligence with LSP |
| `chat.enableSubagent` | boolean | Enable subagent feature |
| `chat.enableTodoList` | boolean | Enable todo list feature |
| `chat.enableNotifications` | boolean | Enable desktop notifications |
| `chat.greeting.enabled` | boolean | Show greeting message on start |
| `chat.allowAnimations` | boolean | Enable animated spinners and progress indicators (default: true) |
| `chat.allowAsciiArt` | boolean | Enable Unicode/braille symbols and decorative art (default: true) |
| `chat.allowIcons` | boolean | Show status indicator icons (default: true) |
| `chat.disableAutoCompaction` | boolean | Disable automatic summarization |
| `chat.disableGranularTrust` | boolean | Disable granular trust options |
| `chat.autoExpandToolOutput` | boolean | Always show full tool output |
| `chat.disableWrap` | boolean | Disable word-wrapping in chat output; long lines soft-wrap visually but stay as a single logical line for clean copy-paste |
| `chat.keybindings.cancelStream` | string | Key to cancel streaming (default: `esc`) |
| `chat.keybindings.closeMenu` | string | Key to close slash menus and panels (default: `esc`) |
| `chat.keybindings.quit` | string | Key to quit (double-press required, default: `ctrl+c`) |
| `telemetry.enabled` | boolean | Enable/disable telemetry |
| `knowledge.defaultIncludePatterns` | array | File patterns to include |
| `knowledge.defaultExcludePatterns` | array | File patterns to exclude |
| `knowledge.maxFiles` | number | Maximum files for indexing |
| `knowledge.chunkSize` | number | Text chunk size |
| `knowledge.chunkOverlap` | number | Overlap between chunks |
| `knowledge.indexType` | string | Index type: fast (BM25) or best (semantic) |
| `api.timeout` | number | API request timeout in milliseconds |
| `api.codewhisperer.service` | string | CodeWhisperer endpoint (global only) |
| `api.q.service` | string | Q service endpoint (global only) |
| `mcp.initTimeout` | number | MCP server init timeout |
| `mcp.noInteractiveTimeout` | number | Non-interactive MCP timeout |
| `compaction.excludeContextWindowPercent` | number | Context % to exclude from compaction |
| `compaction.excludeMessages` | number | Messages to exclude from compaction |

## Examples

### Example 1: Set Default Model

```bash
kiro-cli settings chat.defaultModel "anthropic.claude-3-5-sonnet-20241022-v2:0"
```

### Example 2: Set Default Agent

```bash
kiro-cli settings chat.defaultAgent rust-expert
```

### Example 3: Check a Setting

```bash
kiro-cli settings chat.defaultModel
```

### Example 4: List All Available Settings

```bash
kiro-cli settings list --all
```

### Example 5: Delete a Setting

```bash
kiro-cli settings --delete chat.defaultModel
```

### Example 6: Delete Multiple Settings with Glob

```bash
kiro-cli settings --delete "knowledge.*"
```

### Example 7: Toggle Telemetry

```bash
kiro-cli settings telemetry.enabled false
```

### Example 8: Disable ASCII Art (Accessibility)

```bash
kiro-cli settings chat.allowAsciiArt false
```

### Example 9: Disable Animations

```bash
kiro-cli settings chat.allowAnimations false
```

### Example 10: Rebind V2 TUI Shortcuts

```bash
# Cancel streaming with Ctrl+G instead of Esc
kiro-cli settings chat.keybindings.cancelStream "ctrl+g"

# Close menus with Ctrl+S
kiro-cli settings chat.keybindings.closeMenu "ctrl+s"

# Quit with Ctrl+Q (still requires a double-press)
kiro-cli settings chat.keybindings.quit "ctrl+q"
```

**Syntax**: `+`-separated, case-insensitive. Modifiers: `ctrl`, `shift`, `alt` (aliases: `meta`, `cmd`). Named keys: `esc`, `enter`, `tab`, `up`/`down`/`left`/`right`, `space`, `backspace`, `delete`, `home`, `end`, `pageup`, `pagedown`. Any single ASCII character is also accepted. Invalid values fall back to the built-in default. Applies to V2 TUI only. These settings are global-only and cannot be overridden per workspace.

## Troubleshooting

### Issue: Setting Not Found

**Symptom**: "Unknown setting" error  
**Solution**: Use `kiro-cli settings list --all` to see valid settings.

### Issue: Invalid Value

**Symptom**: "Invalid value" error  
**Solution**: Check setting type. Use `true`/`false` for booleans, JSON for arrays.

### Issue: Changes Not Applied

**Symptom**: Setting changed but behavior unchanged  
**Solution**: Some settings require restarting the chat session.

## Related Features

- [Agent Configuration](../features/agent-configuration.md) - Agent-specific settings

## Technical Details

**Storage**: Global: `~/.kiro/settings/cli.json`, Workspace: `.kiro/settings/cli.json`

**Scope Priority**: Session > Workspace > Global. Workspace overrides global for non-global-only settings.

**Types**: Boolean, String, Number, Array (JSON).

**Naming**: Settings use dot notation (e.g., `chat.defaultModel`).

**Aliases**: `setting` (singular) also works.
