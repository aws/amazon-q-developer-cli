---
doc_meta:
  validated: 2026-05-16
  commit: 531268b4
  status: validated
  testable_headless: true
  category: setting
  title: chat.showThinking
  description: Display agent reasoning blocks in chat output
  keywords: [setting, thinking, reasoning, extended thinking, debug]
  related: [cmd-settings]
---

# chat.showThinking

Display the agent's reasoning (thinking) blocks in chat output.

## Overview

The `chat.showThinking` setting controls whether the TUI renders streaming reasoning/thinking blocks emitted by the agent. When enabled, you can see the agent's internal reasoning process as it works through your request.

This is a **startup-only** setting — changes take effect on the next chat session, not the current one.

The live "Thinking..." spinner is unaffected by this setting and always appears while the agent is processing.

## Usage

### Enable Thinking Display

```bash
kiro-cli settings chat.showThinking true
```

### Disable Thinking Display

```bash
kiro-cli settings chat.showThinking false
```

### Check Current Value

```bash
kiro-cli settings chat.showThinking
```

## Value

**Type**: Boolean  
**Default**: `false`  
**Scope**: Global and workspace

## Examples

### Example 1: Enable Thinking Display

```bash
kiro-cli settings chat.showThinking true
```

Then start a new chat session to see reasoning blocks.

### Example 2: Disable Thinking Display

```bash
kiro-cli settings chat.showThinking false
```

### Example 3: Check Current Setting

```bash
kiro-cli settings chat.showThinking
```

**Output**: `true` or `false`

### Example 4: Enable for Specific Workspace

Edit `.kiro/settings/cli.json` in your project:

```json
{
  "chat.showThinking": true
}
```

## When to Use

Enable this setting when you want to:

- Debug agent behavior and understand its reasoning
- Learn how the agent approaches complex problems
- Verify the agent is considering the right context
- Troubleshoot unexpected responses

Keep it disabled (default) for:

- Cleaner, less verbose output
- Faster visual scanning of responses
- Normal day-to-day usage

## Related

- [kiro-cli settings](../commands/settings.md) - Manage all settings

## Troubleshooting

### Issue: Setting Changed But No Effect

**Symptom**: Enabled the setting but still not seeing thinking blocks  
**Cause**: This is a startup-only setting  
**Solution**: Start a new chat session. The setting does not apply to the current session.

### Issue: No Thinking Blocks Appear

**Symptom**: Setting enabled, new session started, but no thinking blocks  
**Cause**: The model may not emit thinking blocks for simple requests  
**Solution**: Thinking blocks typically appear for complex reasoning tasks. Try a more complex request.

### Issue: Too Much Output

**Symptom**: Thinking blocks make output hard to read  
**Solution**: Disable the setting with `kiro-cli settings chat.showThinking false` and restart the session.
