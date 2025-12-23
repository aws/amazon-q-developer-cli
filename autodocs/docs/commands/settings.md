---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: command
  title: kiro-cli settings
  description: Configure Kiro CLI behavior with get, set, list, open, and delete operations for all settings
  keywords: [settings, config, configure, preferences]
  related: [slash-experiment]
---

# kiro-cli settings

Configure Kiro CLI behavior with get, set, list, open, and delete operations for all settings.

## Overview

The settings command manages Kiro CLI configuration. List all settings, get current values, set new values, delete settings, or open the settings file. Settings persist across sessions and control features like tangent mode, default agent/model, and experimental features.

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
| `--format <FORMAT>` | `-f` | Output format: plain (markdown), json, json-pretty (default: plain) |
| `--verbose` | `-v` | Increase logging verbosity (can be repeated) |
| `--help` | `-h` | Print help information |

### Common Use Cases

#### Use Case 1: List All Settings

```bash
kiro-cli settings list
```

**What this does**: Shows all available settings with current values and descriptions.

#### Use Case 2: Get Setting Value

```bash
kiro-cli settings chat.enableTangentMode
```

**What this does**: Displays current value of tangent mode setting.

#### Use Case 3: Enable Feature

```bash
kiro-cli settings chat.enableTangentMode true
```

**What this does**: Enables tangent mode feature.

#### Use Case 4: Set Default Agent

```bash
kiro-cli settings chat.defaultAgent rust-expert
```

**What this does**: Sets rust-expert as default agent for new sessions.

#### Use Case 5: Delete Setting

```bash
kiro-cli settings --delete chat.enableTangentMode
```

**What this does**: Removes the setting, reverting to default value.

#### Use Case 6: Open Settings File

```bash
kiro-cli settings open
```

**What this does**: Opens settings file in default editor.

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
kiro-cli settings <SETTING_NAME> <VALUE>
```

**Value types**:
- Boolean: `true` or `false`
- String: any text
- Number: integer or decimal
- Array: JSON array format

### delete

Delete setting (revert to default).

```bash
kiro-cli settings --delete <SETTING_NAME>
```

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

### Example 1: Enable Tangent Mode

```bash
kiro-cli settings chat.enableTangentMode true
```

### Example 2: Set Default Model

```bash
kiro-cli settings chat.defaultModel <model-id>
```

### Example 3: Check Current Agent

```bash
kiro-cli settings chat.defaultAgent
```

**Expected Output**:
```
rust-expert
```

### Example 4: Delete Setting

```bash
kiro-cli settings --delete chat.enableTangentMode
```

### Example 5: Open Settings File

```bash
kiro-cli settings open
```

### Example 6: List All Settings

```bash
kiro-cli settings list
```

**Expected Output**:
```
chat.defaultAgent = "rust-expert"
chat.enableTangentMode = true
chat.defaultModel = "<model-id>"
chat.enableThinking = false
chat.enableKnowledge = false
...
```

Shows all settings with current values.

## Troubleshooting

### Issue: Setting Not Found

**Symptom**: "Unknown setting" error  
**Cause**: Invalid setting name  
**Solution**: Use `kiro-cli settings list` to see all valid settings.

### Issue: Invalid Value

**Symptom**: "Invalid value" error  
**Cause**: Wrong type for setting  
**Solution**: Check setting type in list. Use `true`/`false` for booleans, not `yes`/`no`.

### Issue: Changes Not Applied

**Symptom**: Setting changed but behavior unchanged  
**Cause**: Some settings require restart  
**Solution**: Restart chat session or Kiro CLI.

## Related Features

- [/experiment](../slash-commands/experiment.md) - Toggle experimental features
- [Tangent Mode](../features/tangent-mode.md) - Tangent mode feature
- [Agent Configuration](../agent-config/overview.md) - Agent-specific settings

## Limitations

- Settings are global (not per-workspace)
- Some settings require restart to take effect
- No setting validation beyond type checking
- Array settings require JSON format

## Technical Details

**Storage**: Settings stored in global database (`~/.kiro/`).

**Scope**: All settings are user-wide, not workspace-specific.

**Types**: Boolean, String, Number, Array (JSON).

**Naming**: Settings use dot notation (e.g., `chat.enableTangentMode`).

**Aliases**: `setting` (singular) also works as alias.
