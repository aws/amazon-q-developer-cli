---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: Tangent Mode
  description: Experimental feature for creating conversation checkpoints to explore side topics without disrupting main flow
  keywords: [tangent, checkpoint, experimental, branch, explore]
  related: [slash-tangent, enable-tangent-mode]
---

# Tangent Mode

Experimental feature for creating conversation checkpoints to explore side topics without disrupting main flow.

## Overview

Tangent mode creates conversation checkpoints, allowing exploration of side topics, clarifying questions, or alternative approaches without affecting the main conversation. Enter tangent mode, explore, then return to exactly where you left off.

## Enabling

```bash
kiro-cli settings chat.enableTangentMode true
```

Or use `/experiment` and select tangent mode.

## Usage

See [/tangent command documentation](../slash-commands/tangent.md) for complete usage guide.

### Quick Start

1. **Enter**: `/tangent` or `Ctrl+T`
2. **Explore**: Ask questions (prompt shows `↯`)
3. **Exit**: `/tangent` or `Ctrl+T` again

## Use Cases

- **Clarifying questions**: "Wait, what does X mean?"
- **Exploring alternatives**: "What if we used Y instead?"
- **Getting help**: "How do I use this Kiro CLI feature?"
- **Testing understanding**: "Can you explain that differently?"

## Configuration

### Keyboard Shortcut

```bash
kiro-cli settings chat.tangentModeKey t
```

Default: `t` (Ctrl+T)

### Auto-Tangent for Help

```bash
kiro-cli settings chat.introspectTangentMode true
```

Automatically enters tangent mode for Kiro CLI help questions.

## Limitations

- Experimental feature (may change or be removed)
- Only one level of tangent (no nesting)
- Tangent conversations discarded on exit (unless using tail)
- Must be explicitly enabled
- Conflicts with checkpoint feature

## Examples

### Example 1: Explore Alternative

```
> I need to process CSV in Python

Use pandas...

> /tangent
Created checkpoint (↯)

↯ > What about csv module?

csv module is lighter...

↯ > /tangent
Restored from checkpoint

> I'll use pandas
```

### Example 2: Get Help

```
> Write deployment script

↯ > What Kiro CLI file commands exist?

fs_read, fs_write, execute_bash...

↯ > /tangent
Restored
```

### Example 3: Keep Useful Info

```
↯ > What are debugging techniques?

1. Print statements
2. Use debugger...

↯ > /tangent tail
Restored with last entry preserved
```

## Troubleshooting

### Issue: Not Working

**Symptom**: `/tangent` does nothing  
**Cause**: Not enabled  
**Solution**: `kiro-cli settings chat.enableTangentMode true`

### Issue: Shortcut Not Working

**Symptom**: Ctrl+T doesn't work  
**Cause**: Key not configured  
**Solution**: Check `kiro-cli settings chat.tangentModeKey`

### Issue: Lost in Tangent

**Symptom**: Don't know if in tangent  
**Cause**: Missed `↯` indicator  
**Solution**: Look for yellow `↯` in prompt

### Issue: Checkpoint Conflict

**Symptom**: Checkpoint disabled warning  
**Cause**: Features conflict  
**Solution**: Exit tangent to use checkpoint

## Related

- [/tangent](../slash-commands/tangent.md) - Command documentation
- [chat.enableTangentMode](../settings/enable-tangent-mode.md) - Enable setting
- [introspect](../tools/introspect.md) - Auto-tangent for help
