# Tangent Mode

Tangent mode creates conversation checkpoints, allowing you to explore side topics without disrupting your main conversation flow. Enter tangent mode, ask questions or explore ideas, then return to your original conversation exactly where you left off.

## Enabling Tangent Mode

Tangent mode is experimental and must be enabled:

**Via Experiment Command**: Run `/experiment` and select tangent mode from the list.

**Via Settings**: `q settings chat.enableTangentMode true`

## Basic Usage

### Enter Tangent Mode
Use `/tangent` or Ctrl+T:
```
> /tangent
Created a conversation checkpoint (↯). Use ctrl + t or /tangent to restore the conversation later.
```

### In Tangent Mode
You'll see a yellow `↯` symbol in your prompt:
```
↯ > What is the difference between async and sync functions?
```

### Exit Tangent Mode
Use `/tangent` or Ctrl+T again:
```
↯ > /tangent
Restored conversation from checkpoint (↯). - Returned to main conversation.
```

### Exit Tangent Mode with Tail
Use `/tangent tail` to preserve the last conversation entry (question + answer):
```
↯ > /tangent tail
Restored conversation from checkpoint (↯) with last conversation entry preserved.
```

## Usage Examples

### Example 1: Exploring Alternatives
```
> I need to process a large CSV file in Python. What's the best approach?

I recommend using pandas for CSV processing...

> /tangent
Created a conversation checkpoint (↯).

↯ > What about using the csv module instead of pandas?

The csv module is lighter weight...

↯ > /tangent
Restored conversation from checkpoint (↯).

> Thanks! I'll go with pandas. Can you show me error handling?
```

### Example 2: Getting Q CLI Help
```
> Help me write a deployment script

I can help you create a deployment script...

> /tangent
Created a conversation checkpoint (↯).

↯ > What Q CLI commands are available for file operations?

Q CLI provides fs_read, fs_write, execute_bash...

↯ > /tangent
Restored conversation from checkpoint (↯).

> It's a Node.js application for AWS
```

### Example 3: Clarifying Requirements
```
> I need to optimize this SQL query

Could you share the query you'd like to optimize?

> /tangent
Created a conversation checkpoint (↯).

↯ > What information do you need to help optimize a query?

To optimize SQL queries effectively, I need:
1. The current query
2. Table schemas and indexes...

↯ > /tangent
Restored conversation from checkpoint (↯).

> Here's my query: SELECT * FROM orders...
```

### Example 4: Keeping Useful Information
```
> Help me debug this Python error

I can help you debug that. Could you share the error message?

> /tangent
Created a conversation checkpoint (↯).

↯ > What are the most common Python debugging techniques?

Here are the most effective Python debugging techniques:
1. Use print statements strategically
2. Leverage the Python debugger (pdb)...

↯ > /tangent tail
Restored conversation from checkpoint (↯) with last conversation entry preserved.

> Here's my error: TypeError: unsupported operand type(s)...

# The preserved entry (question + answer about debugging techniques) is now part of main conversation
```

## Configuration

### Keyboard Shortcut
```bash
# Change shortcut key (default: t)
q settings chat.tangentModeKey y
```

### Auto-Tangent for Introspect
```bash
# Auto-enter tangent mode for Q CLI help questions
q settings introspect.tangentMode true
```

## Visual Indicators

- **Normal mode**: `> ` (magenta)
- **Tangent mode**: `↯ > ` (yellow ↯ + magenta)
- **With profile**: `[dev] ↯ > ` (cyan + yellow ↯ + magenta)

## Best Practices

### When to Use Tangent Mode
- Asking clarifying questions about the current topic
- Exploring alternative approaches before deciding
- Getting help with Q CLI commands or features
- Testing understanding of concepts

### When NOT to Use
- Completely unrelated topics (start new conversation)
- Long, complex discussions (use regular flow)
- When you want the side discussion in main context

### Tips
1. **Keep tangents focused** - Brief explorations, not extended discussions
2. **Return promptly** - Don't forget you're in tangent mode
3. **Use for clarification** - Perfect for "wait, what does X mean?" questions
4. **Experiment safely** - Test ideas without affecting main conversation
5. **Use `/tangent tail`** - When both the tangent question and answer are useful for main conversation

## Limitations

- Tangent conversations are discarded when you exit
- Only one level of tangent supported (no nested tangents)
- Experimental feature that may change or be removed
- Must be explicitly enabled

## Troubleshooting

### Tangent Mode Not Working
```bash
# Enable via experiment (select from list)
/experiment

# Or enable via settings
q settings chat.enableTangentMode true
```

### Keyboard Shortcut Not Working
```bash
# Check/reset shortcut key
q settings chat.tangentModeKey t
```

### Lost in Tangent Mode
Look for the `↯` symbol in your prompt. Use `/tangent` to exit and return to main conversation.

## Related Features

- **Introspect**: Q CLI help (auto-enters tangent if configured)
- **Experiments**: Manage experimental features with `/experiment`
