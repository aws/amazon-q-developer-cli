# Tangent Mode

Tangent mode creates conversation checkpoints, allowing you to explore side topics without disrupting your main conversation flow. Enter tangent mode, ask questions or explore ideas, then return to your original conversation exactly where you left off.

## Enabling Tangent Mode

Tangent mode is experimental and must be enabled:

**Via Experiment Command**: Run `/experiment` and select tangent mode from the list.

**Via Settings**: `kiro-cli settings chat.enableTangentMode true`

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

### Forget Last Conversations
Use `/tangent forget N` to remove the last N messages from your current conversation:
```
> /tangent forget 2
Seems like you went on a tangent! Forgetting the last 2 messages.
```

Or use `/tangent forget` without a count to interactively select which message to revert back to:
```
> /tangent forget
? Select the message to revert back to (newer messages will be forgotten)
  How do I optimize this query... (forget 1 message after this)
  What about using indexes... (forget 2 messages after this)
  Clear all messages (forget 5 messages)
```

A "message" is one user prompt and all the assistant responses (including any tool use chains) until the next user prompt. This is useful when you realize your last few questions went in the wrong direction and want to backtrack.

**Note:** The count is based on your messages, not the total conversation entries. If your question triggered tool use, the entire chain is removed as one message.

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

### Example 2: Getting KIRO CLI Help
```
> Help me write a deployment script

I can help you create a deployment script...

> /tangent
Created a conversation checkpoint (↯).

↯ > What KIRO CLI commands are available for file operations?

KIRO CLI provides fs_read, fs_write, execute_bash...

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

### Example 5: Backtracking from Wrong Direction
```
> Help me optimize this database query

Let me help you with that. What database are you using?

> PostgreSQL. Here's my query: SELECT * FROM users WHERE...

I see several optimization opportunities...

> Actually, let me try a different approach with indexing

Here's how to add indexes...

> /tangent forget 2
Seems like you went on a tangent! Forgetting the last 2 messages.

# Now back to the point where you shared the query, can start fresh with a better approach
> I think the real issue is the query design itself. Can we restructure it?
```

## Configuration

### Keyboard Shortcut
```bash
# Change shortcut key (default: t)
kiro-cli settings chat.tangentModeKey y
```

### Auto-Tangent for Introspect
```bash
# Auto-enter tangent mode for KIRO CLI help questions
kiro-cli settings introspect.tangentMode true
```

## Visual Indicators

- **Normal mode**: `> ` (magenta)
- **Tangent mode**: `↯ > ` (yellow ↯ + magenta)
- **With profile**: `[dev] ↯ > ` (cyan + yellow ↯ + magenta)

## Best Practices

### When to Use Tangent Mode
- Asking clarifying questions about the current topic
- Exploring alternative approaches before deciding
- Getting help with KIRO CLI commands or features
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
kiro-cli settings chat.enableTangentMode true
```

### Keyboard Shortcut Not Working
```bash
# Check/reset shortcut key
kiro-cli settings chat.tangentModeKey t
```

### Lost in Tangent Mode
Look for the `↯` symbol in your prompt. Use `/tangent` to exit and return to main conversation.

## Related Features

- **Introspect**: KIRO CLI help (auto-enters tangent if configured)
- **Experiments**: Manage experimental features with `/experiment`
