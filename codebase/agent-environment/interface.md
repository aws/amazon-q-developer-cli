# WorkerToHostInterface

## Overview

The **WorkerToHostInterface** defines the communication contract between Workers and the UI layer. It enables different UI implementations (CLI, web, API) to work with the same core Worker logic by providing business-centric communication methods.

## Implementation

**File**: `crates/chat-cli/src/agent_env/worker_interface.rs`

## Trait Definition

```rust
#[async_trait::async_trait]
pub trait WorkerToHostInterface: Send + Sync {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates);
    fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk);
    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, eyre::Error>;
}
```

## Methods

### State Change Notification

**Location**: `worker_interface.rs` line 9

```rust
fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates);
```

Called when worker transitions between states:
- **worker_id**: Identifies which worker changed state
- **new_state**: The new state (Working, Requesting, Receiving, etc.)

**Purpose**: Enables UI to show real-time status updates

**Example**: CLI shows colored state labels, web UI updates progress bar

### Response Chunk Streaming

**Location**: `worker_interface.rs` line 10

```rust
fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk);
```

Called when LLM response chunk arrives:
- **worker_id**: Identifies which worker received chunk
- **chunk**: The response chunk (text or tool request)

**Purpose**: Enables real-time streaming output to user

**Example**: CLI prints text as it arrives, web UI updates chat bubble

### Tool Confirmation Request

**Location**: `worker_interface.rs` lines 11-16

```rust
async fn get_tool_confirmation(
    &self,
    worker_id: Uuid,
    request: String,
    cancellation_token: CancellationToken,
) -> Result<String, eyre::Error>;
```

Requests user input/confirmation:
- **worker_id**: Identifies which worker needs input
- **request**: The question/prompt for user
- **cancellation_token**: Allows cancelling the wait
- **Returns**: User's response or error if cancelled

**Purpose**: Enables interactive workflows with cancellation support

**Example**: CLI prompts for input with Ctrl+C support, web UI shows modal dialog

## CLI Implementation

**File**: `crates/chat-cli/src/agent_env/demo/cli_interface.rs`

### Structure

**Location**: `cli_interface.rs` lines 11-13

```rust
pub struct CliInterface {
    pub color: Color,
}
```

### State Change Implementation

**Location**: `cli_interface.rs` lines 21-30

```rust
fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
    let state_str = format!("{:?}", new_state);
    println!(
        "{}",
        state_str.color(self.color)
    );
}
```

Prints colored state label to console.

### Response Chunk Implementation

**Location**: `cli_interface.rs` lines 32-40

```rust
fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk) {
    match chunk {
        ModelResponseChunk::AssistantMessage(text) => {
            print!("{}", text.color(self.color));
            std::io::stdout().flush().unwrap();
        }
        ModelResponseChunk::ToolUseRequest { tool_name, parameters } => {
            println!("\n[Tool: {} with {}]", tool_name, parameters);
        }
    }
}
```

Prints text chunks in color, flushes immediately for streaming effect.

### Tool Confirmation Implementation

**Location**: `cli_interface.rs` lines 42-73

```rust
async fn get_tool_confirmation(
    &self,
    worker_id: Uuid,
    request: String,
    cancellation_token: CancellationToken,
) -> Result<String, eyre::Error> {
    println!("\n{}", request.color(self.color));
    
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    
    tokio::spawn(async move {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let _ = tx.send(input.trim().to_string()).await;
    });
    
    tokio::select! {
        Some(input) = rx.recv() => Ok(input),
        _ = cancellation_token.cancelled() => {
            Err(eyre::eyre!("Cancelled"))
        }
    }
}
```

Spawns blocking stdin read in separate task, races with cancellation token.

## Usage Pattern

```rust
// Implement interface
struct MyUI;

#[async_trait::async_trait]
impl WorkerToHostInterface for MyUI {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
        // Update UI state indicator
    }
    
    fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk) {
        // Stream text to output
    }
    
    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, eyre::Error> {
        // Show prompt, wait for input with cancellation
    }
}

// Use with worker
let ui = Arc::new(MyUI);
worker.set_state(WorkerStates::Working, &*ui);
ui.response_chunk_received(worker.id, chunk);
let response = ui.get_tool_confirmation(worker.id, "Confirm?", token).await?;
```

## Design Principles

### Business-Centric

Interface methods represent business operations, not UI primitives:
- "Worker changed state" not "Update label text"
- "Response chunk received" not "Append to text buffer"
- "Get tool confirmation" not "Show input dialog"

### Platform-Agnostic

Same interface works for:
- CLI (terminal output, stdin input)
- Web UI (WebSocket streaming, form inputs)
- API (JSON events, HTTP requests)
- GUI (native widgets, event loops)

### Cancellation Support

Interactive methods accept cancellation tokens:
- User can cancel long-running operations
- Clean shutdown without hanging
- Cooperative cancellation (no force-kill)

### Real-Time Feedback

Non-blocking notifications enable responsive UIs:
- State changes notify immediately
- Chunks stream as they arrive
- No polling required

## Future Enhancements

- Progress reporting (percentage, ETA)
- File upload/download requests
- Multi-choice confirmations
- Rich content (images, tables, code blocks)
- Batch operations
- Error/warning notifications
- Metrics/telemetry hooks
