# ContextContainer Design

## Goal

Add a `ContextContainer` to the `Worker` struct that manages conversation history and other contextual information. The container will initially hold conversation history, with room for future expansion.

## Key Design Principle

**Messages are pushed individually as they occur**, creating an alternating chain:

```rust
// New pattern - alternating messages
worker.context_container.conversation_history.push_input_message("hello");
session.run_agent_loop(worker, LoopInput {}, ...);
// Inside agent loop:
worker.context_container.conversation_history.push_assistant_message(response);

// Old pattern (being replaced)
session.run_agent_loop(worker, LoopInput { prompt: "hello" }, ...);
```

## Architecture Overview

```
Worker
├── id: Uuid
├── name: String
├── context_container: ContextContainer  // NEW - Most critical data
│   └── conversation_history: ConversationHistory  // NEW
│       └── entries: Vec<ConversationEntry>
│           ├── user: Option<UserMessage>
│           └── assistant: Option<AssistantMessage>
├── model_provider: Arc<dyn ModelProvider>
├── state: Arc<Mutex<WorkerStates>>
└── last_failure: Arc<Mutex<Option<String>>>
```

## Core Types

### ConversationEntry

Simplified version of `HistoryEntry` with optional fields to support alternating message pattern:

```rust
pub struct ConversationEntry {
    pub user: Option<UserMessage>,
    pub assistant: Option<AssistantMessage>,
}
```

**Rationale**: 
- Both fields optional to support alternating message chains (matches Bedrock/CodeWhisperer APIs)
- Conversion to Q CLI Database Storage format (paired entries) will be handled separately
- Allows natural representation of conversation as sequence of individual messages

### UserMessage

Reuse existing type from `crates/chat-cli/src/cli/chat/conversation.rs`:

```rust
// Already exists - we'll import it
pub enum UserMessage {
    Prompt { content: String, timestamp: Option<DateTime<FixedOffset>> },
    ToolUseResults { results: Vec<ToolUseResult>, timestamp: Option<DateTime<FixedOffset>> },
    // ... other variants
}
```

### AssistantMessage

Reuse existing type from `crates/chat-cli/src/cli/chat/conversation.rs`:

```rust
// Already exists - we'll import it
pub struct AssistantMessage {
    content: String,
    tool_uses: Option<Vec<ToolUse>>,
    // ... other fields
}
```

### ConversationHistory

New lightweight wrapper managing conversation entries as alternating messages:

```rust
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
        self.entries.push(ConversationEntry {
            user: Some(UserMessage::new_prompt(content, None)),
            assistant: None,
        });
    }

    /// Add an assistant response to the conversation
    pub fn push_assistant_message(&mut self, assistant: AssistantMessage) {
        self.entries.push(ConversationEntry {
            user: None,
            assistant: Some(assistant),
        });
    }

    /// Get all conversation entries
    pub fn get_entries(&self) -> &[ConversationEntry] {
        &self.entries
    }
}
```

**Rationale**:
- Entries stored as alternating messages (user-only or assistant-only)
- Matches Bedrock and CodeWhisperer API patterns
- Conversion to Q CLI Database Storage format (paired entries) handled separately when needed
- Simpler API - no staging/commit pattern needed

### ContextContainer

Container for all contextual information:

```rust
pub struct ContextContainer {
    pub conversation_history: ConversationHistory,
    // Future expansion:
    // pub sticky_context: Vec<ContextFile>,
    // pub tool_results: ToolResultCache,
    // pub session_metadata: SessionMetadata,
}

impl ContextContainer {
    pub fn new() -> Self {
        Self {
            conversation_history: ConversationHistory::new(),
        }
    }
}
```

## Integration Points

### 1. Worker Struct

Add `context_container` field after `name`:

```rust
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub context_container: ContextContainer,  // NEW - Most critical data
    pub model_provider: Arc<dyn ModelProvider>,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}

impl Worker {
    pub fn new(name: String, model_provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            context_container: ContextContainer::new(),  // NEW
            model_provider,
            state: Arc::new(Mutex::new(WorkerStates::Inactive)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }
}
```

### 2. AgentLoopInput

Remove `prompt` field since it's now in the worker's context:

```rust
// Before
pub struct AgentLoopInput {
    pub prompt: String,
}

// After
pub struct AgentLoopInput {
    // Empty for now - may add configuration options later
}
```

### 3. AgentLoop Task

Modify to read prompt from worker's context and add response back:

```rust
impl AgentLoop {
    async fn query_llm(&self) -> Result<ModelResponse, eyre::Error> {
        self.check_cancellation()?;
        
        // Get last entry (should be user input)
        let history = self.worker.context_container
            .conversation_history
            .lock()
            .unwrap();
        
        let last_entry = history.get_entries().last()
            .ok_or_else(|| eyre::eyre!("No messages in history"))?;
        
        let prompt = last_entry.user.as_ref()
            .ok_or_else(|| eyre::eyre!("Last entry is not a user message"))?
            .content()
            .to_string();
        
        drop(history);  // Release lock
        
        let request = ModelRequest { prompt };
        
        // ... rest of implementation
    }
    
    async fn run(&self) -> Result<(), eyre::Error> {
        // ... existing setup code ...
        
        let response = self.query_llm().await?;
        
        // Add assistant response to history
        let assistant_message = AssistantMessage {
            content: response.content,
            tool_uses: if response.tool_requests.is_empty() {
                None
            } else {
                Some(response.tool_requests)
            },
        };
        
        self.worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_assistant_message(assistant_message);
        
        // ... rest of implementation
    }
}
```

### 4. Session Usage Pattern

Update how agent loops are invoked:

```rust
// Before
let input = AgentLoopInput {
    prompt: "Hello, world!".to_string(),
};
let job = session.run_agent_loop(worker, input, ui_interface)?;

// After
worker.context_container
    .conversation_history
    .push_input_message("Hello, world!".to_string());
let input = AgentLoopInput {};
let job = session.run_agent_loop(worker, input, ui_interface)?;
```

## Design Decisions

### 1. Why Not Reuse ConversationState Directly?

**Reasons**:
- Heavy dependencies (Os, Agents, ToolManager, Database)
- Side effects (automatic persistence)
- Not thread-safe (requires `&mut self`)
- Async initialization
- Overkill for initial implementation

**Approach**: Create minimal abstractions using existing message types as building blocks.

### 2. Why Alternating Message Pattern?

Matches both Bedrock and CodeWhisperer API patterns:
- **Bedrock**: Flat list of messages with roles (User/Assistant)
- **CodeWhisperer**: Alternating user/assistant objects in history array

This makes conversion to API formats straightforward - just filter and map entries.

### 3. Why Optional Fields in ConversationEntry?

Allows storing individual messages rather than requiring pairs:
- `{user: Some(...), assistant: None}` - User input
- `{user: None, assistant: Some(...)}` - Assistant response
- Conversion to Q CLI Database Storage format (paired entries) handled separately when needed

### 4. Why "input_message" Instead of "user_message"?

Workers can be created and spawned by other workers, not just by users. The term "input" is more generic and accurate for worker-to-worker communication.

### 5. Why Vec Instead of VecDeque?

- Simpler for initial implementation
- No need for front-popping yet
- Can switch to VecDeque later if needed for history trimming

### 6. Why Not Store Metadata?

Keeping it minimal for now. Can add later:
- Request metadata (latency, token counts)
- Timestamps
- Model information

### 7. Database Persistence Strategy

Conversion to Q CLI Database Storage format (paired `HistoryEntry` objects) will be implemented separately:
- Internal representation: Alternating messages (matches APIs)
- Database format: Paired entries (matches existing storage)
- Conversion happens at persistence boundary

### 8. Mutability Concerns

`Worker` is wrapped in `Arc<Worker>`, but `context_container` needs mutation. Options:

**Option A**: Make `context_container` use interior mutability:
```rust
pub struct Worker {
    // ... other fields ...
    pub context_container: Arc<Mutex<ContextContainer>>,
}
```

**Option B**: Make Worker methods take `&mut self` where needed (breaks current Arc pattern)

**Recommendation**: Option A - use `Arc<Mutex<ContextContainer>>` for thread-safe mutation.

## File Structure

New files to create:

```
crates/chat-cli/src/agent_env/
├── context_container/
│   ├── mod.rs                      # Module exports
│   ├── context_container.rs        # ContextContainer struct
│   ├── conversation_history.rs     # ConversationHistory struct
│   └── conversation_entry.rs       # ConversationEntry struct
```

Files to modify:

```
crates/chat-cli/src/agent_env/
├── mod.rs                          # Add context_container module export
├── worker.rs                       # Add context_container field after name
├── worker_tasks/agent_loop.rs      # Remove prompt from input, use context
└── demo/init.rs                    # Update demo to use new pattern
```

**Note**: Renamed from `context/` to `context_container/` to avoid potential naming collisions with other context-related modules.

## Migration Path

1. Create new context types (no breaking changes)
2. Add `context_container` to Worker (breaking change)
3. Update AgentLoop to read from context (breaking change)
4. Update demo to use new pattern
5. Remove `prompt` from AgentLoopInput (breaking change)

## Future Expansion

The `ContextContainer` is designed to accommodate:

- **Sticky context files**: Files that persist across turns

## Open Questions

1. **Should ConversationHistory be cloneable?** Probably yes, for checkpointing/branching
2. **Max history length?** Not enforced initially, add later if needed
3. **Serialization?** Not needed initially (no persistence), add Serde derives later
4. **Error handling for unstaged messages?** Currently using debug_assert, could return Result
5. **Thread safety guarantees?** Using Arc<Mutex<>> for now, could explore lock-free alternatives

## Non-Goals (For This Phase)

- Database persistence
- History summarization/compaction
- Tool result handling
- Context file management
- Multi-agent context sharing
- Undo/redo functionality
