# ContextContainer API Reference

## Module: `agent_env::context_container`

### Types

#### `ConversationEntry`

A single message in the conversation (either user input or assistant response).

```rust
pub struct ConversationEntry {
    pub user: Option<UserMessage>,
    pub assistant: Option<AssistantMessage>,
}
```

**Methods**:
```rust
impl ConversationEntry {
    pub fn new_user(user: UserMessage) -> Self
    pub fn new_assistant(assistant: AssistantMessage) -> Self
}
```

**Traits**: `Debug`, `Clone`

**Rationale**: Optional fields allow storing individual messages in alternating pattern, matching Bedrock and CodeWhisperer API structures.

---

#### `ConversationHistory`

Manages conversation history as alternating messages.

```rust
pub struct ConversationHistory {
    entries: Vec<ConversationEntry>,
}
```

**Methods**:

```rust
impl ConversationHistory {
    /// Create a new empty conversation history
    pub fn new() -> Self
    
    /// Add an input message to the conversation
    pub fn push_input_message(&mut self, content: String)
    
    /// Add an assistant response to the conversation
    pub fn push_assistant_message(&mut self, assistant: AssistantMessage)
    
    /// Get all conversation entries
    pub fn get_entries(&self) -> &[ConversationEntry]
    
    /// Get number of entries (individual messages, not turns)
    pub fn len(&self) -> usize
    
    /// Check if history is empty
    pub fn is_empty(&self) -> bool
}
```

**Traits**: `Debug`, `Clone`, `Default`

**Usage Example**:

```rust
let mut history = ConversationHistory::new();

// Add user message
history.push_input_message("Hello, AI!".to_string());
assert_eq!(history.len(), 1);

// Add assistant response
let assistant_msg = AssistantMessage::new("Hello, human!".to_string(), None);
history.push_assistant_message(assistant_msg);
assert_eq!(history.len(), 2);

// Entries are alternating
let entries = history.get_entries();
assert!(entries[0].user.is_some() && entries[0].assistant.is_none());
assert!(entries[1].user.is_none() && entries[1].assistant.is_some());
```

---

#### `ContextContainer`

Container for all contextual information available to a worker.

```rust
pub struct ContextContainer {
    pub conversation_history: Arc<Mutex<ConversationHistory>>,
}
```

**Methods**:

```rust
impl ContextContainer {
    /// Create a new context container with empty history
    pub fn new() -> Self
}
```

**Traits**: `Debug`, `Clone`, `Default`

**Usage Example**:

```rust
let container = ContextContainer::new();

// Access conversation history
{
    let mut history = container.conversation_history.lock().unwrap();
    history.push_user_message("Test".to_string());
}

// History is accessible from multiple references
let container_clone = container.clone();
{
    let history = container_clone.conversation_history.lock().unwrap();
    assert!(history.has_staged_message());
}
```

---

### Integration with Worker

#### Updated Worker Struct

```rust
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub context_container: ContextContainer,  // NEW - Most critical data
    pub model_provider: Arc<dyn ModelProvider>,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}
```

#### Usage Pattern

```rust
// Create worker
let worker = session.build_worker();

// Add input message to worker's context
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_input_message("Hello".to_string());

// Run agent loop (no prompt in input)
let input = AgentLoopInput {};
let job = session.run_agent_loop(worker, input, ui_interface)?;

// Wait for completion (assistant response added inside agent loop)
job.wait().await?;

// Check history
let history = worker.context_container
    .conversation_history
    .lock()
    .unwrap();
assert_eq!(history.len(), 2);  // 1 user + 1 assistant
```

---

### Integration with AgentLoop

#### Updated AgentLoopInput

```rust
pub struct AgentLoopInput {
    // Empty - all context comes from Worker
}
```

#### AgentLoop Implementation Pattern

```rust
impl AgentLoop {
    async fn query_llm(&self) -> Result<ModelResponse, eyre::Error> {
        // Get last entry from worker's context
        let prompt = {
            let history = self.worker.context_container
                .conversation_history
                .lock()
                .unwrap();
            
            let last_entry = history.get_entries().last()
                .ok_or_else(|| eyre::eyre!("No messages in history"))?;
            
            match &last_entry.user {
                Some(UserMessage::Prompt { content, .. }) => content.clone(),
                Some(_) => return Err(eyre::eyre!("Expected prompt message")),
                None => return Err(eyre::eyre!("Last entry is not a user message")),
            }
        };  // Lock dropped here
        
        // Use prompt in LLM request
        let request = ModelRequest { prompt };
        let response = self.worker.model_provider.request(request, ...).await?;
        
        Ok(response)
    }
    
    async fn run(&self) -> Result<(), eyre::Error> {
        // Query LLM
        let response = self.query_llm().await?;
        
        // Create assistant message
        let assistant_message = AssistantMessage::new(
            response.content,
            if response.tool_requests.is_empty() {
                None
            } else {
                Some(response.tool_requests)
            },
        );
        
        // Add assistant response to history
        self.worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_assistant_message(assistant_message);
        
        Ok(())
    }
}
```

---

## Common Patterns

### Pattern 1: Single Turn Conversation

```rust
// Setup
let worker = session.build_worker();

// Add user message
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_input_message("What is 2+2?".to_string());

// Execute (assistant response added inside)
let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
job.wait().await?;

// Check result
let history = worker.context_container.conversation_history.lock().unwrap();
assert_eq!(history.len(), 2);  // 1 user + 1 assistant
let entries = history.get_entries();
println!("User: {:?}", entries[0].user);
println!("Assistant: {:?}", entries[1].assistant);
```

### Pattern 2: Multi-Turn Conversation

```rust
let worker = session.build_worker();

for prompt in ["Hello", "How are you?", "Goodbye"] {
    // Add user message
    worker.context_container
        .conversation_history
        .lock()
        .unwrap()
        .push_input_message(prompt.to_string());
    
    // Execute turn
    let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
    job.wait().await?;
}

// Check full history
let history = worker.context_container.conversation_history.lock().unwrap();
assert_eq!(history.len(), 6);  // 3 user + 3 assistant
```

### Pattern 3: Inspecting History

```rust
let history = worker.context_container.conversation_history.lock().unwrap();

let mut turn = 0;
for (i, entry) in history.get_entries().iter().enumerate() {
    if let Some(user) = &entry.user {
        turn += 1;
        println!("Turn {} - User: {}", turn, user.content());
    }
    if let Some(assistant) = &entry.assistant {
        println!("Turn {} - Assistant: {}", turn, assistant.content());
        if let Some(tools) = assistant.tool_uses() {
            println!("  Tools used: {}", tools.len());
        }
    }
}
```

### Pattern 4: Helper Methods (Future Enhancement)

To improve ergonomics, consider adding helper methods to Worker:

```rust
impl Worker {
    /// Convenience method to push input message
    pub fn push_input_message(&self, content: String) {
        self.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message(content);
    }
    
    /// Get conversation history length
    pub fn conversation_length(&self) -> usize {
        self.context_container
            .conversation_history
            .lock()
            .unwrap()
            .len()
    }
    
    /// Check if ready for next turn
    pub fn has_staged_message(&self) -> bool {
        self.context_container
            .conversation_history
            .lock()
            .unwrap()
            .has_staged_message()
    }
}
```

Usage:
```rust
worker.push_input_message("Hello".to_string());
let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
job.wait().await?;
println!("Conversation has {} turns", worker.conversation_length());
```

---

## Error Handling

### Current Implementation

The current implementation has no panic conditions - messages are simply added to the history as they occur.

### Potential Issues

1. **Empty history**: Calling agent loop with no user message will fail
2. **Wrong message type**: Last entry not being a user message will fail in query_llm

### Future: Result-Based API

Consider adding validation:

```rust
impl ConversationHistory {
    pub fn validate_for_llm_call(&self) -> Result<(), HistoryError> {
        let last_entry = self.entries.last()
            .ok_or(HistoryError::EmptyHistory)?;
        
        if last_entry.user.is_none() {
            return Err(HistoryError::LastEntryNotUserMessage);
        }
        
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("History is empty")]
    EmptyHistory,
    #[error("Last entry is not a user message")]
    LastEntryNotUserMessage,
}
```

---

## Thread Safety

### Mutex Usage

`ContextContainer` uses `Arc<Mutex<ConversationHistory>>` for thread-safe access:

- **Safe**: Multiple threads can hold Arc clones
- **Blocking**: Lock acquisition blocks the thread
- **Poisoning**: Mutex will poison if a thread panics while holding the lock

### Best Practices

1. **Minimize lock duration**: Acquire lock, do work, drop lock immediately
2. **No locks across await**: Never hold a lock across an await point
3. **Clone data if needed**: Clone data out of the lock before async operations

**Good**:
```rust
let prompt = {
    let history = worker.context_container.conversation_history.lock().unwrap();
    history.get_next_message().unwrap().clone()
};  // Lock dropped
let response = llm.query(prompt).await;  // Safe to await
```

**Bad**:
```rust
let history = worker.context_container.conversation_history.lock().unwrap();
let prompt = history.get_next_message().unwrap();
let response = llm.query(prompt).await;  // DEADLOCK RISK - lock held across await
```

---

## Migration Guide

### From Old Pattern

```rust
// OLD: Prompt in input
let input = AgentLoopInput {
    prompt: "Hello".to_string(),
};
let job = session.run_agent_loop(worker, input, ui)?;
```

### To New Pattern

```rust
// NEW: Prompt in context
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_input_message("Hello".to_string());

let input = AgentLoopInput {};
let job = session.run_agent_loop(worker, input, ui)?;
```

### Benefits

1. **Persistent history**: Conversation history maintained across turns
2. **Extensible**: Easy to add more context types
3. **Inspectable**: Can examine history at any time
4. **Shareable**: Context can be cloned/shared between workers (future)

---

## Testing

### Unit Test Example

```rust
#[test]
fn test_conversation_flow() {
    let mut history = ConversationHistory::new();
    
    // Initial state
    assert!(history.is_empty());
    
    // Add user message
    history.push_input_message("Test".to_string());
    assert_eq!(history.len(), 1);
    
    // Add assistant response
    let assistant = AssistantMessage::new("Response".to_string(), None);
    history.push_assistant_message(assistant);
    assert_eq!(history.len(), 2);
    
    // Verify alternating pattern
    let entries = history.get_entries();
    assert!(entries[0].user.is_some() && entries[0].assistant.is_none());
    assert!(entries[1].user.is_none() && entries[1].assistant.is_some());
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_multi_turn_conversation() {
    let session = setup_test_session();
    let worker = session.build_worker();
    
    // Turn 1
    worker.context_container
        .conversation_history
        .lock()
        .unwrap()
        .push_input_message("Hello".to_string());
    let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
    job.wait().await?;
    
    // Turn 2
    worker.context_container
        .conversation_history
        .lock()
        .unwrap()
        .push_input_message("Goodbye".to_string());
    let job = session.run_agent_loop(worker.clone(), AgentLoopInput {}, ui)?;
    job.wait().await?;
    
    // Verify
    let history = worker.context_container.conversation_history.lock().unwrap();
    assert_eq!(history.len(), 4);  // 2 user + 2 assistant
}
```
