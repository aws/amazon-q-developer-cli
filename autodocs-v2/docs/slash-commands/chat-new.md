---
doc_meta:
  validated: 2026-03-22
  commit: b8a43f49
  status: validated
  testable_headless: false
  category: slash_command
  title: /chat new
  description: Start a fresh conversation without restarting the CLI
  keywords: [chat, new, fresh, conversation, reset, start over]
  related: [chat-load, chat-save, clear]
---

# /chat new

Start a fresh conversation without restarting the CLI.

## Overview

The `/chat new` command creates a brand new conversation session, clearing all messages and starting fresh. The previous session is preserved and can be resumed later with `/chat`.

This command is available in both V1 (legacy UI) and V2 (TUI). In V2, it creates a new ACP session and terminates the old one.

## Usage

### Start a fresh conversation

```
/chat new
```

### Start with an initial prompt

```
/chat new <prompt>
```

Immediately sends the prompt to the new conversation after creation.

## Examples

### Basic usage

```
> /chat new
✔ New conversation started. Use /chat to return to previous sessions.
```

### With an initial prompt

```
> /chat new explain how async works in Rust
```

This creates a new conversation and immediately sends "explain how async works in Rust" as the first message.

## How It Works

1. The current session is saved automatically
2. A new session is created with a fresh conversation ID
3. All messages are cleared from the display
4. Model and agent settings are preserved
5. MCP servers remain available (V2 creates new server instances)

## Differences from /clear

| Feature | `/chat new` | `/clear` |
|---------|-------------|----------|
| Creates new session ID | Yes | No |
| Previous session resumable via `/chat` | Yes | No |
| Clears conversation history | Yes | Yes |
| Resets context window | Yes | Yes |
| Preserves model selection | Yes | Yes |
| Preserves agent selection | Yes | Yes |

## Related

- `/chat` — List and load previous sessions
- `/chat save` — Save conversation to file
- `/chat load` — Load conversation from file
- `/clear` — Clear conversation without creating new session

## Limitations

- In V1, the previous session must have been saved to the database to be resumable
- In V2, MCP servers are re-initialized in the new session (brief startup delay)
