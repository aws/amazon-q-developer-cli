# Graceful Shutdown Design (Continuation-Based)

## Problem
When user exits the application (via Ctrl+C, /quit, or Ctrl+D), we need to:
1. Stop the continuation chain from spawning new tasks
2. Cancel all active jobs
3. Wait for jobs to finish cancellation
4. Save state (history, etc.)
5. Clean up resources
6. Return from `ChatArgs.execute()` properly

The challenge: No explicit "main loop" to break out of - shutdown must work through continuation chain.

## Requirements
- Stop continuation chain from spawning new tasks
- Cancel all active jobs when shutdown is triggered
- Wait for job cancellations with timeout
- Save user input history
- Clean up resources (file handles, network connections)
- Return from async `ChatArgs.execute()` method
- Handle shutdown from multiple sources (Ctrl+C, /quit, error)

## Architecture

### Shutdown Flow
```
Shutdown Trigger (Ctrl+C, /quit, etc.)
    ↓
Cancel shutdown_token
    ↓
Continuations check shutdown_token
    ↓
Stop spawning new tasks
    ↓
Cancel all active jobs
    ↓
Wait for jobs to complete (with timeout)
    ↓
Save history and cleanup
    ↓
Return from AgentEnvUi.run()
```

## AgentEnvUi Design

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs`

### Structure
```rust
use std::sync::Arc;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub struct AgentEnvUi {
    session: Arc<Session>,
    input_handler: Arc<tokio::sync::Mutex<InputHandler>>,
    shutdown_token: CancellationToken,
    history_path: PathBuf,
}

impl AgentEnvUi {
    pub fn new(session: Session, history_path: PathBuf) -> Result<Self, eyre::Error> {
        let input_handler = InputHandler::new(Some(history_path.clone()))?;
        
        Ok(Self {
            session: Arc::new(session),
            input_handler: Arc::new(tokio::sync::Mutex::new(input_handler)),
            shutdown_token: CancellationToken::new(),
            history_path,
        })
    }
    
    /// Main entry point - blocks until shutdown
    pub async fn run(self) -> Result<(), eyre::Error> {
        let session = self.session.clone();
        let shutdown_token = self.shutdown_token.clone();
        
        // Setup Ctrl+C handler
        let ctrl_c_handler = Arc::new(CtrlCHandler::new(
            shutdown_token.clone(),
            session.clone(),
        ));
        ctrl_c_handler.start_listening();
        
        // Build worker
        let worker = session.build_worker();
        let ui_interface = Arc::new(CliInterface::new("\x1b[36m"));
        
        // Launch initial PromptTask with continuation chain
        self.launch_initial_prompt(
            worker,
            ui_interface,
            shutdown_token.clone(),
        ).await?;
        
        // Wait for shutdown signal
        shutdown_token.cancelled().await;
        
        // Perform graceful shutdown
        self.shutdown().await?;
        
        Ok(())
    }
    
    /// Launch initial PromptTask that starts the continuation chain
    async fn launch_initial_prompt(
        &self,
        worker: Arc<Worker>,
        ui_interface: Arc<dyn WorkerToHostInterface>,
        shutdown_token: CancellationToken,
    ) -> Result<(), eyre::Error> {
        let prompt_task = PromptTask::new(
            worker.clone(),
            self.input_handler.clone(),
            CancellationToken::new(),
        );
        
        let mut job = self.session.launch_task(
            worker.clone(),
            Arc::new(prompt_task),
        )?;
        
        // Set up continuation: PromptTask → AgentLoop
        let continuation = self.create_prompt_to_agent_continuation(
            worker,
            ui_interface,
            shutdown_token,
        );
        
        job.worker_job_continuations.add_or_run_now(
            "prompt_to_agent",
            continuation,
            worker.clone(),
        ).await;
        
        job.launch();
        
        Ok(())
    }
    
    /// Perform graceful shutdown
    async fn shutdown(&self) -> Result<(), eyre::Error> {
        println!("\nShutting down...");
        
        // 1. Cancel all active jobs
        self.session.cancel_all_jobs();
        
        // 2. Wait for jobs to complete (with timeout)
        tokio::select! {
            _ = self.session.wait_for_all_jobs() => {
                // Normal completion
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                // Timeout - force exit
                eprintln!("Warning: Jobs did not complete within timeout");
            }
        }
        
        // 3. Save input history
        let mut handler = self.input_handler.lock().await;
        if let Err(e) = handler.save_history(&self.history_path) {
            eprintln!("Warning: Failed to save history: {}", e);
        }
        
        println!("Shutdown complete");
        Ok(())
    }
}
```

## Continuation Chain with Shutdown Checks

### PromptTask → AgentLoop Continuation
```rust
fn create_prompt_to_agent_continuation(
    &self,
    worker: Arc<Worker>,
    ui_interface: Arc<dyn WorkerToHostInterface>,
    shutdown_token: CancellationToken,
) -> WorkerJobContinuationFn {
    let session = self.session.clone();
    let input_handler = self.input_handler.clone();
    
    Continuations::boxed(move |worker, completion_type, error_msg| {
        let session = session.clone();
        let worker = worker.clone();
        let ui_interface = ui_interface.clone();
        let input_handler = input_handler.clone();
        let shutdown_token = shutdown_token.clone();
        
        async move {
            // Check shutdown FIRST
            if shutdown_token.is_cancelled() {
                return;  // Stop continuation chain
            }
            
            match completion_type {
                WorkerJobCompletionType::Cancelled => {
                    // PromptTask cancelled (Ctrl+C during prompt) - trigger shutdown
                    shutdown_token.cancel();
                    return;
                }
                WorkerJobCompletionType::Failed => {
                    // Error during input
                    if let Some(msg) = error_msg {
                        eprintln!("Input error: {}", msg);
                    }
                    shutdown_token.cancel();
                    return;
                }
                WorkerJobCompletionType::Normal => {
                    // Get user input
                    let input = {
                        let handler = input_handler.lock().await;
                        match handler.get_last_input() {
                            Some(input) => input.to_string(),
                            None => {
                                shutdown_token.cancel();
                                return;
                            }
                        }
                    };
                    
                    // Check for /quit command
                    if input.trim() == "/quit" {
                        shutdown_token.cancel();
                        return;
                    }
                    
                    // Check shutdown again before launching
                    if shutdown_token.is_cancelled() {
                        return;
                    }
                    
                    // Launch AgentLoop
                    let agent_input = AgentLoopInput {
                        prompt: input,
                    };
                    
                    let mut job = match session.run_agent_loop(
                        worker.clone(),
                        agent_input,
                        ui_interface.clone(),
                    ) {
                        Ok(job) => job,
                        Err(e) => {
                            eprintln!("Failed to launch agent: {}", e);
                            shutdown_token.cancel();
                            return;
                        }
                    };
                    
                    // Set up continuation: AgentLoop → PromptTask
                    let continuation = create_agent_to_prompt_continuation(
                        session.clone(),
                        worker.clone(),
                        input_handler.clone(),
                        ui_interface.clone(),
                        shutdown_token.clone(),
                    );
                    
                    job.worker_job_continuations.add_or_run_now(
                        "agent_to_prompt",
                        continuation,
                        worker.clone(),
                    ).await;
                    
                    job.launch();
                }
            }
        }
    })
}
```

### AgentLoop → PromptTask Continuation
```rust
fn create_agent_to_prompt_continuation(
    session: Arc<Session>,
    worker: Arc<Worker>,
    input_handler: Arc<tokio::sync::Mutex<InputHandler>>,
    ui_interface: Arc<dyn WorkerToHostInterface>,
    shutdown_token: CancellationToken,
) -> WorkerJobContinuationFn {
    Continuations::boxed(move |worker, completion_type, error_msg| {
        let session = session.clone();
        let worker = worker.clone();
        let input_handler = input_handler.clone();
        let ui_interface = ui_interface.clone();
        let shutdown_token = shutdown_token.clone();
        
        async move {
            // Check shutdown FIRST
            if shutdown_token.is_cancelled() {
                return;  // Stop continuation chain
            }
            
            match completion_type {
                WorkerJobCompletionType::Failed => {
                    if let Some(msg) = error_msg {
                        eprintln!("Agent failed: {}", msg);
                    }
                    // Continue to prompt anyway (don't exit on agent failure)
                }
                WorkerJobCompletionType::Cancelled => {
                    println!("Task cancelled");
                    // Continue to prompt (user can try again or quit)
                }
                WorkerJobCompletionType::Normal => {
                    // Normal completion
                }
            }
            
            // Check shutdown again before launching
            if shutdown_token.is_cancelled() {
                return;
            }
            
            // Launch PromptTask
            let prompt_task = PromptTask::new(
                worker.clone(),
                input_handler.clone(),
                CancellationToken::new(),
            );
            
            let mut job = match session.launch_task(
                worker.clone(),
                Arc::new(prompt_task),
            ) {
                Ok(job) => job,
                Err(e) => {
                    eprintln!("Failed to launch prompt: {}", e);
                    shutdown_token.cancel();
                    return;
                }
            };
            
            // Set up continuation: PromptTask → AgentLoop
            let continuation = create_prompt_to_agent_continuation(
                session.clone(),
                worker.clone(),
                input_handler.clone(),
                ui_interface.clone(),
                shutdown_token.clone(),
            );
            
            job.worker_job_continuations.add_or_run_now(
                "prompt_to_agent",
                continuation,
                worker.clone(),
            ).await;
            
            job.launch();
        }
    })
}
```

## Session Enhancement

```rust
// In crates/chat-cli/src/agent_env/session.rs

impl Session {
    /// Cancel all active jobs
    pub fn cancel_all_jobs(&self) {
        let jobs = self.jobs.lock().unwrap();
        for job in jobs.iter() {
            job.cancel();
        }
    }
    
    /// Wait for all jobs to complete
    pub async fn wait_for_all_jobs(&self) {
        loop {
            let has_active = {
                let jobs = self.jobs.lock().unwrap();
                jobs.iter().any(|job| !job.is_complete())
            };
            
            if !has_active {
                break;
            }
            
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
```

## Integration with ChatArgs.execute()

### Current Pattern
```rust
// In crates/chat-cli/src/cli/chat/mod.rs
impl ChatArgs {
    pub async fn execute(&self, ...) -> Result<()> {
        // ... existing setup ...
        
        // OLD: Runs existing chat session
        let session = ChatSession::new(...);
        session.run().await?;
        
        Ok(())
    }
}
```

### New Pattern with Agent Env
```rust
impl ChatArgs {
    pub async fn execute(&self, ...) -> Result<()> {
        // ... existing setup ...
        
        // NEW: Check if agent_env mode is enabled
        if self.use_agent_env {
            let session = Session::new(model_providers);
            let ui = AgentEnvUi::new(session, history_path)?;
            
            // This blocks until shutdown completes
            ui.run().await?;
            
            // Returns here when shutdown completes
            return Ok(());
        }
        
        // ... existing chat session logic ...
        Ok(())
    }
}
```

## Shutdown Triggers

### 1. Ctrl+C During PromptTask
```rust
// PromptTask continuation sees Cancelled
shutdown_token.cancel();
```

### 2. Ctrl+C During AgentLoop (Single)
```rust
// Job cancelled, continuation spawns PromptTask
// User can continue or type /quit
```

### 3. Ctrl+C (Double)
```rust
// CtrlCHandler sees double press
shutdown_token.cancel();
```

### 4. /quit Command
```rust
// PromptTask continuation sees /quit
shutdown_token.cancel();
```

### 5. Ctrl+D (EOF)
```rust
// InputHandler returns error
// PromptTask completes with Failed
// Continuation triggers shutdown
```

### 6. Fatal Error
```rust
// Task fails with error
// Continuation can choose to shutdown or continue
```

## Shutdown Sequence Diagram

```
User Action (Ctrl+C, /quit, etc.)
    ↓
shutdown_token.cancel()
    ↓
Continuations check shutdown_token
    ↓
Stop spawning new tasks
    ↓
AgentEnvUi.run() wakes from shutdown_token.cancelled()
    ↓
Call shutdown()
    ↓
    ├─→ Cancel all active jobs
    │       ↓
    │   Wait for jobs to complete (5s timeout)
    │       ↓
    ├─→ Save input history
    │       ↓
    └─→ Return from AgentEnvUi.run()
            ↓
        Return from ChatArgs.execute()
            ↓
        CLI Framework Exits
```

## Error Handling During Shutdown

### Non-Fatal Errors
```rust
// Save history failure - log and continue
if let Err(e) = handler.save_history(&history_path) {
    eprintln!("Warning: Failed to save history: {}", e);
}
```

### Timeout Handling
```rust
// Jobs don't complete within timeout - force exit
tokio::select! {
    _ = session.wait_for_all_jobs() => {}
    _ = tokio::time::sleep(Duration::from_secs(5)) => {
        eprintln!("Warning: Jobs did not complete within timeout");
    }
}
```

## Benefits of Continuation-Based Shutdown

1. **No explicit loop to break** - shutdown naturally stops continuation chain
2. **Parallel-ready** - each worker's continuation chain stops independently
3. **Clean cancellation** - CancellationToken propagates through all tasks
4. **Simple state** - single shutdown_token, no complex state machine
5. **Testable** - can test shutdown at any point in continuation chain

## Testing Considerations

### Test Scenarios
1. Normal shutdown via /quit
2. Shutdown via Ctrl+C during PromptTask
3. Shutdown via double Ctrl+C during AgentLoop
4. Shutdown with active jobs (verify cancellation and wait)
5. Shutdown with history save failure
6. Shutdown timeout (jobs don't respond to cancellation)

### Mock Shutdown
```rust
#[cfg(test)]
impl AgentEnvUi {
    pub fn trigger_shutdown(&self) {
        self.shutdown_token.cancel();
    }
}
```

## Dependencies

Already in project:
- `tokio_util::sync::CancellationToken` - Shutdown signaling
- `tokio::time` - Timeout handling
- `std::time` - Duration

## Implementation Order

1. Create `AgentEnvUi` struct with shutdown_token
2. Implement `run()` method with shutdown wait
3. Implement `shutdown()` sequence
4. Add shutdown checks to continuations
5. Integrate with `ChatArgs.execute()`
6. Add timeout handling
7. Add error handling and logging
8. Add tests
