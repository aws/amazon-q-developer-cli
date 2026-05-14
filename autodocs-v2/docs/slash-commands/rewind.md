---
doc_meta:
  validated: 2026-05-13
  commit: 32e2af204
  status: validated
  testable_headless: false
  category: slash_command
  title: /rewind
  description: Fork conversation at an earlier turn to explore a different path
  keywords: [rewind, fork, branch, undo, history, turn, rollback, session]
  related: [chat-new, chat-load, compact, context]
---

# /rewind

Fork conversation at an earlier turn to explore a different path.

## Overview

The `/rewind` command lets you go back to an earlier point in your conversation and continue from there. Instead of modifying the current session, it creates a new session containing only the history up to your selected turn. The original session remains untouched.

This is useful when you want to:
- Try a different approach after the AI went down the wrong path
- Explore an alternative solution without losing your current progress
- Recover from a conversation that got off track

## Usage

```
/rewind
```

Opens a picker showing all your previous prompts. Select one to fork from that point.

```
/rewind <log_index>
```

Fork directly at the specified log entry index (for scripting or when you know the exact turn).

## Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `log_index` | No | Index of the prompt entry to rewind to. If omitted, opens interactive picker. |

## How It Works

1. `/rewind` displays a list of all user prompts in the current session
2. Each entry shows the prompt preview and context usage at that turn
3. Select a turn to fork from
4. A new session is created containing history up to (and including) that turn's response
5. The TUI automatically switches to the new session
6. Continue the conversation from that point

The original session is preserved—you can switch back to it with `/chat load` or the session picker.

## Examples

### Example 1: Interactive Rewind

```
/rewind
```

Opens a picker showing your conversation turns:

```
┌─ Select turn to rewind to ─────────────────────────────┐
│ > Help me refactor the authentication module          │ 45%
│   Add unit tests for the login function               │ 32%
│   Create a new user registration endpoint             │ 18%
│   Set up the project structure                        │  5%
└────────────────────────────────────────────────────────┘
```

The percentage shows context window usage at each turn. Selecting "Add unit tests..." creates a new session starting from that point.

### Example 2: Rewind to Specific Turn

```
/rewind 4
```

Forks the session at log entry index 4 (the third user prompt, since indices start at 0 and include assistant responses).

**Output**:
```
Rewound to earlier turn (new session abc123-def456)
```

### Example 3: Exploring Alternatives

You asked the AI to implement a feature using approach A, but now want to try approach B:

```
You: Implement caching using Redis
AI: [implements Redis caching...]
You: Actually, let's try a different approach
/rewind
```

Select the turn before "Implement caching using Redis", then ask for a different approach:

```
You: Implement caching using an in-memory LRU cache instead
```

## Picker Display

The rewind picker shows:

| Column | Description |
|--------|-------------|
| Prompt preview | First 80 characters of your message |
| Context % | Context window usage at that turn (e.g., "45%") |
| Response preview | First few lines of the AI's response (on hover) |

Turns are listed newest-first for quick access to recent history.

## Troubleshooting

### Issue: "Session has no turns to rewind to"

**Cause**: The current session has no conversation history yet.  
**Solution**: Have at least one exchange before using `/rewind`.

### Issue: "Turn index N is out of range"

**Cause**: The specified log index doesn't exist.  
**Solution**: Use `/rewind` without arguments to see available turns, or check the index is within range.

### Issue: "Entry at index N is not a user prompt"

**Cause**: The specified index points to an assistant message or other log entry type.  
**Solution**: Use `/rewind` without arguments to select from valid prompt entries.

### Issue: Rewound session missing some context

**Cause**: If the original session was compacted, rewinding past the compaction point restores full pre-compaction history.  
**Solution**: This is expected behavior—the new session has the complete history that existed at that turn.

## Related Features

- [/chat new](chat-new.md) — Start a completely fresh session
- [/chat load](chat-load.md) — Load a previously saved session
- [/compact](compact.md) — Summarize history to free context space
- [/context](context.md) — Check current context window usage
