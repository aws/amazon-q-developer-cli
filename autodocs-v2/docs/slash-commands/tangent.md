---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /tangent
  description: Create conversation checkpoints to explore side topics without disrupting main conversation flow
  keywords: [tangent, checkpoint, branch, explore, forget]
  related: [tangent-mode, introspect]
---

# /tangent

Create conversation checkpoints to explore side topics without disrupting main conversation flow.

## Overview

The `/tangent` command creates conversation checkpoints, allowing exploration of side topics without affecting main conversation. Enter tangent mode, ask questions, then return to original conversation exactly where you left off. Visual indicator (`↯`) shows tangent mode status.

## Usage

### Toggle Tangent Mode

```
/tangent
```

**Keyboard Shortcut**: `Ctrl+T` (configurable)

### Exit with Last Entry

```
/tangent tail
```

Preserves last conversation entry (question + answer) when exiting.

### Forget Messages

```
/tangent forget [count]
```

Remove last N messages. Without count, shows interactive picker.

## How It Works

1. **Enter**: `/tangent` creates checkpoint of current conversation
2. **Explore**: Ask questions in tangent mode (marked with `↯`)
3. **Exit**: `/tangent` restores conversation to checkpoint
4. **Tail**: `/tangent tail` restores but keeps last Q&A

## Examples

### Example 1: Explore Alternative

```
> I need to process CSV in Python

Use pandas...

> /tangent
Created checkpoint (↯)

↯ > What about csv module instead?

csv module is lighter...

↯ > /tangent
Restored from checkpoint (↯)

> I'll use pandas. Show error handling?
```

### Example 2: Get Help

```
> Help me write deployment script

↯ > What Kiro CLI commands are available?

Kiro CLI provides fs_read, fs_write...

↯ > /tangent
Restored from checkpoint
```

### Example 3: Keep Useful Info

```
> Debug this Python error

↯ > What are common debugging techniques?

1. Print statements
2. Use pdb debugger...

↯ > /tangent tail
Restored with last entry preserved

> Here's my error: TypeError...
```

### Example 4: Backtrack

```
> Optimize this query

PostgreSQL query...

> Try indexing approach

Here's how...

> /tangent forget 2
Forgetting last 2 messages

> Actually, let's restructure the query
```

### Example 5: Interactive Forget

```
> /tangent forget

? Select message to revert to:
  How do I optimize... (forget 1 message)
  What about indexes... (forget 2 messages)
  Clear all messages (forget 5 messages)
```

## Configuration

### Enable Tangent Mode

```bash
kiro-cli settings chat.enableTangentMode true
```

Or use `/experiment` and select tangent mode.

### Change Keyboard Shortcut

```bash
kiro-cli settings chat.tangentModeKey y
```

Default: `t` (Ctrl+T)

### Auto-Tangent for Introspect

```bash
kiro-cli settings chat.introspectTangentMode true
```

Automatically enters tangent mode for Kiro CLI help questions.

## Visual Indicators

- **Normal**: `> ` (magenta)
- **Tangent**: `↯ > ` (yellow ↯ + magenta)
- **With agent**: `[agent] ↯ > `

## Subcommands

### (no subcommand)

Toggle tangent mode on/off.

```
/tangent
```

### tail

Exit tangent mode, keep last conversation entry.

```
/tangent tail
```

Useful when tangent Q&A is relevant to main conversation.

### forget

Remove last N messages from conversation.

```
/tangent forget [count]
```

Without count: Interactive picker  
With count: Remove N messages immediately

**Warning**: Counts >5 show warning. Cannot be undone.

## Best Practices

### When to Use

- Clarifying questions about current topic
- Exploring alternatives before deciding
- Getting Kiro CLI help
- Testing understanding

### When NOT to Use

- Completely unrelated topics (start new conversation)
- Long complex discussions
- When you want side discussion in main context

### Tips

1. Keep tangents focused and brief
2. Return promptly (don't forget you're in tangent)
3. Use for "wait, what does X mean?" questions
4. Use `/tangent tail` when both Q&A are useful

## Troubleshooting

### Issue: Tangent Mode Not Working

**Symptom**: Command does nothing  
**Cause**: Feature not enabled  
**Solution**: `kiro-cli settings chat.enableTangentMode true`

### Issue: Keyboard Shortcut Not Working

**Symptom**: Ctrl+T doesn't work  
**Cause**: Key not configured or conflict  
**Solution**: Check `kiro-cli settings chat.tangentModeKey`

### Issue: Lost in Tangent Mode

**Symptom**: Don't know if in tangent  
**Cause**: Missed visual indicator  
**Solution**: Look for `↯` in prompt. Use `/tangent` to exit.

### Issue: Checkpoint Disabled Warning

**Symptom**: Warning about checkpoint feature  
**Cause**: Checkpoint experiment conflicts with tangent  
**Solution**: Checkpoint disabled in tangent mode. Exit tangent to use checkpoint.

## Related Features

- [Tangent Mode Feature](../features/tangent-mode.md) - Complete guide
- [introspect](../tools/introspect.md) - Auto-tangent for help
- [/experiment](experiment.md) - Enable experimental features
- [Settings](../commands/settings.md) - Configure tangent mode

## Limitations

- Tangent conversations discarded on exit (unless using tail)
- Only one level of tangent (no nested tangents)
- Experimental feature may change
- Must be explicitly enabled
- Checkpoint feature disabled in tangent mode

## Technical Details

**Checkpoint**: Saves conversation state when entering tangent mode.

**Restore**: Reverts to checkpoint state when exiting.

**Tail**: Preserves last user prompt + assistant response when exiting.

**Forget**: Removes last N "messages" (user prompt + all assistant responses until next user prompt).

**Duration Tracking**: Tracks time spent in tangent mode for telemetry.

**Visual Indicator**: Yellow `↯` symbol added to prompt in tangent mode.

**Keyboard Binding**: Configurable via `chat.tangentModeKey` setting (default: 't').
