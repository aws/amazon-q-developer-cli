# Ctrl+C Handling Design (Continuation-Based)

## Problem
Ctrl+C needs to integrate with continuation-based architecture:
1. **During Prompt (in AgentEnvTextUi)**: Exit application
2. **During AgentLoop**: Cancel job (continuation re-queues prompt)
3. **Second Ctrl+C**: Force exit regardless of context
4. **During shutdown**: Force immediate exit (hard stop)

## Requirements
- Cancel jobs gracefully via CancellationToken
- Allow quick exit with double Ctrl+C (within 1 second)
- No explicit context tracking (continuations handle flow)
- Trigger shutdown signal to stop UI loop

## Architecture

### Cancellation Flow
```
Ctrl+C Signal
    ↓
Check if already shutting down
    ↓ No
Check time since last Ctrl+C
    ↓ > 1 second (first press)
Cancel current job's CancellationToken
    ↓
Job completes with Cancelled status
    ↓
Continuation decides next action:
    - During prompt → exit (handled by InputHandler)
    - AgentLoop cancelled → re-queue prompt
```

### Double Ctrl+C Flow
```
Ctrl+C Signal
    ↓
Check time since last Ctrl+C
    ↓ < 1 second (second press)
Trigger shutdown signal
    ↓
AgentEnvTextUi sees shutdown
    ↓
Stop processing prompts
    ↓
Application exits
```

## CtrlCHandler Design

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/ctrl_c_handler.rs`

### Structure
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::signal;
use tokio::sync::Notify;

pub struct CtrlCHandler {
    // Timestamp of last Ctrl+C (for double-press detection)
    last_interrupt_time: Arc<AtomicU64>,
    
    // Shutdown signal (stops continuation chain)
    shutdown_signal: Arc<Notify>,
    
    // Session reference (to cancel all jobs)
    session: Arc<Session>,
}

impl CtrlCHandler {
    pub fn new(shutdown_signal: Arc<Notify>, session: Arc<Session>) -> Self {
        Self {
            last_interrupt_time: Arc::new(AtomicU64::new(0)),
            shutdown_signal,
            session,
        }
    }
    
    /// Start listening for Ctrl+C signals
    pub fn start_listening(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                match signal::ctrl_c().await {
                    Ok(()) => {
                        self.handle_ctrl_c().await;
                    }
                    Err(e) => {
                        eprintln!("Error setting up Ctrl+C handler: {}", e);
                        break;
                    }
                }
            }
        });
    }
    
    /// Handle Ctrl+C signal
    async fn handle_ctrl_c(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let last_time = self.last_interrupt_time.swap(now, Ordering::SeqCst);
        let time_since_last = now.saturating_sub(last_time);
        
        if time_since_last < 1000 {
            // Double Ctrl+C (within 1 second) - force exit
            println!("\n^C (Force exit)");
            self.shutdown_signal.notify_one();
        } else {
            // First Ctrl+C - cancel all active jobs
            println!("\n^C (Cancelling... Press Ctrl+C again to force exit)");
            self.session.cancel_all_jobs();
        }
    }
}
```

## Integration with AgentEnvTextUi

### Setup
```rust
// In AgentEnvTextUi::run()
pub async fn run(mut self) -> Result<(), eyre::Error> {
    let ctrl_c_handler = Arc::new(CtrlCHandler::new(
        self.shutdown_signal.clone(),
        self.session.clone(),
    ));
    
    // Start listening for Ctrl+C
    ctrl_c_handler.start_listening();
    
    // Start with initial prompt
    self.prompt_queue.enqueue(self.worker.clone()).await;
    
    loop {
        // Check for shutdown
        tokio::select! {
            _ = self.shutdown_signal.notified() => break,
            else => {}
        }
        
        // Process next prompt in queue
        let request = match self.prompt_queue.dequeue().await {
            Some(req) => req,
            None => {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        
        // Read user input (Ctrl+C here exits via InputHandler error)
        let input = match self.input_handler.read_line(&request.worker.name).await {
            Ok(input) => input,
            Err(e) => {
                eprintln!("Input error: {}", e);
                break;
            }
        };
        
        // ... rest of processing ...
    }
    
    // Cleanup
    self.session.cancel_all_jobs();
    self.input_handler.save_history()?;
    
    Ok(())
}
```

## Continuation Integration

### AgentLoop Continuation (in AgentEnvTextUi)
```rust
// When AgentLoop completes
fn create_agent_completion_continuation(&self) -> WorkerJobContinuationFn {
    let prompt_queue = self.prompt_queue.clone();
    
    Continuations::boxed(move |worker, completion_type, error_msg| {
        let prompt_queue = prompt_queue.clone();
        
        async move {
            match completion_type {
                WorkerJobCompletionType::Cancelled => {
                    println!("Task cancelled");
                    // Re-queue prompt to continue
                    prompt_queue.enqueue(worker).await;
                }
                WorkerJobCompletionType::Normal => {
                    // Normal completion - re-queue prompt
                    prompt_queue.enqueue(worker).await;
                }
                WorkerJobCompletionType::Failed => {
                    if let Some(msg) = error_msg {
                        eprintln!("Agent failed: {}", msg);
                    }
                    // Re-queue prompt to continue
                    prompt_queue.enqueue(worker).await;
                }
            }
        }
    })
}
```

Note: Continuations don't need to check shutdown state - the main loop in AgentEnvTextUi handles that. Continuations just re-queue prompts, and if shutdown is triggered, the main loop will exit before processing them.

## Behavior Examples

### Scenario 1: Ctrl+C During Prompt
```
Q> [user typing...]
^C
[InputHandler returns error]
[AgentEnvTextUi main loop exits]
[Application exits]
```

### Scenario 2: Single Ctrl+C During AgentLoop
```
Q> analyze this code
[AgentLoop running...]
^C (Cancelling... Press Ctrl+C again to force exit)
[AgentLoop cancelled]
[Continuation re-queues prompt]
Task cancelled

Q> [back to prompt]
```

### Scenario 3: Double Ctrl+C
```
Q> analyze this code
[AgentLoop running...]
^C (Cancelling... Press Ctrl+C again to force exit)
^C (Force exit)
[Shutdown signal triggered]
[AgentEnvTextUi main loop exits]
[All jobs cancelled]
[Application exits]
```

### Scenario 4: /quit Command
```
Q> /quit
[AgentEnvTextUi sees /quit → breaks loop]
[Application exits]
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

## Shutdown Detection

AgentEnvTextUi uses `Arc<Notify>` for shutdown signal:

```rust
pub struct AgentEnvTextUi {
    shutdown_signal: Arc<Notify>,
    // ...
}

// In main loop
loop {
    tokio::select! {
        _ = self.shutdown_signal.notified() => break,
        else => {
            // Process prompts
        }
    }
}
```

Continuations don't need to check shutdown - they just re-queue prompts. The main loop handles shutdown detection and stops processing the queue.

## Edge Cases

### 1. Ctrl+C During Job Spawn
- Job not yet started
- CancellationToken not yet created
- Solution: Session.cancel_all_jobs() is safe to call anytime

### 2. Rapid Multiple Ctrl+C
- User mashes Ctrl+C
- Solution: Time-based detection (1 second window)

### 3. Ctrl+C After Shutdown Started
- Already shutting down
- Solution: Main loop checks shutdown_signal before processing prompts

### 4. Job Doesn't Respond to Cancellation
- Job hangs despite cancellation
- Solution: Hard timeout in cleanup (5 seconds)

### 5. Ctrl+C While Prompt Queued But Not Active
- Prompt in queue, not yet displayed
- Solution: Main loop exits, queued prompts never processed

## Testing Considerations

### Test Scenarios
1. Ctrl+C during prompt exits immediately
2. Ctrl+C during AgentLoop cancels and returns to prompt
3. Double Ctrl+C exits immediately
4. /quit command exits gracefully
5. Ctrl+C after shutdown started is no-op
6. Prompt queued but not active when shutdown triggered

### Mock Signal Handler
```rust
#[cfg(test)]
impl CtrlCHandler {
    pub async fn simulate_ctrl_c(&self) {
        self.handle_ctrl_c().await;
    }
}
```

## Dependencies

Already in project:
- `tokio::signal` - Ctrl+C signal handling
- `tokio_util::sync::CancellationToken` - Shutdown coordination
- `std::sync::atomic` - Thread-safe timestamp tracking

## Implementation Order

1. Create `CtrlCHandler` struct with timestamp tracking
2. Implement signal listener with `tokio::signal::ctrl_c()`
3. Add double-press detection with timing
4. Integrate with Session.cancel_all_jobs()
5. Update continuations to check shutdown_token
6. Add tests
7. Add UX polish (messages, colors)
