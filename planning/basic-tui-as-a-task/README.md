# Basic TUI Implementation Plan

This directory contains the architecture and implementation plan for the basic Terminal User Interface (TUI) for the agent loop.

## Overview

The goal is to implement a TUI that:
- Runs agent tasks to completion
- Prompts user for next input
- Spawns new tasks in a loop
- Handles Ctrl+C gracefully (cancel job vs exit app)
- Cleans up old completed/cancelled jobs

## Documents

### [00-overview.md](00-overview.md)
High-level architecture overview covering:
- Main TUI loop flow
- Key components and their responsibilities
- Integration with existing ChatArgs.execute()
- File structure and dependencies

### [01-job-cleanup.md](01-job-cleanup.md)
Job lifecycle management covering:
- Keeping max 3 inactive jobs
- Cleanup triggers and timing
- Session and WorkerJob enhancements
- Testing strategies

### [02-user-input.md](02-user-input.md)
User input handling covering:
- InputHandler design using rustyline
- Async input with cancellation support
- Special commands (/quit, /help, etc.)
- History management

### [03-ctrl-c-handling.md](03-ctrl-c-handling.md)
Ctrl+C signal handling covering:
- Context-aware behavior (prompt vs job)
- Double Ctrl+C detection
- CtrlCHandler implementation
- Integration with main loop

### [04-graceful-shutdown.md](04-graceful-shutdown.md)
Graceful shutdown coordination covering:
- ShutdownCoordinator design
- Shutdown sequence and triggers
- Timeout handling
- Integration with ChatArgs.execute()

### [05-complete-flow-example.md](05-complete-flow-example.md)
Complete implementation with concrete code examples:
- Full AgentEnvUi implementation
- All component implementations
- Example user sessions
- Integration patterns

### [06-diagrams.md](06-diagrams.md)
Visual diagrams and flowcharts:
- Component relationships
- Main loop flow
- Ctrl+C state machine
- Job lifecycle
- Shutdown sequence
- Memory layout examples

## Key Design Decisions

### 1. Job Cleanup
- **Decision**: Keep max 3 inactive jobs, cleanup before spawning new job
- **Rationale**: Provides recent history for debugging without unbounded growth

### 2. Ctrl+C Behavior
- **Decision**: Different behavior for prompt (exit) vs job (cancel)
- **Rationale**: Matches user expectations and provides quick exit + job cancellation

### 3. Shutdown Coordination
- **Decision**: Wait for jobs to complete immediately after cancellation
- **Rationale**: Jobs respond to cancellation tokens quickly, no need for timeout polling

### 4. Input Handling
- **Decision**: Use rustyline with async wrapper
- **Rationale**: Provides readline features (history, editing) with cancellation support

## Implementation Order

1. **Session enhancements** (job cleanup)
   - Add MAX_INACTIVE_JOBS constant
   - Add is_active() and get_state() to WorkerJob
   - Add cleanup_inactive_jobs() to Session

2. **InputHandler** (user input)
   - Create InputHandler with rustyline
   - Add async wrapper with spawn_blocking
   - Add cancellation token support
   - Add history management

3. **CtrlCHandler** (Ctrl+C handling)
   - Create CtrlCHandler with state tracking
   - Implement signal listener
   - Add context switching methods
   - Implement double-press detection

4. **ShutdownCoordinator** (graceful shutdown)
   - Create ShutdownCoordinator
   - Add `wait_for_active_jobs()` to Session
   - Implement shutdown sequence

5. **AgentEnvUi** (main TUI loop)
   - Create AgentEnvUi struct
   - Implement main loop
   - Integrate all components
   - Add error handling

6. **Integration** (ChatArgs.execute)
   - Add agent_env mode flag
   - Route to AgentEnvUi.run()
   - Test end-to-end flow

## File Structure

```
crates/chat-cli/src/
├── agent_env/
│   ├── session.rs              # Enhanced with job cleanup
│   ├── worker_job.rs           # Enhanced with state queries
│   └── ...
└── cli/chat/
    ├── mod.rs                  # ChatArgs.execute() integration
    └── agent_env_ui/           # NEW
        ├── mod.rs              # AgentEnvUi main loop
        ├── input_handler.rs    # User input with rustyline
        ├── ctrl_c_handler.rs   # Ctrl+C signal handling
        └── shutdown_coordinator.rs  # Graceful shutdown
```

## Constants

```rust
// Job cleanup
pub const MAX_INACTIVE_JOBS: usize = 3;

// Double Ctrl+C window
pub const DOUBLE_CTRL_C_WINDOW_MS: u64 = 1000;
```

## Dependencies

All dependencies already in project:
- `rustyline` - readline implementation
- `crossterm` - terminal styling
- `tokio` - async runtime
- `tokio-util` - CancellationToken
- `tokio::signal` - Ctrl+C handling

## Testing Strategy

Each component has dedicated test scenarios:
- **Job cleanup**: Verify max 3 inactive jobs kept
- **Input handling**: Test cancellation, empty input, special commands
- **Ctrl+C handling**: Test context switching, double-press detection
- **Shutdown**: Test multiple triggers, error handling, immediate job completion
- **Integration**: Test full TUI loop end-to-end

## Next Steps

1. Review this plan with team
2. Start implementation following the order above
3. Write tests alongside implementation
4. Integrate with existing chat infrastructure
5. Test with real agent tasks
