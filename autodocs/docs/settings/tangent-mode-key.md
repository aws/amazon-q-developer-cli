---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: setting
  title: chat.tangentModeKey
  description: Configure keyboard shortcut for tangent mode toggle
  keywords: [setting, tangent, key, shortcut, keybinding]
  related: [enable-tangent-mode, slash-tangent]
---

# chat.tangentModeKey

Configure keyboard shortcut for tangent mode toggle.

## Overview

Sets the keyboard shortcut key for toggling tangent mode. The key is used with Ctrl (Ctrl+key) to quickly switch between main conversation and tangent mode for side discussions or help queries.

## Usage

```bash
kiro-cli settings chat.tangentModeKey t
```

**Type**: String (single character)  
**Default**: `t` (Ctrl+T)

## Examples

```bash
# Set to 'y' (Ctrl+Y)
kiro-cli settings chat.tangentModeKey y

# Delete setting (revert to default)
kiro-cli settings --delete chat.tangentModeKey
```

## Troubleshooting

### Issue: Shortcut Not Working

**Symptom**: New key doesn't work  
**Cause**: Invalid key or conflict  
**Solution**: Use single character (a-z). Restart session.

### Issue: Forgot What Key Is Set

**Symptom**: Don't know current shortcut  
**Cause**: Changed and forgot  
**Solution**: Check with `kiro-cli settings chat.tangentModeKey`

## Related

- [/tangent](../slash-commands/tangent.md) - Tangent mode command
- [chat.enableTangentMode](enable-tangent-mode.md) - Enable feature
