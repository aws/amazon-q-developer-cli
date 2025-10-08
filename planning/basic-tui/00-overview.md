# Basic TUI Architecture Overview

## Goal
Implement a basic Terminal User Interface (TUI) for the agent loop that:
1. Runs a job (agent task) to completion
2. Prompts user for next input
3. Spawns new job with user input
4. Repeats the cycle
5. Handles Ctrl+C gracefully (cancel job vs exit app)
6. Cleans up old completed/cancelled jobs

## High-Level Flow

```
[App Start]
    ↓
[Initialize Session + UI]
    ↓
[Prompt User] ←──────────────┐
    ↓                         │
[Spawn Job]                   │
    ↓                         │
[Wait for Job Completion]     │
    ↓                         │
[Cleanup Old Jobs]            │
    ↓                         │
[Check Exit Condition] ───────┤
    ↓ (continue)              │
    └─────────────────────────┘
    ↓ (exit)
[Graceful Shutdown]
    ↓
[Exit]
```

## Key Components

### 1. AgentEnvUi (new module)
Location: `crates/chat-cli/src/cli/chat/agent_env_ui/`

Responsibilities:
- Manage the main TUI loop
- Handle user input prompts
- Coordinate job lifecycle
- Handle Ctrl+C signals
- Trigger job cleanup

### 2. Session (enhanced)
Location: `crates/chat-cli/src/agent_env/session.rs`

New responsibilities:
- Track job states (active/inactive)
- Cleanup old inactive jobs (keep max 3)
- Provide job state queries

### 3. InputHandler (new)
Location: `crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs`

Responsibilities:
- Wrap rustyline for user input
- Handle Ctrl+C during prompt (exit app)
- Provide cancellable async input

### 4. CtrlCHandler (new)
Location: `crates/chat-cli/src/cli/chat/agent_env_ui/ctrl_c_handler.rs`

Responsibilities:
- Register Ctrl+C signal handler
- Distinguish between "in prompt" vs "job running" contexts
- Trigger appropriate action (cancel job vs exit app)

### 5. ShutdownCoordinator (new)
Location: `crates/chat-cli/src/cli/chat/agent_env_ui/shutdown_coordinator.rs`

Responsibilities:
- Coordinate graceful shutdown
- Cancel all active jobs
- Wait for cleanup completion
- Provide await mechanism for ChatArgs.execute
- Support "hard stop" to force completion if needed (e.g., second Ctrl+C during shutdown)

## Integration Point

The new TUI replaces existing logic in `ChatArgs.execute()`:
```rust
// In crates/chat-cli/src/cli/chat/mod.rs
impl ChatArgs {
    pub async fn execute(&self, ...) -> Result<()> {
        // ... setup: model providers, history path, etc ...
        
        let session = Session::new(model_providers);
        let ui = AgentEnvUi::new(session, history_path)?;
        ui.run().await
    }
}
```


## State Management

### Job States
- **Active**: Job is currently running
- **Inactive (Completed)**: Job finished successfully
- **Inactive (Cancelled)**: Job was cancelled
- **Inactive (Failed)**: Job failed with error

### Cleanup Policy
- Keep maximum 3 inactive jobs in memory
- When spawning new job, remove oldest inactive jobs if count > 3
- Active jobs are never removed

## Ctrl+C Behavior

### Context: User Prompt
- **First Ctrl+C**: Exit application immediately
- No "double Ctrl+C" pattern needed (different from old chat session)

### Context: Job Running
- **First Ctrl+C**: Cancel current job, return to prompt
- **Second Ctrl+C** (within 1s, before returning to prompt): Exit application


## Shutdown Sequence

1. User triggers exit (Ctrl+C at prompt or `/quit` command)
2. ShutdownCoordinator sets shutdown flag
3. Cancel all active jobs via Session.cancel_all_jobs()
4. Wait for job cancellations to complete (with timeout)
5. Cleanup resources
6. Return from ChatArgs.execute()

## Constants

```rust
// In session.rs or config module
pub const MAX_INACTIVE_JOBS: usize = 3;
```

## Dependencies

Already in project:
- `rustyline` - for user input
- `crossterm` - for terminal styling
- `tokio::signal::ctrl_c` - for Ctrl+C handling (already used in chat_session.rs)
- `tokio::sync::broadcast` - for signal broadcasting (already used in chat_session.rs)
- `tokio_util::sync::CancellationToken` - for cancellation coordination

## File Structure

```
crates/chat-cli/src/cli/chat/agent_env_ui/
├── mod.rs                      # Main TUI loop
├── input_handler.rs            # User input with rustyline
├── ctrl_c_handler.rs           # Ctrl+C signal handling
├── shutdown_coordinator.rs     # Graceful shutdown logic
└── ui_state.rs                 # UI state tracking
```


## Next Steps

See individual design documents:
- `01-job-cleanup.md` - Job cleanup implementation
- `02-user-input.md` - User input handling
- `03-ctrl-c-handling.md` - Ctrl+C signal handling
- `04-graceful-shutdown.md` - Shutdown coordination
