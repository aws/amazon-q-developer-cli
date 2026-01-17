---
doc_meta:
  validated: 2026-01-17
  commit: TBD
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli settings
  description: Configure Kiro CLI behavior at global and workspace levels with get, set, list, open, and delete operations
  keywords: [settings, config, configure, preferences, workspace, global]
  related: [slash-experiment, agent-configuration]
---

# kiro-cli settings

Configure Kiro CLI behavior with get, set, list, open, and delete operations for all settings.

## Overview

The settings command manages Kiro CLI configuration at both global and workspace levels. List all settings, get current values, set new values, delete settings, or open the settings file. Settings persist across sessions and control features like tangent mode, default agent/model, and experimental features.

**Workspace Settings**: Most settings can be overridden at the workspace level, allowing per-project customization while maintaining global defaults.

## Usage

### Basic Usage

```bash
kiro-cli settings [KEY] [VALUE]
kiro-cli settings <COMMAND>
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--delete` | `-d` | Delete a key (no value needed) |
| `--global` | | Set or delete in global settings (default) |
| `--workspace` | | Set or delete in workspace settings |
| `--format <FORMAT>` | `-f` | Output format: plain (markdown), json, json-pretty (default: plain) |
| `--verbose` | `-v` | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | Print help information |

## Workspace Settings

Settings can be configured at two levels:
- **Global**: `~/.kiro/settings/cli.json` - applies to all workspaces
- **Workspace**: `.kiro/settings/cli.json` - overrides global for current workspace

### Workspace-Overridable Settings

Most settings can be overridden at workspace level, including:
- Feature flags (e.g., `chat.enableTangentMode`, `chat.enableKnowledge`)
- Model preferences (e.g., `chat.defaultModel`)
- Agent preferences (e.g., `chat.defaultAgent`)
- UI settings (e.g., `chat.editMode`, `chat.disableMarkdownRendering`)
- Knowledge configuration (e.g., `knowledge.maxFiles`, `knowledge.chunkSize`)
- Key bindings (e.g., `chat.tangentModeKey`, `chat.autocompletionKey`)

### Global-Only Settings

Some settings cannot be overridden at workspace level:
- `telemetry.enabled` - Telemetry collection
- `telemetryClientId` - Legacy client ID
- `codeWhisperer.shareCodeWhispererContentWithAWS` - Content sharing
- `api.codewhisperer.service` - CodeWhisperer endpoint
- `api.q.service` - Q service endpoint
- `mcp.loadedBefore` - MCP tracking flag

### Common Use Cases

#### Use Case 1: List All Settings with Sources

```bash
kiro-cli settings list
```

**What this does**: Shows all configured settings with their values and sources (global/workspace).

#### Use Case 2: Get Setting Value with Source

```bash
kiro-cli settings chat.enableTangentMode
```

**What this does**: Displays current value and whether it's from global or workspace settings.

#### Use Case 3: Set Workspace-Specific Model

```bash
kiro-cli settings --workspace chat.defaultModel "anthropic.claude-3-5-sonnet-20241022-v2:0"
```

**What this does**: Sets model for current workspace only, overriding global default.

#### Use Case 4: Set Global Default Agent

```bash
kiro-cli settings --global chat.defaultAgent rust-expert
```

**What this does**: Sets rust-expert as default agent globally (all workspaces).

#### Use Case 5: Enable Feature for Workspace

```bash
kiro-cli settings --workspace chat.enableTangentMode true
```

**What this does**: Enables tangent mode for current workspace only.

#### Use Case 6: Delete Workspace Override

```bash
kiro-cli settings --delete --workspace chat.defaultModel
```

**What this does**: Removes workspace override, falling back to global setting.

#### Use Case 7: Open Settings File

```bash
kiro-cli settings open
```

**What this does**: Opens global settings file in default editor.

## Commands

### list

List all settings with values and descriptions.

```bash
kiro-cli settings list [OPTIONS]
```

**Options**:
- `--all`: Show all available settings
- `--format <FORMAT>`: Output format (plain, json, json-pretty)

### open

Open settings file in default editor.

```bash
kiro-cli settings open
```

### get

Get current value of setting.

```bash
kiro-cli settings <SETTING_NAME>
```

### set

Set new value for setting.

```bash
kiro-cli settings [--global|--workspace] <SETTING_NAME> <VALUE>
```

**Options**:
- `--global`: Set in global settings (default)
- `--workspace`: Set in workspace settings

**Value types**:
- Boolean: `true` or `false`
- String: any text
- Number: integer or decimal
- Array: JSON array format

### delete

Delete setting (revert to default or global).

```bash
kiro-cli settings --delete [--global|--workspace] <SETTING_NAME>
```

**Options**:
- `--global`: Delete from global settings (default)
- `--workspace`: Delete from workspace settings (falls back to global)

## Key Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `chat.enableTangentMode` | boolean | false | Enable tangent mode feature |
| `chat.tangentModeKey` | string | t | Key binding for tangent mode toggle |
| `chat.defaultAgent` | string | none | Default agent for new sessions |
| `chat.defaultModel` | string | none | Default AI model |
| `chat.enableThinking` | boolean | false | Enable thinking tool |
| `chat.enableKnowledge` | boolean | false | Enable knowledge base |
| `chat.enableCodeIntelligence` | boolean | false | Enable code intelligence |
| `chat.enableTodoList` | boolean | false | Enable TODO list feature |
| `chat.enableCheckpoint` | boolean | false | Enable checkpoint feature |
| `chat.enableDelegate` | boolean | false | Enable delegate tool |
| `chat.introspectTangentMode` | boolean | false | Auto-tangent for introspect |
| `chat.greetingEnabled` | boolean | true | Show greeting on start |
| `chat.disableMarkdownRendering` | boolean | false | Disable markdown formatting |
| `chat.enableContextUsageIndicator` | boolean | false | Show context usage in prompt |

## Examples

### Example 1: Set Workspace-Specific Model

```bash
kiro-cli settings --workspace chat.defaultModel "anthropic.claude-3-5-sonnet-20241022-v2:0"
```

### Example 2: Set Global Default Agent

```bash
kiro-cli settings --global chat.defaultAgent rust-expert
```

### Example 3: Check Setting with Source

```bash
kiro-cli settings chat.defaultModel
```

**Expected Output**:
```
anthropic.claude-3-5-sonnet-20241022-v2:0 (workspace)
```

### Example 4: List Settings with Sources

```bash
kiro-cli settings list
```

**Expected Output**:
```
chat.defaultAgent = "rust-expert" (global)
chat.defaultModel = "anthropic.claude-3-5-sonnet-20241022-v2:0" (workspace)
chat.enableTangentMode = true (workspace)
...
```

### Example 5: Delete Workspace Override

```bash
kiro-cli settings --delete --workspace chat.defaultModel
```

**What this does**: Removes workspace override, falls back to global setting.

### Example 6: Enable Feature Globally

```bash
kiro-cli settings chat.enableTangentMode true
```

**Note**: Without `--workspace` flag, defaults to global.

## Troubleshooting

### Issue: Setting Not Found

**Symptom**: "Unknown setting" error  
**Cause**: Invalid setting name  
**Solution**: Use `kiro-cli settings list` to see all valid settings.

### Issue: Invalid Value

**Symptom**: "Invalid value" error  
**Cause**: Wrong type for setting  
**Solution**: Check setting type in list. Use `true`/`false` for booleans, not `yes`/`no`.

### Issue: Cannot Override at Workspace Level

**Symptom**: "Setting cannot be overridden at workspace level" error  
**Cause**: Attempting to set global-only setting with `--workspace` flag  
**Solution**: Remove `--workspace` flag or use a different setting. Global-only settings include telemetry and API endpoints.

### Issue: Changes Not Applied

**Symptom**: Setting changed but behavior unchanged  
**Cause**: Some settings require restart  
**Solution**: Restart chat session or Kiro CLI.

## Related Features

- [/experiment](../slash-commands/experiment.md) - Toggle experimental features
- [Tangent Mode](../features/tangent-mode.md) - Tangent mode feature
- [Agent Configuration](../agent-config/overview.md) - Agent-specific settings

## Limitations

- Some settings require restart to take effect
- No setting validation beyond type checking
- Array settings require JSON format
- Global-only settings cannot be overridden at workspace level

## Technical Details

**Storage**: 
- Global: `~/.kiro/settings/cli.json`
- Workspace: `.kiro/settings/cli.json`

**Scope**: Settings can be global (user-wide) or workspace-specific.

**Precedence**: Workspace settings override global settings for workspace-overridable settings.

**Types**: Boolean, String, Number, Array (JSON).

**Naming**: Settings use dot notation (e.g., `chat.enableTangentMode`).

**Aliases**: `setting` (singular) also works as alias.
