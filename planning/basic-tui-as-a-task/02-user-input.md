# User Input Handling Design (Continuation-Based)

## Problem
Need to prompt user for input between job executions using the continuation mechanism, enabling:
- Seamless transition from AgentLoop → PromptTask → AgentLoop
- Support for future parallel worker execution
- Proper handling of Ctrl+C, Ctrl+D, and cancellation
- Empty input handling (skip and reprompt)

## Architecture Principle

**NO EXPLICIT LOOPS**. The "loop" is implicit through continuation chains:

```
AgentLoop completes → continuation spawns PromptTask
PromptTask completes → continuation spawns AgentLoop
... continues until /quit or cancellation
```

This design enables multiple workers to run independently with their own continuation chains.

## PromptTask Implementation

### Location
`crates/chat-cli/src/agent_env/worker_tasks/prompt_task.rs`

### Structure
```rust
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use rustyline::{Editor, history::FileHistory};

pub struct PromptTask {
    worker: Arc<Worker>,
    input_handler: Arc<tokio::sync::Mutex<InputHandler>>,
    cancellation_token: CancellationToken,
}

pub struct PromptTaskOutput {
    pub user_input: String,
}

impl PromptTask {
    pub fn new(
        worker: Arc<Worker>,
        input_handler: Arc<tokio::sync::Mutex<InputHandler>>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            input_handler,
            cancellation_token,
        }
    }
}

#[async_trait::async_trait]
impl WorkerTask for PromptTask {
    fn get_worker(&self) -> Arc<Worker> {
        Arc::clone(&self.worker)
    }
    
    async fn run(&self) -> Result<(), eyre::Error> {
        let mut handler = self.input_handler.lock().await;
        
        loop {
            if self.cancellation_token.is_cancelled() {
                return Err(eyre::eyre!("Cancelled"));
            }
            
            // Read input (blocking operation)
            let input = handler.read_line().await?;
            
            // Skip empty input
            if input.trim().is_empty() {
                continue;
            }
            
            // Store result for continuation to access
            // TODO: Need mechanism to pass data to continuation
            return Ok(());
        }
    }
}
```

## InputHandler (Simplified)

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs`

### Structure
```rust
use rustyline::{Editor, history::FileHistory};

pub struct InputHandler {
    editor: Editor<(), FileHistory>,
    prompt_text: String,
    last_input: Option<String>,
}

impl InputHandler {
    pub fn new(history_path: Option<PathBuf>) -> Result<Self, eyre::Error> {
        let mut editor = Editor::<(), FileHistory>::new()?;
        
        if let Some(path) = &history_path {
            let _ = editor.load_history(path);
        }
        
        Ok(Self {
            editor,
            prompt_text: "Q> ".to_string(),
            last_input: None,
        })
    }
    
    /// Read one line of input (blocking)
    /// Returns Err on Ctrl+C or Ctrl+D
    pub async fn read_line(&mut self) -> Result<String, eyre::Error> {
        let prompt = self.prompt_text.clone();
        let editor = &mut self.editor;
        
        // Run in blocking task since rustyline is not async
        let result = tokio::task::spawn_blocking(move || {
            editor.readline(&prompt)
        }).await?;
        
        match result {
            Ok(line) => {
                self.editor.add_history_entry(&line)?;
                self.last_input = Some(line.clone());
                Ok(line)
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                Err(eyre::eyre!("User interrupted (Ctrl+C)"))
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                Err(eyre::eyre!("User interrupted (Ctrl+D)"))
            }
            Err(e) => {
                Err(eyre::eyre!("Input error: {}", e))
            }
        }
    }
    
    pub fn get_last_input(&self) -> Option<&str> {
        self.last_input.as_deref()
    }
    
    pub fn save_history(&mut self, path: &Path) -> Result<(), eyre::Error> {
        self.editor.save_history(path)?;
        Ok(())
    }
}
```

## Continuation Chain Setup

### Initial Launch
```rust
// In AgentEnvUi::run()
pub async fn run(self) -> Result<(), eyre::Error> {
    let input_handler = Arc::new(tokio::sync::Mutex::new(
        InputHandler::new(self.history_path.clone())?
    ));
    
    // Start with PromptTask
    let prompt_task = PromptTask::new(
        self.worker.clone(),
        input_handler.clone(),
        self.cancellation_token.clone(),
    );
    
    let mut job = self.session.launch_task(
        self.worker.clone(),
        Arc::new(prompt_task),
    )?;
    
    // Set up continuation: PromptTask → AgentLoop
    let continuation = self.create_prompt_to_agent_continuation(input_handler.clone());
    job.worker_job_continuations.add_or_run_now(
        "prompt_to_agent",
        continuation,
        self.worker.clone(),
    ).await;
    
    job.launch();
    
    // Wait for shutdown signal
    self.shutdown_signal.notified().await;
    
    // Cancel all jobs
    self.session.cancel_all_jobs();
    
    Ok(())
}
```

### PromptTask → AgentLoop Continuation
```rust
fn create_prompt_to_agent_continuation(
    &self,
    input_handler: Arc<tokio::sync::Mutex<InputHandler>>,
) -> WorkerJobContinuationFn {
    let session = self.session.clone();
    let worker = self.worker.clone();
    let cancellation_token = self.cancellation_token.clone();
    let shutdown_signal = self.shutdown_signal.clone();
    
    Continuations::boxed(move |worker, completion_type, error_msg| {
        let session = session.clone();
        let worker = worker.clone();
        let input_handler = input_handler.clone();
        let cancellation_token = cancellation_token.clone();
        let shutdown_signal = shutdown_signal.clone();
        
        async move {
            match completion_type {
                WorkerJobCompletionType::Cancelled => {
                    // User pressed Ctrl+C - exit
                    shutdown_signal.notify_one();
                    return;
                }
                WorkerJobCompletionType::Failed => {
                    if let Some(msg) = error_msg {
                        eprintln!("Input error: {}", msg);
                    }
                    shutdown_signal.notify_one();
                    return;
                }
                WorkerJobCompletionType::Normal => {
                    // Get user input
                    let input = {
                        let handler = input_handler.lock().await;
                        match handler.get_last_input() {
                            Some(input) => input.to_string(),
                            None => {
                                shutdown_signal.notify_one();
                                return;
                            }
                        }
                    };
                    
                    // Check for /quit command
                    if input.trim() == "/quit" {
                        shutdown_signal.notify_one();
                        return;
                    }
                    
                    // Launch AgentLoop with user input
                    let agent_input = AgentLoopInput {
                        prompt: input,
                    };
                    
                    let mut job = match session.run_agent_loop(
                        worker.clone(),
                        agent_input,
                        ui_interface,
                    ) {
                        Ok(job) => job,
                        Err(e) => {
                            eprintln!("Failed to launch agent: {}", e);
                            shutdown_signal.notify_one();
                            return;
                        }
                    };
                    
                    // Set up continuation: AgentLoop → PromptTask
                    let continuation = create_agent_to_prompt_continuation(
                        session.clone(),
                        worker.clone(),
                        input_handler.clone(),
                        cancellation_token.clone(),
                        shutdown_signal.clone(),
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
    cancellation_token: CancellationToken,
    shutdown_signal: Arc<tokio::sync::Notify>,
) -> WorkerJobContinuationFn {
    Continuations::boxed(move |worker, completion_type, error_msg| {
        let session = session.clone();
        let worker = worker.clone();
        let input_handler = input_handler.clone();
        let cancellation_token = cancellation_token.clone();
        let shutdown_signal = shutdown_signal.clone();
        
        async move {
            match completion_type {
                WorkerJobCompletionType::Failed => {
                    if let Some(msg) = error_msg {
                        eprintln!("Agent failed: {}", msg);
                    }
                    // Continue to prompt anyway
                }
                WorkerJobCompletionType::Cancelled => {
                    // Job was cancelled, but continue to prompt
                    println!("Task cancelled");
                }
                WorkerJobCompletionType::Normal => {
                    // Normal completion
                }
            }
            
            // Launch PromptTask
            let prompt_task = PromptTask::new(
                worker.clone(),
                input_handler.clone(),
                cancellation_token.clone(),
            );
            
            let mut job = match session.launch_task(
                worker.clone(),
                Arc::new(prompt_task),
            ) {
                Ok(job) => job,
                Err(e) => {
                    eprintln!("Failed to launch prompt: {}", e);
                    shutdown_signal.notify_one();
                    return;
                }
            };
            
            // Set up continuation: PromptTask → AgentLoop
            let continuation = create_prompt_to_agent_continuation(
                session.clone(),
                worker.clone(),
                input_handler.clone(),
                cancellation_token.clone(),
                shutdown_signal.clone(),
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

## Ctrl+C Behavior

### During PromptTask
- Ctrl+C causes `read_line()` to return error
- PromptTask completes with Failed status
- Continuation sees Failed → triggers shutdown
- Application exits

### During AgentLoop
- Ctrl+C cancels the job via CancellationToken
- AgentLoop completes with Cancelled status
- Continuation sees Cancelled → launches PromptTask
- User can continue or type /quit

## Special Commands

Only `/quit` is supported in first iteration:
```rust
if input.trim() == "/quit" {
    shutdown_signal.notify_one();
    return;
}
```

## Benefits of This Design

1. **No explicit loops** - continuation chain creates implicit loop
2. **Parallel-ready** - each worker can have independent continuation chain
3. **Clean cancellation** - CancellationToken propagates through chain
4. **Simple state** - no complex state machine in UI layer
5. **Testable** - each task and continuation can be tested independently

## Implementation Order

1. Create `InputHandler` with basic readline
2. Create `PromptTask` implementing WorkerTask
3. Create continuation helper functions
4. Wire up initial launch in AgentEnvUi
5. Test single iteration (prompt → agent → prompt)
6. Add /quit command handling
7. Add Ctrl+C handling
8. Add history persistence on shutdown
