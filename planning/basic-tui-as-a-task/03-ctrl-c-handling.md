# Ctrl+C Handling Design (Continuation-Based)

## Problem
Ctrl+C needs to integrate with continuation-based architecture:
1. **During PromptTask**: Exit application (break continuation chain)
2. **During AgentLoop**: Cancel job (continuation spawns PromptTask)
3. **Second Ctrl+C**: Force exit regardless of context
4. **During shutdown**: Force immediate exit (hard stop)

## Requirements
- Cancel jobs gracefully via CancellationToken
- Allow quick exit with double Ctrl+C (within 1 second)
- No explicit context tracking (continuations handle flow)
- Trigger shutdown signal to stop continuation chain

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
    - PromptTask cancelled → trigger shutdown
    - AgentLoop cancelled → spawn PromptTask
```

### Double Ctrl+C Flow
```
Ctrl+C Signal
    ↓
Check time since last Ctrl+C
    ↓ < 1 second (second press)
Trigger shutdown signal
    ↓
All continuations see shutdown
    ↓
Stop spawning new tasks
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

## Integration with AgentEnvUi

### Setup
```rust
// In AgentEnvUi::run()
pub async fn run(mut self) -> Result<(), eyre::Error> {
    let shutdown_signal = Arc::new(Notify::new());
    let session = Arc::new(self.session);
    
    let ctrl_c_handler = Arc::new(CtrlCHandler::new(
        shutdown_signal.clone(),
        session.clone(),
    ));
    
    // Start listening for Ctrl+C
    ctrl_c_handler.start_listening();
    
    // Launch initial PromptTask with continuations
    self.launch_initial_prompt(shutdown_signal.clone()).await?;
    
    // Wait for shutdown signal
    shutdown_signal.notified().await;
    
    // Cancel all jobs (if not already cancelled)
    session.cancel_all_jobs();
    
    // Wait for jobs to complete
    session.wait_for_all_jobs().await;
    
    // Save history
    self.input_handler.save_history(&self.history_path)?;
    
    Ok(())
}
```

## Continuation Integration

### PromptTask Continuation
```rust
// When PromptTask completes
match completion_type {
    WorkerJobCompletionType::Cancelled => {
        // User pressed Ctrl+C during prompt - trigger shutdown
        shutdown_signal.notify_one();
        return;
    }
    WorkerJobCompletionType::Normal => {
        // Got user input - check for /quit
        if input == "/quit" {
            shutdown_signal.notify_one();
            return;
        }
        
        // Launch AgentLoop
        // ...
    }
    WorkerJobCompletionType::Failed => {
        // Error during input - trigger shutdown
        eprintln!("Input error: {}", error_msg.unwrap_or_default());
        shutdown_signal.notify_one();
        return;
    }
}
```

### AgentLoop Continuation
```rust
// When AgentLoop completes
match completion_type {
    WorkerJobCompletionType::Cancelled => {
        // Job was cancelled - check if shutting down
        // (This check prevents spawning PromptTask during shutdown)
        if is_shutting_down() {
            return;
        }
        
        println!("Task cancelled");
        // Launch PromptTask to continue
        // ...
    }
    WorkerJobCompletionType::Normal | WorkerJobCompletionType::Failed => {
        // Check if shutting down
        if is_shutting_down() {
            return;
        }
        
        // Launch PromptTask to continue
        // ...
    }
}
```

## Behavior Examples

### Scenario 1: Ctrl+C During PromptTask
```
Q> [user typing...]
^C (Cancelling... Press Ctrl+C again to force exit)
[PromptTask cancelled]
[Continuation sees Cancelled → triggers shutdown]
[Application exits]
```

### Scenario 2: Single Ctrl+C During AgentLoop
```
Q> analyze this code
[AgentLoop running...]
^C (Cancelling... Press Ctrl+C again to force exit)
[AgentLoop cancelled]
[Continuation sees Cancelled → spawns PromptTask]
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
[All jobs cancelled]
[Application exits]
```

### Scenario 4: /quit Command
```
Q> /quit
[Continuation sees /quit → triggers shutdown]
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

## Shutdown Detection in Continuations

### Approach 1: Check Shutdown Signal
```rust
// Pass shutdown_signal to continuation
if shutdown_signal.is_notified() {
    return;  // Don't spawn next task
}
```

**Problem**: `Notify` doesn't have `is_notified()` method

### Approach 2: Use CancellationToken
```rust
// Use CancellationToken instead of Notify
let shutdown_token = CancellationToken::new();

// In continuation
if shutdown_token.is_cancelled() {
    return;  // Don't spawn next task
}

// In Ctrl+C handler
shutdown_token.cancel();
```

**Recommended**: Use CancellationToken for shutdown signal

### Updated Structure
```rust
pub struct CtrlCHandler {
    last_interrupt_time: Arc<AtomicU64>,
    shutdown_token: CancellationToken,  // Changed from Notify
    session: Arc<Session>,
}

// In continuation
if shutdown_token.is_cancelled() {
    return;  // Don't spawn next task
}
```

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
- Solution: Continuations check shutdown_token before spawning

### 4. Job Doesn't Respond to Cancellation
- Job hangs despite cancellation
- Solution: Hard timeout in wait_for_all_jobs() (5 seconds)

## Testing Considerations

### Test Scenarios
1. Ctrl+C during PromptTask exits immediately
2. Ctrl+C during AgentLoop cancels and returns to prompt
3. Double Ctrl+C exits immediately
4. /quit command exits gracefully
5. Ctrl+C after shutdown started is no-op

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
