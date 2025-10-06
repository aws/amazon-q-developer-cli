# WorkerTask System

## Overview

The **WorkerTask** trait defines the interface for executable work units. It abstracts different types of tasks that can be performed by a Worker, enabling the same execution framework to handle agent loops, commands, orchestration, and custom operations.

## WorkerTask Trait

**File**: `crates/chat-cli/src/agent_env/worker_task.rs`

### Definition

```rust
#[async_trait::async_trait]
pub trait WorkerTask: Send + Sync {
    fn get_worker(&self) -> &Worker;
    async fn run(&self) -> Result<(), eyre::Error>;
}
```

### Methods

- **get_worker()**: Returns reference to the Worker executing this task
- **run()**: Async method that executes the task logic

### Constraints

- **Send + Sync**: Task can be safely moved between threads and accessed concurrently
- **async**: Task execution is asynchronous
- **Result**: Task can succeed or fail with error

## Task Implementations

### AgentLoop

**File**: `crates/chat-cli/src/agent_env/worker_tasks/agent_loop.rs`

The main agent conversation loop implementation.

#### Structure

**Location**: `agent_loop.rs` lines 10-24

```rust
pub struct AgentLoopInput {
    pub prompt: String,
}

pub struct AgentLoop {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    input: AgentLoopInput,
    host_interface: Arc<dyn WorkerToHostInterface>,
}
```

#### Constructor

**Location**: `agent_loop.rs` lines 26-38

```rust
pub fn new(
    worker: Arc<Worker>,
    input: AgentLoopInput,
    host_interface: Arc<dyn WorkerToHostInterface>,
    cancellation_token: CancellationToken,
) -> Self
```

#### Key Methods

**Cancellation Check**

**Location**: `agent_loop.rs` lines 40-47

```rust
fn check_cancellation(&self) -> Result<(), eyre::Error>
```

Checks if task was cancelled and returns error if so.

**LLM Query**

**Location**: `agent_loop.rs` lines 49-88

```rust
async fn query_llm(&self) -> Result<ModelResponse, eyre::Error>
```

Queries the LLM:
1. Checks cancellation
2. Creates ModelRequest with prompt
3. Sets state to Requesting
4. Calls model provider with callbacks:
   - `when_receiving_begin`: Sets state to Receiving
   - `when_received`: Forwards chunks to UI
5. Handles errors (sets failure state)
6. Returns response

**Task Execution**

**Location**: `agent_loop.rs` lines 93-119

```rust
async fn run(&self) -> Result<(), eyre::Error>
```

Main execution flow:
1. Logs start time
2. Checks cancellation
3. Clears previous failure
4. Sets state to Working
5. Queries LLM
6. Logs tool requests if any
7. Sets state to Inactive
8. Logs completion time

#### State Flow

```
Working → Requesting → Receiving → Inactive
                                 ↓
                           InactiveFailed (on error)
```

### WorkerProtoLoop (Demo)

**File**: `crates/chat-cli/src/agent_env/demo/proto_loop.rs`

Demonstration task showing complete agent flow with tool confirmation.

#### Structure

**Location**: `proto_loop.rs` lines 10-24

```rust
pub struct WorkerInput {
    pub prompt: String,
    pub color: Color,
}

pub struct WorkerProtoLoop {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    input: WorkerInput,
    host_interface: Arc<dyn WorkerToHostInterface>,
}
```

#### Execution Flow

**Location**: `proto_loop.rs` lines 49-119

The demo task demonstrates:
1. **Working**: Prepares request
2. **Requesting**: Sends to LLM
3. **Receiving**: Streams response
4. **Waiting**: Requests tool confirmation from user
5. **Inactive**: Completes successfully

## Creating Custom Tasks

To create a custom task:

```rust
pub struct MyCustomTask {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    // ... custom fields
}

#[async_trait::async_trait]
impl WorkerTask for MyCustomTask {
    fn get_worker(&self) -> &Worker {
        &self.worker
    }

    async fn run(&self) -> Result<(), eyre::Error> {
        // 1. Check cancellation periodically
        if self.cancellation_token.is_cancelled() {
            return Err(eyre::eyre!("Cancelled"));
        }

        // 2. Update worker state
        self.worker.set_state(WorkerStates::Working, &*self.host_interface);

        // 3. Do work...

        // 4. Handle errors
        if let Err(e) = some_operation() {
            self.worker.set_failure(e.to_string());
            self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
            return Err(e);
        }

        // 5. Complete successfully
        self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
        Ok(())
    }
}
```

## Task Design Patterns

### Cancellation Handling

Always check cancellation token at key points:
```rust
if self.cancellation_token.is_cancelled() {
    return Err(eyre::eyre!("Cancelled"));
}
```

### State Management

Update state for UI feedback:
```rust
self.worker.set_state(WorkerStates::Working, &*self.host_interface);
```

### Error Handling

Record failures before returning error:
```rust
self.worker.set_failure(error_msg);
self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
```

### Streaming Output

Use interface to send chunks:
```rust
self.host_interface.response_chunk_received(
    self.worker.id,
    ModelResponseChunk::AssistantMessage(text),
);
```

### User Interaction

Request input with cancellation support:
```rust
let response = self.host_interface.get_tool_confirmation(
    self.worker.id,
    "Confirm action?".to_string(),
    self.cancellation_token.clone(),
).await?;
```

## Future Task Types

Planned task implementations:
- **CompactTask**: `/compact` command implementation
- **OrchestratorLoop**: Multi-agent coordination
- **ToolExecutionTask**: Isolated tool execution
- **ConversationTask**: Multi-turn conversation management

## Design Notes

- Tasks are self-contained and reusable
- Tasks don't handle UI directly (use interface)
- Tasks are cancellable at any point
- Tasks manage worker state transitions
- Tasks can be composed (task launching sub-tasks)
- Tasks are async-first (no blocking operations)
