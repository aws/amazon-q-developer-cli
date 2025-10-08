# Worker

## Overview

The **Worker** represents a complete AI agent configuration - essentially "agent config + conversation history + LLM access + tools" bundled into a single unit. Each Worker is an independent agent that can execute tasks.

## Implementation

**File**: `crates/chat-cli/src/agent_env/worker.rs`

## Structure

```rust
pub struct Worker {
    pub id: Uuid,
    pub name: String,
    pub model_provider: BedrockConverseStreamModelProvider,
    pub state: Arc<Mutex<WorkerStates>>,
    pub last_failure: Arc<Mutex<Option<String>>>,
}
```

### Fields

- **id**: Unique identifier for the worker (UUID)
- **name**: Human-readable name for identification
- **model_provider**: LLM provider for making requests
- **state**: Thread-safe current state (see WorkerStates below)
- **last_failure**: Thread-safe storage for last error message

## Worker States

**File**: `crates/chat-cli/src/agent_env/worker.rs` (lines 8-16)

```rust
pub enum WorkerStates {
    Inactive,
    Working,
    Requesting,
    Receiving,
    Waiting,
    UsingTool,
    InactiveFailed,
}
```

### State Descriptions

- **Inactive**: Worker is idle, ready for new task
- **Working**: Preparing request or processing data
- **Requesting**: Sending request to LLM provider
- **Receiving**: Streaming response chunks from LLM
- **Waiting**: Waiting for user input or confirmation
- **UsingTool**: Executing a tool
- **InactiveFailed**: Task completed with error

## Key Methods

### Constructor

**Location**: `worker.rs` lines 24-33

```rust
pub fn new(name: String, model_provider: BedrockConverseStreamModelProvider) -> Self
```

Creates a new Worker with:
- Generated UUID
- Provided name and model provider
- Initial state: Inactive
- No failure recorded

### State Management

**Location**: `worker.rs` lines 35-41

```rust
pub fn set_state(&self, new_state: WorkerStates, interface: &dyn WorkerToHostInterface)
```

Updates worker state and notifies UI via interface. Thread-safe.

**Location**: `worker.rs` lines 43-45

```rust
pub fn get_state(&self) -> WorkerStates
```

Returns current state. Thread-safe.

### Error Tracking

**Location**: `worker.rs` lines 47-50

```rust
pub fn set_failure(&self, error: String)
```

Records error message for failed tasks.

**Location**: `worker.rs` lines 52-54

```rust
pub fn get_failure(&self) -> Option<String>
```

Retrieves last error message if any.

## Thread Safety

All mutable state is protected by `Arc<Mutex<T>>`:
- State changes are atomic
- Multiple tasks can safely read/write state
- No data races possible

## Usage Pattern

```rust
// Create worker
let worker = Worker::new(
    "MyAgent".to_string(),
    model_provider,
);

// Update state (notifies UI)
worker.set_state(WorkerStates::Working, &ui_interface);

// Check state
if worker.get_state() == WorkerStates::Inactive {
    // Ready for new task
}

// Record error
worker.set_failure("Connection timeout".to_string());
worker.set_state(WorkerStates::InactiveFailed, &ui_interface);
```

## Design Notes

- Workers are designed to be reusable across multiple tasks
- State transitions always notify UI for real-time feedback
- Error tracking persists across state changes
- Thread-safe design enables concurrent access from multiple tasks
- Currently hardcoded to BedrockConverseStreamModelProvider (will be abstracted)

## Future Enhancements

- Abstract model provider to trait object
- Add conversation history storage
- Add tools provider
- Add configuration parameters
- Add metrics/telemetry
