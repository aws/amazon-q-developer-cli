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
| `chat.enableThinking` | boolean | Enable thinking tool for complex reasoning |
| `chat.enableKnowledge` | boolean | Enable knowledge base functionality |
| `chat.enableCodeIntelligence` | boolean | Enable code intelligence with LSP |
| `chat.enableTangentMode` | boolean | Enable tangent mode feature |
| `chat.enableSubagent` | boolean | Enable subagent feature |
| `chat.enableTodoList` | boolean | Enable todo list feature |
| `chat.enableCheckpoint` | boolean | Enable checkpoint feature |
| `chat.enableDelegate` | boolean | Enable delegate tool |
| `chat.enableContextUsageIndicator` | boolean | Show context usage in prompt |
| `chat.enableNotifications` | boolean | Enable desktop notifications |
| `chat.enableHistoryHints` | boolean | Show conversation history hints |
| `chat.editMode` | boolean | Enable edit mode for chat |
| `chat.greeting.enabled` | boolean | Show greeting message on start |
| `chat.disableMarkdownRendering` | boolean | Disable markdown formatting |
| `chat.disableAutoCompaction` | boolean | Disable automatic summarization |
| `chat.skimCommandKey` | string | Key binding for fuzzy search |
| `chat.autocompletionKey` | string | Key binding for autocompletion |
| `chat.tangentModeKey` | string | Key binding for tangent mode |
| `chat.delegateModeKey` | string | Key binding for delegate |
| `chat.diffTool` | string | External diff tool command |
| `chat.uiMode` | string | UI variant to use |
| `telemetry.enabled` | boolean | Enable/disable telemetry |
| `knowledge.defaultIncludePatterns` | array | File patterns to include |
| `knowledge.defaultExcludePatterns` | array | File patterns to exclude |
| `knowledge.maxFiles` | number | Maximum files for indexing |
| `knowledge.chunkSize` | number | Text chunk size |
| `knowledge.chunkOverlap` | number | Overlap between chunks |
| `knowledge.indexType` | string | Type of knowledge index |
| `api.timeout` | number | API request timeout (seconds) |
| `api.codewhisperer.service` | string | CodeWhisperer endpoint |
| `api.q.service` | string | Q service endpoint |
| `mcp.initTimeout` | number | MCP server init timeout |
| `mcp.noInteractiveTimeout` | number | Non-interactive MCP timeout |
| `mcp.loadedBefore` | boolean | Track loaded MCP servers |
| `compaction.excludeContextWindowPercent` | number | Context % to exclude from compaction |
| `compaction.excludeMessages` | number | Messages to exclude from compaction |
| `introspect.tangentMode` | boolean | Auto-tangent for introspect |

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
