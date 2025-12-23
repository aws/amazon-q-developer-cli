---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.enableTangentMode
  description: Enable tangent mode feature for conversation checkpoints
  keywords: [setting, tangent, checkpoint, experimental]
  related: [slash-tangent, tangent-mode]
---

# chat.enableTangentMode

Enable tangent mode feature for conversation checkpoints.

## Overview

The `chat.enableTangentMode` setting enables the tangent mode experimental feature, allowing creation of conversation checkpoints to explore side topics without disrupting main conversation flow.

## Usage

### Enable Tangent Mode

```bash
kiro-cli settings chat.enableTangentMode true
```

### Disable Tangent Mode

```bash
kiro-cli settings chat.enableTangentMode false
```

### Check Status

```bash
kiro-cli settings chat.enableTangentMode
```

## Value

**Type**: Boolean  
**Default**: `false`  
**Values**: `true` or `false`

## Examples

### Example 1: Enable Feature

```bash
kiro-cli settings chat.enableTangentMode true
```

Enables `/tangent` command and Ctrl+T shortcut.

### Example 2: Check if Enabled

```bash
kiro-cli settings chat.enableTangentMode
```

**Output**: `true` or `false`

### Example 3: Disable Feature

```bash
kiro-cli settings chat.enableTangentMode false
```

## Related

- [/tangent](../slash-commands/tangent.md) - Tangent mode command
- [Tangent Mode](../features/tangent-mode.md) - Complete guide
- [/experiment](../slash-commands/experiment.md) - Alternative way to enable
- [chat.tangentModeKey](tangent-mode-key.md) - Configure keyboard shortcut

## Troubleshooting

### Issue: Feature Not Working

**Symptom**: `/tangent` doesn't work after enabling  
**Cause**: Need to restart session  
**Solution**: Exit and restart chat

### Issue: Keyboard Shortcut Not Working

**Symptom**: Ctrl+T doesn't work  
**Cause**: Key binding not configured  
**Solution**: Check `kiro-cli settings chat.tangentModeKey`
