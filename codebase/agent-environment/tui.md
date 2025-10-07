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
- Displays tool use requests
- Auto-approves tools (TODO: interactive confirmation)

#### CtrlCHandler
**Location**: `crates/chat-cli/src/cli/chat/agent_env_ui/ctrl_c_handler.rs`

Signal handling with context-aware behavior:
- First Ctrl+C: Cancel active jobs
- Second Ctrl+C (within 1s): Force exit
- Ctrl+C at prompt: Exit immediately

## Flow

### Startup Flow
```
ChatArgs.execute()
    ↓
Create Session with model providers
    ↓
Create AgentEnvTextUi
    ↓
Build initial worker
    ↓
If input provided:
    - Stage input in worker context
    - Launch AgentLoop with continuation
Else:
    - Trigger continuation to start with prompt
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

## Multi-Worker Support (Future)

The architecture supports multiple workers running in parallel:

```rust
// Create additional worker
let worker2 = session.build_worker();

// Launch job for worker2
let job = session.run_agent_loop(
    worker2.clone(),
    AgentLoopInput {},
    ui.create_ui_interface(),
)?;

// Use UI's continuation to re-queue prompt
let continuation = ui.create_agent_completion_continuation();
job.worker_job_continuations.add_or_run_now(
    "agent_to_prompt",
    continuation,
    worker2.clone(),
).await;
```

Key points:
- Prompt queue ensures one prompt at a time
- Worker name displayed in prompt
- FIFO ordering for fairness
- Jobs can run while prompts are queued

## Integration

### Entry Point
**Location**: `crates/chat-cli/src/cli/chat/mod.rs`

```rust
impl ChatArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        let session = Arc::new(build_session().await?);
        let history_path = chat_cli_bash_history_path(os).ok();
        let ui = AgentEnvTextUi::new(session.clone(), history_path)?;
        
        let worker = session.build_worker();
        
        if let Some(input) = self.input {
            // Launch with input
            worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input);
            
            let job = session.run_agent_loop(
                worker.clone(),
                AgentLoopInput {},
                ui.create_ui_interface(),
            )?;
            
            let continuation = ui.create_agent_completion_continuation();
            job.worker_job_continuations.add_or_run_now(
                "agent_to_prompt",
                continuation,
                worker.clone(),
            ).await;
        } else {
            // Start with prompt
            let continuation = ui.create_agent_completion_continuation();
            continuation(worker, WorkerJobCompletionType::Normal, None).await;
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
