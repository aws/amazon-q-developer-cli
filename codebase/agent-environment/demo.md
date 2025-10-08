# Demo Implementation

## Overview

The demo implementation showcases the complete agent environment architecture with a working example that demonstrates concurrent agents, streaming responses, state management, and user interaction.

## Demo Entry Point

**File**: `crates/chat-cli/src/agent_env/demo/init.rs`

### Main Function

**Location**: `init.rs` lines 7-42

```rust
pub async fn demo_main() -> Result<(), eyre::Error>
```

Demonstrates the architecture:

1. **AWS Setup** (lines 10-12)
   ```rust
   let config = aws_config::load_from_env().await;
   let client = aws_sdk_bedrockruntime::Client::new(&config);
   ```

2. **Session Creation** (lines 14-17)
   ```rust
   let session = Session::new(vec![
       BedrockConverseStreamModelProvider::new(client),
   ]);
   ```

3. **Worker Creation** (lines 19-20)
   ```rust
   let worker1 = session.build_worker();
   let worker2 = session.build_worker();
   ```

4. **Launch Agent Loops** (lines 22-35)
   ```rust
   let job1 = session.run_agent_loop(
       worker1.clone(),
       AgentLoopInput { prompt: "Say hello".to_string() },
       Arc::new(CliInterface { color: Color::Cyan }),
   )?;
   
   let job2 = session.run_agent_loop(
       worker2.clone(),
       AgentLoopInput { prompt: "Count to 5".to_string() },
       Arc::new(CliInterface { color: Color::Green }),
   )?;
   ```

5. **Wait for Completion** (lines 37-38)
   ```rust
   job1.wait().await?;
   job2.wait().await?;
   ```

## CLI Interface

**File**: `crates/chat-cli/src/agent_env/demo/cli_interface.rs`

Provides colored console output for demo visualization.

### Structure

**Location**: `cli_interface.rs` lines 11-13

```rust
pub struct CliInterface {
    pub color: Color,
}
```

Each agent gets a different color for visual distinction.

### Implementation Details

See [interface.md](./interface.md) for full implementation details.

### Color Usage

- **Cyan**: First agent
- **Green**: Second agent
- **Yellow**: Warnings/tool requests
- **Red**: Errors

## Proto Loop (Legacy Demo)

**File**: `crates/chat-cli/src/agent_env/demo/proto_loop.rs`

Original demonstration task showing complete flow with tool confirmation.

### Structure

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

### Execution Flow

**Location**: `proto_loop.rs` lines 49-119

Demonstrates complete agent flow:

1. **Working State** (lines 54-56)
   ```rust
   self.worker.set_state(WorkerStates::Working, &*self.host_interface);
   tokio::time::sleep(Duration::from_millis(500)).await;
   ```

2. **LLM Request** (lines 58-78)
   ```rust
   self.worker.set_state(WorkerStates::Requesting, &*self.host_interface);
   let response = self.worker.model_provider.request(
       request,
       || { self.worker.set_state(WorkerStates::Receiving, &*self.host_interface); },
       |chunk| { self.host_interface.response_chunk_received(self.worker.id, chunk); },
       self.cancellation_token.clone(),
   ).await?;
   ```

3. **User Interaction** (lines 80-91)
   ```rust
   self.worker.set_state(WorkerStates::Waiting, &*self.host_interface);
   let user_input = self.host_interface.get_tool_confirmation(
       self.worker.id,
       format!("Enter something ({}): ", self.input.color),
       self.cancellation_token.clone(),
   ).await?;
   ```

4. **Completion** (lines 93-95)
   ```rust
   self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
   ```

## Running the Demo

### Two Workers Demo (Current Implementation)

The current implementation demonstrates two workers running in parallel with color-coded output.

**Entry Point**: `crates/chat-cli/src/cli/chat/mod.rs` - `ChatArgs::execute()`

#### With Input (Both Workers Process Same Input)

```bash
cargo run --bin chat_cli -- chat "Tell me a joke"
```

Expected behavior:
- Both workers receive the same input
- Both start processing simultaneously
- Output appears interleaved with green and cyan colors
- Both workers complete and return to prompt queue

#### Interactive Mode (Workers Alternate)

```bash
cargo run --bin chat_cli -- chat
```

Expected behavior:
- Both workers queued for prompts
- First prompt goes to worker 1 (green output)
- After completion, worker 1 re-queues
- Second prompt goes to worker 2 (cyan output)
- Alternating pattern continues

### Expected Output

With input:
```
Starting Agent Environment with TWO WORKERS...

[Green text] Hello! Here's a joke for you...
[Cyan text] Sure! Why did the chicken...
[Green text] ...cross the road?
[Cyan text] ...cross the road?
```

Interactive mode:
```
Starting Agent Environment with TWO WORKERS...
Worker-1> Hello
[Green text] Hi! How can I help you?

Worker-2> What's 2+2?
[Cyan text] 2+2 equals 4.
```

**Note**: Output will be interleaved character-by-character. This is expected behavior for this iteration.

## Demo Features Showcased

### Two Workers Implementation

**Current Status**: Fully implemented and functional

The production implementation in `ChatArgs::execute()` demonstrates:

#### Color-Coded Output
- **Worker 1**: Green (`\x1b[32m`)
- **Worker 2**: Cyan (`\x1b[36m`)
- Colors help distinguish output from each worker

#### Worker Interface Reuse
- Interfaces pre-registered with colors in `AgentEnvTextUi`
- `get_worker_interface()` method provides lookup-first pattern
- Same interface instance reused across multiple job launches

#### Parallel Execution
- Both workers process same input simultaneously
- Independent state machines
- Shared Session and model provider
- Output streams interleaved in real-time

#### Prompt Queue Management
- Workers automatically re-queue after completion
- FIFO ordering ensures fair access
- Single input handler serializes user prompts

### Concurrent Execution

Two agents run simultaneously:
- Different prompts
- Different colors
- Independent state machines
- Shared model provider

### Streaming Responses

Text appears character-by-character:
- Real-time feedback
- Low latency perception
- Responsive UI

### State Management

State transitions visible:
- Working → Requesting → Receiving → Inactive
- Color-coded for each agent
- Real-time updates

### Resource Sharing

Both agents share:
- Same Session
- Same model provider
- Same tokio runtime
- Independent execution

### Cancellation Support

Can be cancelled mid-execution:
- Ctrl+C support
- Clean shutdown
- No hanging tasks

## Customizing the Demo

### Change Prompts

Edit `init.rs` lines 24, 30:
```rust
AgentLoopInput { prompt: "Your custom prompt".to_string() }
```

### Change Colors

Edit `init.rs` lines 26, 32:
```rust
Arc::new(CliInterface { color: Color::Red })
```

### Add More Agents

```rust
let worker3 = session.build_worker();
let job3 = session.run_agent_loop(
    worker3.clone(),
    AgentLoopInput { prompt: "Third task".to_string() },
    Arc::new(CliInterface { color: Color::Yellow }),
)?;
job3.wait().await?;
```

### Add Delays

```rust
tokio::time::sleep(Duration::from_secs(2)).await;
```

### Add Cancellation

```rust
// Cancel after 5 seconds
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(5)).await;
    session.cancel_all_jobs();
});
```

## Testing the Demo

### Verify Concurrent Execution

Both agents should output interleaved:
- Colors alternate
- State changes interleaved
- Responses stream simultaneously

### Verify Cancellation

Press Ctrl+C during execution:
- Both agents should stop
- Clean shutdown
- No error messages

### Verify Error Handling

Disconnect network during execution:
- Agents should fail gracefully
- Error messages displayed
- State transitions to InactiveFailed

## Demo Limitations

### Current Implementation (Two Workers)

**Known Limitations**:
- **Interleaved Output**: Character-by-character mixing of output from both workers
  - This is expected and will be addressed with proper TUI in future iteration
- **Single Input Handler**: Only one worker can receive user input at a time
  - This is correct behavior - prompt queue serializes input requests
- **No Worker Identification**: Only color distinguishes workers
  - Could add worker name/ID prefix if needed
- **Auto-Approve Tools**: All tool requests auto-approved
  - Real implementation will need proper tool approval UI

### Legacy Demo Limitations

Original demo (proto_loop) is simplified:
- No tool execution
- No conversation history
- No error recovery
- No progress reporting
- Fixed model parameters

These will be added in production implementation.

## Next Steps

After demo validation:
1. Integrate with ChatArgs.execute()
2. Add real tool execution
3. Add conversation persistence
4. Add configuration management
5. Add metrics/telemetry
6. Add comprehensive error handling
