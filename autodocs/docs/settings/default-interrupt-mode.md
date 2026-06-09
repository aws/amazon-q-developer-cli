---
doc_meta:
  validated: 2026-06-08
  commit: 18f860655
  status: validated
  testable_headless: true
  category: setting
  title: chat.defaultInterruptBehavior
  description: Default follow-up delivery mode for new chat sessions
  keywords: [setting, follow-up, steering, queuing, mid-turn, queue, interrupt, steer]
  related: [mid-turn-steering, key-bindings-settings]
---

# chat.defaultInterruptBehavior

Set the default follow-up delivery mode for new chat sessions.

## Overview

The `chat.defaultInterruptBehavior` setting controls whether new chat sessions start in steering mode (messages injected mid-turn) or queuing mode (messages buffered until turn ends). You can toggle between modes at runtime with `Ctrl+S`.

## Usage

### Set to Steer Mode (default)

```bash
kiro-cli settings set chat.defaultInterruptBehavior steer
```

### Set to Queue Mode

```bash
kiro-cli settings set chat.defaultInterruptBehavior queue
```

### Check Current Setting

```bash
kiro-cli settings get chat.defaultInterruptBehavior
```

### Via /settings Menu

```
/settings

> terminal
> interrupt behaviour
> steer ●     Inject interrupts mid-turn at tool boundaries
> queue       Buffer interrupts and send after turn ends
```

## Value

**Type**: String  
**Default**: `steer`  
**Values**: `steer` or `queue`

## Mode Comparison

| Mode | Behavior | Best For |
|------|----------|----------|
| `steer` | Messages sent to backend, injected at next tool boundary | Real-time course correction |
| `queue` | Messages buffered locally, sent after turn ends | Batching multiple interrupts |

## Examples

### Example 1: Default Steer Behavior

```bash
kiro-cli settings set chat.defaultInterruptBehavior steer
kiro-cli chat
```

New session starts in steering mode. Follow-up messages typed while agent is processing are injected mid-turn.

### Example 2: Default Queue Behavior

```bash
kiro-cli settings set chat.defaultInterruptBehavior queue
kiro-cli chat
```

New session starts in queuing mode. Follow-up messages are buffered locally until the current turn ends.

### Example 3: Check and Toggle

```bash
# Check current default
kiro-cli settings get chat.defaultInterruptBehavior
# Output: steer

# Switch to queue as default
kiro-cli settings set chat.defaultInterruptBehavior queue
```

## Related

- [Mid-Turn Steering](../features/mid-turn-steering.md) - Complete guide to follow-up modes
- [Key Bindings Settings](key-bindings-settings.md) - Configure `Ctrl+S` toggle keybinding

## Troubleshooting

### Issue: New Session Not Using Expected Mode

**Symptom**: Started a new chat but it's in the wrong mode  
**Cause**: Setting was changed after session started  
**Solution**: Setting only affects new sessions. Use `Ctrl+S` to toggle in current session.

### Issue: Setting Not Persisting

**Symptom**: Setting resets after restart  
**Cause**: Invalid value or write failure  
**Solution**: Use exact values `steer` or `queue` (case-sensitive)
