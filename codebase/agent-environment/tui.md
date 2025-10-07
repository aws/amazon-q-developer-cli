# Terminal User Interface (TUI)

## Overview

The TUI provides an interactive terminal interface for the agent environment, enabling users to:
- Run agent tasks to completion
- Provide input for subsequent tasks
- Cancel running tasks with Ctrl+C
- Exit gracefully with /quit or Ctrl+C at prompt

## Architecture

The TUI is built on the continuation-based architecture, where job completion automatically triggers the next prompt through continuations rather than explicit loops.

### Key Components

#### AgentEnvTextUi
**Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs`

Main orchestrator that:
- Manages the prompt queue
- Processes user input
- Launches agent tasks
- Sets up continuations for job completion
- Handles graceful shutdown

#### PromptQueue
**Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/prompt_queue.rs`

FIFO queue for managing prompt requests from multiple workers:
- `enqueue(worker)` - Add worker to prompt queue
- `dequeue()` - Get next worker to prompt
- Supports future parallel worker execution

#### InputHandler
**Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs`

User input management using rustyline:
- Readline features (history, editing)
- Ctrl+C/Ctrl+D detection
- History persistence
- Async wrapper for blocking readline

#### TextUiWorkerToHostInterface
**Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/text_ui_worker_to_host_interface.rs`

Implements WorkerToHostInterface for terminal output:
- Streams assistant messages to stdout
- Optional color support for distinguishing multiple workers
- Displays tool use requests
- Auto-approves tools (TODO: interactive confirmation)

**Color Support**: Accepts optional ANSI color code in constructor:
```rust
TextUiWorkerToHostInterface::new(Some("\x1b[32m"))  // Green
TextUiWorkerToHostInterface::new(None)              // No color
```

#### CtrlCHandler
**Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/ctrl_c_handler.rs`

Signal handling with context-aware behavior:
- First Ctrl+C: Cancel active jobs
- Second Ctrl+C (within 1s): Force exit
- Ctrl+C at prompt: Exit immediately

## Flow

### Startup Flow (Two Workers Implementation)
```
ChatArgs.execute()
    ↓
Create Session with model providers
    ↓
Create AgentEnvTextUi
    ↓
Build two workers (worker1, worker2)
    ↓
Pre-register colored interfaces:
    - Worker 1: Green (\x1b[32m)
    - Worker 2: Cyan (\x1b[36m)
    ↓
If input provided:
    - Stage input in both workers' contexts
    - Launch AgentLoop for worker1 with continuation
    - Launch AgentLoop for worker2 with continuation
Else:
    - Trigger continuation for worker1 (queues for prompt)
    - Trigger continuation for worker2 (queues for prompt)
    ↓
Run UI main loop
```

### Main Loop Flow
```
Loop:
    Check shutdown signal → Exit if triggered
    ↓
    Dequeue next prompt request
    ↓
    Read user input (blocks)
    ↓
    Handle special cases:
        - Empty input → Re-queue and continue
        - /quit → Exit
        - Ctrl+C/Ctrl+D → Exit
    ↓
    Cleanup old inactive jobs
    ↓
    Push input to conversation history
    ↓
    Launch AgentLoop
    ↓
    Set continuation to re-queue prompt on completion
```

### Continuation Flow
```
AgentLoop completes
    ↓
Continuation triggered with completion type
    ↓
Display completion message (if failed/cancelled)
    ↓
Re-queue worker for next prompt
    ↓
Main loop processes next prompt
```

## Job Cleanup

The Session maintains a maximum of 3 inactive jobs to prevent unbounded memory growth:

- **Cleanup trigger**: Before launching new job
- **Policy**: Keep most recent 3 inactive jobs
- **Active jobs**: Never removed
- **Implementation**: `Session::cleanup_inactive_jobs()`

## Ctrl+C Behavior

### During Prompt
- InputHandler returns error
- Main loop exits
- Application shuts down

### During AgentLoop
- First Ctrl+C: Cancels job via CancellationToken
- Job completes with Cancelled status
- Continuation re-queues prompt
- User can continue or exit

### Double Ctrl+C
- Within 1 second of first Ctrl+C
- Triggers shutdown signal
- Main loop exits immediately
- All jobs cancelled

## Graceful Shutdown

Shutdown sequence:
1. Shutdown signal triggered (Ctrl+C, /quit, error)
2. Main loop exits
3. `Session::cancel_all_jobs()` called
4. Input history saved
5. Return from `ChatArgs.execute()`

## Multi-Worker Support (Implemented)

The architecture now supports multiple workers running in parallel with color-coded output.

### Worker Interface Management

**AgentEnvTextUi** maintains a map of worker interfaces:

```rust
pub struct AgentEnvTextUi {
    // ... other fields
    worker_interfaces: Arc<Mutex<HashMap<Uuid, Arc<dyn WorkerToHostInterface>>>>,
}
```

### get_worker_interface Method

Provides lookup-first pattern for interface reuse:

```rust
pub fn get_worker_interface(
    &self,
    worker_id: Option<Uuid>,
    color: Option<&'static str>,
) -> Arc<dyn WorkerToHostInterface> {
    if let Some(id) = worker_id {
        let mut interfaces = self.worker_interfaces.lock().unwrap();
        
        // Check if interface already exists
        if let Some(interface) = interfaces.get(&id) {
            return interface.clone();
        }
        
        // Create and store new interface
        let interface = Arc::new(TextUiWorkerToHostInterface::new(color));
        interfaces.insert(id, interface.clone());
        interface
    } else {
        // No worker_id, create without storing
        Arc::new(TextUiWorkerToHostInterface::new(color))
    }
}
```

### Usage Pattern

```rust
// Create workers
let worker1 = session.build_worker();
let worker2 = session.build_worker();

// Pre-register colored interfaces
ui.get_worker_interface(Some(worker1.id), Some("\x1b[32m"));  // Green
ui.get_worker_interface(Some(worker2.id), Some("\x1b[36m"));  // Cyan

// Launch jobs - interfaces automatically reused
let job1 = session.run_agent_loop(
    worker1.clone(),
    AgentLoopInput {},
    ui.get_worker_interface(Some(worker1.id), None),
)?;

let job2 = session.run_agent_loop(
    worker2.clone(),
    AgentLoopInput {},
    ui.get_worker_interface(Some(worker2.id), None),
)?;
```

### Key Features

- **Interface Reuse**: Same interface instance used across multiple job launches
- **Color Coding**: Each worker gets distinct color for output
- **Automatic Lookup**: `get_worker_interface()` returns existing interface if present
- **Prompt Queue**: Ensures one prompt at a time despite multiple workers
- **FIFO Ordering**: Workers prompted in completion order

## Integration

### Entry Point (Two Workers Implementation)
**Location**: `crates/chat-cli/src/cli/chat/mod.rs`

```rust
impl ChatArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        let session = Arc::new(build_session().await?);
        let history_path = chat_cli_bash_history_path(os).ok();
        let ui = AgentEnvTextUi::new(session.clone(), history_path)?;
        
        // Create two workers
        let worker1 = session.build_worker();
        let worker2 = session.build_worker();
        
        // Pre-register colored interfaces
        ui.get_worker_interface(Some(worker1.id), Some("\x1b[32m"));
        ui.get_worker_interface(Some(worker2.id), Some("\x1b[36m"));
        
        if let Some(input) = self.input {
            // Launch both workers with same input
            worker1.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
            
            let job1 = session.run_agent_loop(
                worker1.clone(),
                AgentLoopInput {},
                ui.get_worker_interface(Some(worker1.id), None),
            )?;
            
            let continuation1 = ui.create_agent_completion_continuation();
            job1.worker_job_continuations.add_or_run_now(
                "agent_to_prompt",
                continuation1,
                worker1.clone(),
            ).await;
            
            worker2.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input);
            
            let job2 = session.run_agent_loop(
                worker2.clone(),
                AgentLoopInput {},
                ui.get_worker_interface(Some(worker2.id), None),
            )?;
            
            let continuation2 = ui.create_agent_completion_continuation();
            job2.worker_job_continuations.add_or_run_now(
                "agent_to_prompt",
                continuation2,
                worker2.clone(),
            ).await;
        } else {
            // Queue both workers for prompts
            let continuation = ui.create_agent_completion_continuation();
            continuation(worker1, WorkerJobCompletionType::Normal, None).await;
            continuation(worker2, WorkerJobCompletionType::Normal, None).await;
        }
        
        ui.run().await?;
        Ok(ExitCode::SUCCESS)
    }
}
```

## Constants

```rust
// Session job cleanup
pub const MAX_INACTIVE_JOBS: usize = 3;

// Ctrl+C double-press window
const DOUBLE_CTRL_C_WINDOW_MS: u64 = 1000;
```

## Dependencies

All dependencies already in project:
- `rustyline` - Readline implementation
- `crossterm` - Terminal styling
- `tokio` - Async runtime
- `tokio-util` - CancellationToken
- `tokio::signal` - Ctrl+C handling

## Future Enhancements

1. **Interactive tool confirmation**: Replace auto-approval with user prompts
2. **Rich terminal UI**: Progress bars, status indicators
3. **Command history search**: Fuzzy finding in history
4. **Multi-worker UI**: Display multiple workers with status
5. **Streaming indicators**: Show when agent is thinking/working
