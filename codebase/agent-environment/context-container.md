# Context Container & Conversation History

## Overview

The `ContextContainer` manages contextual information for each Worker, primarily conversation history. It enables Workers to maintain state across multiple turns and supports multi-turn conversations.

## Architecture

```
Worker
├── context_container: ContextContainer
    └── conversation_history: Arc<Mutex<ConversationHistory>>
        └── entries: Vec<ConversationEntry>
            ├── user: Option<UserMessage>
            └── assistant: Option<AssistantMessage>
```

## Core Types

### ContextContainer

Thread-safe container for all contextual information.

```rust
pub struct ContextContainer {
    pub conversation_history: Arc<Mutex<ConversationHistory>>,
}
```

**Location**: `crates/chat-cli/src/agent_env/context_container/context_container.rs`

### ConversationHistory

Manages conversation as alternating messages.

```rust
pub struct ConversationHistory {
    entries: Vec<ConversationEntry>,
}

impl ConversationHistory {
    pub fn new() -> Self
    pub fn push_input_message(&mut self, content: String)
    pub fn push_assistant_message(&mut self, assistant: AssistantMessage)
    pub fn get_entries(&self) -> &[ConversationEntry]
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
}
```

**Location**: `crates/chat-cli/src/agent_env/context_container/conversation_history.rs`

### ConversationEntry

Single message in conversation (user or assistant).

```rust
pub struct ConversationEntry {
    pub user: Option<UserMessage>,
    pub assistant: Option<AssistantMessage>,
}
```

**Location**: `crates/chat-cli/src/agent_env/context_container/conversation_entry.rs`

## Usage Pattern

### Single Turn

```rust
// Stage user message
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_input_message("What is 2+2?".to_string());

// Run agent loop (reads from context, adds response to context)
let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
job.wait().await?;

// History now contains: [user message, assistant response]
```

### Multi-Turn Conversation

```rust
for prompt in ["Hello", "How are you?", "Goodbye"] {
    worker.context_container
        .conversation_history
        .lock()
        .unwrap()
        .push_input_message(prompt.to_string());
    
    let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
    job.wait().await?;
}

// History contains 6 entries: 3 user + 3 assistant
```

### Inspecting History

```rust
let history = worker.context_container
    .conversation_history
    .lock()
    .unwrap();

for entry in history.get_entries() {
    if let Some(user) = &entry.user {
        println!("User: {}", user.prompt().unwrap_or(""));
    }
    if let Some(assistant) = &entry.assistant {
        println!("Assistant: {}", assistant.content());
    }
}
```

## Design Principles

### Alternating Message Pattern

Messages stored individually (not as pairs) to match LLM API patterns:
- Bedrock: Flat list with roles
- CodeWhisperer: Alternating user/assistant objects

### Thread Safety

Uses `Arc<Mutex<>>` for interior mutability:
- Safe across threads
- Lock must be acquired/released properly
- Never hold lock across `await` points

### Message Types

Reuses existing types from `cli::chat::message`:
- `UserMessage`: Prompt, tool results, cancelled tools
- `AssistantMessage`: Response or tool use

## Integration with AgentLoop

1. **Before execution**: Caller stages user message in context
2. **During execution**: AgentLoop reads last user message from context
3. **After execution**: AgentLoop adds assistant response to context

```rust
// Caller responsibility
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_input_message(prompt);

// AgentLoop reads from context
let prompt = history.get_entries().last()?.user?.prompt()?;

// AgentLoop adds response
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_assistant_message(response);
```

## Future Enhancements

- Tool result support in conversation flow
- Metadata tracking (timestamps, token counts)
- History trimming/summarization
- Sticky context files
- Serialization for persistence
- Helper methods on Worker for ergonomics
- Lock-free alternatives
- Conversation branching/checkpointing
