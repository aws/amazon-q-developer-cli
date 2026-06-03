---
doc_meta:
  validated: 2026-06-03
  commit: 54d1f048f
  status: validated
  testable_headless: true
  category: setting
  title: chat.showThinking
  description: Control how agent reasoning blocks are displayed in chat
  keywords: [setting, thinking, reasoning, extended thinking, debug, collapsed, expanded]
  related: [cmd-settings]
---

# chat.showThinking

Control how the agent's reasoning (thinking) blocks are displayed in chat.

## Overview

The `chat.showThinking` setting controls how the TUI renders reasoning/thinking blocks emitted by the agent. You can choose to always see the full reasoning stream, see just a collapsed header that expands on demand, or hide reasoning entirely.

When a reasoning block completes, it displays "Thought for Ns" showing how long the agent spent reasoning.

## Values

| Value | Description |
|-------|-------------|
| `expanded` | Always show the full reasoning stream inline (default for new users) |
| `collapsed` | Show header only; press `ctrl+o` to expand the full stream |
| `off` | Hide reasoning blocks entirely |

**Type**: String (`expanded`, `collapsed`, `off`)  
**Default**: `expanded`  
**Scope**: Global and workspace

### Backward Compatibility

Legacy boolean values are still accepted:
- `true` → `collapsed`
- `false` → `off`

## Usage

### Set to Expanded (Always Show Reasoning)

```bash
kiro-cli settings chat.showThinking expanded
```

### Set to Collapsed (Header Only, Ctrl+O to View)

```bash
kiro-cli settings chat.showThinking collapsed
```

### Set to Off (Hide Reasoning)

```bash
kiro-cli settings chat.showThinking off
```

### Check Current Value

```bash
kiro-cli settings chat.showThinking
```

## Examples

### Example 1: Enable Full Reasoning Display

```bash
kiro-cli settings chat.showThinking expanded
```

Reasoning streams inline as the agent thinks, showing all content.

### Example 2: Collapsed Mode with On-Demand Expansion

```bash
kiro-cli settings chat.showThinking collapsed
```

You'll see a header like "Thinking..." while active, or "Thought for 5s" when complete. Press `ctrl+o` to expand and see the full reasoning, press again to collapse.

### Example 3: Hide Reasoning Entirely

```bash
kiro-cli settings chat.showThinking off
```

No reasoning blocks appear — only the final response.

### Example 4: Configure in Workspace Settings

Edit `.kiro/settings/cli.json` in your project:

```json
{
  "chat.showThinking": "collapsed"
}
```

### Example 5: Using the Display Settings Panel

Press `/settings` in chat, then navigate to "Show thinking" and use left/right arrows to cycle through `collapsed` → `expanded` → `off`.

## Keyboard Shortcuts

In `collapsed` mode:

| Key | Action |
|-----|--------|
| `ctrl+o` | Toggle between collapsed (header only) and expanded (full stream) |

The `ctrl+o` shortcut is shared with tool output expansion — pressing it toggles both thinking and tool outputs together.

## Display Behavior

### While Reasoning

- **expanded**: Full reasoning text streams inline
- **collapsed**: Shows "Thinking..." header with a `ctrl+o to view` hint
- **off**: Nothing displayed

### After Reasoning Completes

- **expanded**: Full reasoning text visible, header shows "Thought for Ns"
- **collapsed**: Shows "Thought for Ns" header; `ctrl+o` to view full text
- **off**: Nothing displayed

### In Chat History

Once a turn scrolls into history (static buffer), the reasoning block shows only the "Thought for Ns" header — the body is not expandable in historical turns.

## When to Use

**Use `expanded`** when you want to:
- Watch the agent's reasoning in real-time
- Debug agent behavior and understand its approach
- Learn how the agent tackles complex problems

**Use `collapsed`** when you want to:
- Keep output clean but have reasoning available on demand
- Reduce visual noise while retaining access to reasoning
- Focus on responses but occasionally check reasoning

**Use `off`** when you want to:
- Maximum clean output
- Faster visual scanning of responses
- Hide verbose reasoning for simple tasks

## Related

- [kiro-cli settings](../commands/settings.md) — Manage all settings

## Troubleshooting

### Issue: No Thinking Blocks Appear

**Symptom**: Setting is `expanded` or `collapsed` but no thinking blocks appear  
**Cause**: The model may not emit thinking blocks for simple requests  
**Solution**: Thinking blocks typically appear for complex reasoning tasks. Try a more complex request.

### Issue: Ctrl+O Not Working

**Symptom**: Pressing `ctrl+o` doesn't expand the thinking block  
**Cause**: Either the mode is `expanded` (always shown, nothing to toggle) or `off` (nothing rendered)  
**Solution**: Set mode to `collapsed` for toggle behavior: `kiro-cli settings chat.showThinking collapsed`

### Issue: Can't Expand Historical Reasoning

**Symptom**: Old thinking blocks only show "Thought for Ns" with no way to expand  
**Cause**: Historical turns (in the static buffer) show only the header  
**Solution**: This is expected. Reasoning is only expandable for the current active turn.

### Issue: Legacy Boolean Setting

**Symptom**: Had `chat.showThinking: true` but now it behaves differently  
**Cause**: `true` now maps to `collapsed` (header only by default)  
**Solution**: To get always-visible reasoning, set explicitly to `expanded`: `kiro-cli settings chat.showThinking expanded`
