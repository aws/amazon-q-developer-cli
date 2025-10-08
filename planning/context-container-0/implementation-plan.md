# ContextContainer Implementation Plan

## Overview

This plan implements the ContextContainer design in phases, minimizing breaking changes and ensuring each step is testable.

## Phase 1: Create Context Types (No Breaking Changes)

### Step 1.1: Create module structure

```bash
mkdir -p crates/chat-cli/src/agent_env/context_container
```

**Files to create**:
- `crates/chat-cli/src/agent_env/context_container/mod.rs`
- `crates/chat-cli/src/agent_env/context_container/conversation_entry.rs`
- `crates/chat-cli/src/agent_env/context_container/conversation_history.rs`
- `crates/chat-cli/src/agent_env/context_container/context_container.rs`

### Step 1.2: Implement ConversationEntry

**File**: `crates/chat-cli/src/agent_env/context_container/conversation_entry.rs`

```rust
use crate::cli::chat::conversation::{UserMessage, AssistantMessage};

/// A single message in the conversation (either user input or assistant response)
#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub user: Option<UserMessage>,
    pub assistant: Option<AssistantMessage>,
}

impl ConversationEntry {
    pub fn new_user(user: UserMessage) -> Self {
        Self { user: Some(user), assistant: None }
    }
    
    pub fn new_assistant(assistant: AssistantMessage) -> Self {
        Self { user: None, assistant: Some(assistant) }
    }
}
```

**Dependencies**: Import existing types from `crates/chat-cli/src/cli/chat/conversation.rs`

### Step 1.3: Implement ConversationHistory

**File**: `crates/chat-cli/src/agent_env/context_container/conversation_history.rs`

```rust
use crate::cli::chat::conversation::UserMessage;
use super::conversation_entry::ConversationEntry;

/// Manages conversation history as alternating messages
#[derive(Debug, Clone)]
pub struct ConversationHistory {
    entries: Vec<ConversationEntry>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an input message to the conversation
    pub fn push_input_message(&mut self, content: String) {
        self.entries.push(ConversationEntry::new_user(
            UserMessage::new_prompt(content, None)
        ));
    }

    /// Add an assistant response to the conversation
    pub fn push_assistant_message(&mut self, assistant: crate::cli::chat::conversation::AssistantMessage) {
        self.entries.push(ConversationEntry::new_assistant(assistant));
    }

    /// Get all conversation entries
    pub fn get_entries(&self) -> &[ConversationEntry] {
        &self.entries
    }

    /// Get number of entries (individual messages, not turns)
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ConversationHistory {
    fn default() -> Self {
        Self::new()
    }
}
```

### Step 1.4: Implement ContextContainer

**File**: `crates/chat-cli/src/agent_env/context_container/context_container.rs`

```rust
use std::sync::{Arc, Mutex};
use super::conversation_history::ConversationHistory;

/// Container for all contextual information available to a worker
#[derive(Debug, Clone)]
pub struct ContextContainer {
    pub conversation_history: Arc<Mutex<ConversationHistory>>,
    // Future fields:
    // pub sticky_context: Arc<Mutex<Vec<ContextFile>>>,
    // pub tool_results: Arc<Mutex<ToolResultCache>>,
}

impl ContextContainer {
    pub fn new() -> Self {
        Self {
            conversation_history: Arc::new(Mutex::new(ConversationHistory::new())),
        }
    }
}

impl Default for ContextContainer {
    fn default() -> Self {
        Self::new()
    }
}
```

**Note**: Using `Arc<Mutex<>>` for interior mutability since Worker is wrapped in Arc.

### Step 1.5: Create module exports

**File**: `crates/chat-cli/src/agent_env/context_container/mod.rs`

```rust
mod conversation_entry;
mod conversation_history;
mod context_container;

pub use conversation_entry::ConversationEntry;
pub use conversation_history::ConversationHistory;
pub use context_container::ContextContainer;
```

### Step 1.6: Export from agent_env module

**File**: `crates/chat-cli/src/agent_env/mod.rs`

Add to existing exports:

```rust
// Add this line
mod context_container;

// Add to pub use section
pub use context_container::{ContextContainer, ConversationHistory, ConversationEntry};
```

### Step 1.7: Verify compilation

Use build command template as is!
```bash
cd /Volumes/workplace/q-cli-dmay && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check  --package chat-cli > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"
```


**Expected**: No errors (new types created but not used yet)

## Phase 2: Add ContextContainer to Worker (Breaking Change)

### Step 2.1: Update Worker struct

**File**: `crates/chat-cli/src/agent_env/worker.rs`

```rust
use super::context_container::ContextContainer;  // Add import

pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub context_container: ContextContainer,  // Add after name - most critical data
    pub model_provider: Arc<dyn ModelProvider>,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}

impl Worker {
    pub fn new(name: String, model_provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            context_container: ContextContainer::new(),  // Initialize
            model_provider,
            state: Arc::new(Mutex::new(WorkerStates::Inactive)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }
}
```

### Step 2.2: Verify compilation

Use build command template as is!
```bash
cd /path/to/package && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check --package chat-cli > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"
```

**Expected**: Compilation succeeds (Worker now has context_container)

## Phase 3: Update AgentLoop to Use Context (Breaking Change)

### Step 3.1: Keep AgentLoopInput with prompt temporarily

**File**: `crates/chat-cli/src/agent_env/worker_tasks/agent_loop.rs`

Keep the existing structure for now to avoid breaking demo:

```rust
pub struct AgentLoopInput {
    pub prompt: String,  // Keep for now
}
```

### Step 3.2: Update AgentLoop to read from context

```rust
impl AgentLoop {
    pub fn new(
        worker: Arc<Worker>,
        input: AgentLoopInput,
        host_interface: Arc<dyn WorkerToHostInterface>,
        cancellation_token: CancellationToken,
    ) -> Self {
        // Add the prompt to worker's context
        worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message(input.prompt.clone());
        
        Self {
            worker,
            host_interface,
            cancellation_token,
        }
    }
}
```

### Step 3.3: Update query_llm to read from context

```rust
impl AgentLoop {
    async fn query_llm(&self) -> Result<ModelResponse, eyre::Error> {
        self.check_cancellation()?;
        
        // Get last entry from worker's context
        let history = self.worker.context_container
            .conversation_history
            .lock()
            .unwrap();
        
        let last_entry = history.get_entries().last()
            .ok_or_else(|| eyre::eyre!("No messages in history"))?;
        
        let prompt = match &last_entry.user {
            Some(user_msg) => match user_msg {
                crate::cli::chat::conversation::UserMessage::Prompt { content, .. } => {
                    content.clone()
                },
                _ => return Err(eyre::eyre!("Expected prompt message")),
            },
            None => return Err(eyre::eyre!("Last entry is not a user message")),
        };
        
        drop(history);  // Release lock before async operation
        
        let request = ModelRequest { prompt };
        
        self.worker.set_state(WorkerStates::Requesting, &*self.host_interface);
        
        // ... rest of existing implementation
    }
}
```

### Step 3.4: Update run() to add assistant response

```rust
impl AgentLoop {
    async fn run(&self) -> Result<(), eyre::Error> {
        let start = std::time::Instant::now();
        info!(worker_id = %self.worker.id, "Agent loop started");

        self.check_cancellation()?;
        self.worker.set_failure("".to_string());
        self.worker.set_state(WorkerStates::Working, &*self.host_interface);

        let response = self.query_llm().await?;

        // Create assistant message
        let assistant_message = crate::cli::chat::conversation::AssistantMessage::new(
            response.content.clone(),
            if response.tool_requests.is_empty() {
                None
            } else {
                Some(response.tool_requests.clone())
            },
        );

        // Add assistant response to history
        self.worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_assistant_message(assistant_message);

        if !response.tool_requests.is_empty() {
            info!(
                worker_id = %self.worker.id,
                tool_count = response.tool_requests.len(),
                "Tool requests accumulated"
            );
        }

        self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
        
        let elapsed = start.elapsed();
        info!(
            worker_id = %self.worker.id,
            duration_ms = elapsed.as_millis(),
            "Agent loop completed"
        );

        Ok(())
    }
}
```

**Note**: Need to check if `AssistantMessage::new()` exists or create appropriate constructor.

### Step 3.5: Verify compilation

Use build command template as is!
```bash
cd /path/to/package && echo "Build started at: $(date)" && echo "Output file: /tmp/build_output_$(date +%s).txt" && cargo check --package chat-cli > /tmp/build_output_$(date +%s).txt 2>&1 && echo "Build completed"
```

**Expected**: May have errors about AssistantMessage constructor - will need to check actual API.

## Phase 4: Update Demo (Breaking Change)

### Step 4.1: Update demo initialization

**File**: `crates/chat-cli/src/agent_env/demo/init.rs`

Current pattern:
```rust
let input = AgentLoopInput {
    prompt: "Hello, world!".to_string(),
};
session.run_agent_loop(worker, input, ui_interface)?;
```

Keep the same for now since we're still accepting prompt in AgentLoopInput.

### Step 4.2: Test the demo

```bash
cargo run --bin chat_cli -- chat --demo
```

**Expected**: Demo should work with new context system (prompt flows through context internally).

## Phase 5: Remove Prompt from AgentLoopInput (Optional Breaking Change)

This step can be deferred to a later phase if we want to maintain backward compatibility.

### Step 5.1: Empty AgentLoopInput

```rust
pub struct AgentLoopInput {
    // Empty - all context comes from Worker
}
```

### Step 5.2: Update AgentLoop constructor

```rust
impl AgentLoop {
    pub fn new(
        worker: Arc<Worker>,
        _input: AgentLoopInput,  // Unused now
        host_interface: Arc<dyn WorkerToHostInterface>,
        cancellation_token: CancellationToken,
    ) -> Self {
        // Don't stage prompt here anymore - caller must do it
        Self {
            worker,
            host_interface,
            cancellation_token,
        }
    }
}
```

### Step 5.3: Update demo to push prompt before creating task

```rust
// Stage prompt in worker's context
worker.context_container
    .conversation_history
    .lock()
    .unwrap()
    .push_input_message("Hello, world!".to_string());

// Create task with empty input
let input = AgentLoopInput {};
session.run_agent_loop(worker, input, ui_interface)?;
```

## Testing Strategy

### Unit Tests

Create `crates/chat-cli/src/agent_env/context_container/tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_history_alternating_messages() {
        let mut history = ConversationHistory::new();
        
        assert!(history.is_empty());
        
        // Add user message
        history.push_input_message("Hello".to_string());
        assert_eq!(history.len(), 1);
        
        // Add assistant response
        let assistant = AssistantMessage::new("Hi there".to_string(), None);
        history.push_assistant_message(assistant);
        assert_eq!(history.len(), 2);
        
        // Verify entries
        let entries = history.get_entries();
        assert!(entries[0].user.is_some());
        assert!(entries[0].assistant.is_none());
        assert!(entries[1].user.is_none());
        assert!(entries[1].assistant.is_some());
    }

    #[test]
    fn test_multiple_turns() {
        let mut history = ConversationHistory::new();
        
        for i in 0..3 {
            history.push_input_message(format!("Message {}", i));
            let assistant = AssistantMessage::new(format!("Response {}", i), None);
            history.push_assistant_message(assistant);
        }
        
        assert_eq!(history.len(), 6);  // 3 user + 3 assistant
    }

    #[test]
    fn test_context_container_creation() {
        let container = ContextContainer::new();
        let history = container.conversation_history.lock().unwrap();
        assert!(history.is_empty());
    }
}
```

### Integration Tests

Test with actual AgentLoop:

```rust
#[tokio::test]
async fn test_agent_loop_with_context() {
    // Setup session and worker
    let session = Session::new(vec![model_provider]);
    let worker = session.build_worker();
    
    // Stage prompt in context
    worker.context_container
        .conversation_history
        .lock()
        .unwrap()
        .push_input_message("Test prompt".to_string());
    
    // Run agent loop
    let input = AgentLoopInput {};
    let job = session.run_agent_loop(worker.clone(), input, ui_interface)?;
    job.wait().await?;
    
    // Verify history was updated
    let history = worker.context_container
        .conversation_history
        .lock()
        .unwrap();
    assert_eq!(history.len(), 1);
    assert!(!history.has_staged_message());
}
```

## Rollout Checklist

- [ ] Phase 1: Create context types
  - [ ] Step 1.1: Create module structure
  - [ ] Step 1.2: Implement ConversationEntry
  - [ ] Step 1.3: Implement ConversationHistory
  - [ ] Step 1.4: Implement ContextContainer
  - [ ] Step 1.5: Create module exports
  - [ ] Step 1.6: Export from agent_env
  - [ ] Step 1.7: Verify compilation

- [ ] Phase 2: Add to Worker
  - [ ] Step 2.1: Update Worker struct
  - [ ] Step 2.2: Verify compilation

- [ ] Phase 3: Update AgentLoop
  - [ ] Step 3.1: Keep AgentLoopInput with prompt
  - [ ] Step 3.2: Stage prompt in constructor
  - [ ] Step 3.3: Read from context in query_llm
  - [ ] Step 3.4: Commit turn in run()
  - [ ] Step 3.5: Verify compilation

- [ ] Phase 4: Update Demo
  - [ ] Step 4.1: Update demo initialization (if needed)
  - [ ] Step 4.2: Test demo

- [ ] Phase 5: Remove prompt from input (optional)
  - [ ] Step 5.1: Empty AgentLoopInput
  - [ ] Step 5.2: Update constructor
  - [ ] Step 5.3: Update demo

- [ ] Testing
  - [ ] Write unit tests
  - [ ] Write integration tests
  - [ ] Manual testing with demo

## Known Issues and Workarounds

### Issue 1: AssistantMessage Constructor

The existing `AssistantMessage` type may not have a simple constructor. Need to check actual API.

**Workaround**: Use whatever constructor exists, or create a helper function.

### Issue 2: UserMessage Variants

`UserMessage` is an enum with multiple variants. Need to handle pattern matching properly.

**Workaround**: For now, only support `Prompt` variant. Add tool results later.

### Issue 3: Arc<Mutex<>> Ergonomics

Lots of `.lock().unwrap()` calls make code verbose.

**Workaround**: Consider adding helper methods on Worker:
```rust
impl Worker {
    pub fn push_input_message(&self, content: String) {
        self.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message(content);
    }
}
```

### Issue 4: Lock Holding Across Await

Must be careful not to hold locks across await points.

**Workaround**: Always drop locks before async operations:
```rust
let prompt = {
    let history = self.worker.context_container.conversation_history.lock().unwrap();
    history.get_next_message().unwrap().clone()
};  // Lock dropped here
// Now safe to await
```

## Future Enhancements

After this implementation is complete:

1. **Add tool result support** to ConversationHistory
2. **Add metadata tracking** (timestamps, token counts)
3. **Implement history trimming** (max length, summarization)
4. **Add serialization** for persistence
5. **Add sticky context** to ContextContainer
6. **Add helper methods** on Worker for ergonomics
7. **Consider lock-free alternatives** to Arc<Mutex<>>
8. **Add conversation branching** (checkpoints, undo/redo)

## Dependencies to Check

Before starting implementation, verify these types exist and are accessible:

- [ ] `crate::cli::chat::conversation::UserMessage`
- [ ] `crate::cli::chat::conversation::AssistantMessage`
- [ ] `crate::cli::chat::conversation::ToolUse`
- [ ] `crate::cli::chat::conversation::ToolUseResult`

If any are missing or have different APIs, adjust the implementation accordingly.
