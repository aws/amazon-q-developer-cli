# User Input Handling Design (Continuation-Based)

## Problem
Need to prompt user for input between job executions using the continuation mechanism, enabling:
- Seamless transition from AgentLoop → Prompt → AgentLoop
- Support for future parallel worker execution (prompt queueing)
- Proper handling of Ctrl+C, Ctrl+D, and cancellation
- Empty input handling (skip and reprompt)
- Future support for web API alongside TUI

## Architecture Principle

**Separation of Concerns:**
- **Session**: Manages jobs and tasks (long-running autonomous processes)
- **AgentEnvTextUi**: Manages UI state (prompts, display, user interaction)

**NO EXPLICIT LOOPS**. The "loop" is implicit through continuation chains:

```
AgentLoop completes → continuation notifies UI → UI prompts user
User enters input → UI launches AgentLoop → continuation set up
... continues until /quit or cancellation
```

This design keeps Session focused on job management while AgentEnvTextUi handles all UI concerns.

## PromptRequest Queue

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/prompt_queue.rs`

### Structure
```rust
use std::sync::Arc;
use std::collections::VecDeque;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::agent_env::Worker;

pub struct PromptRequest {
    pub worker: Arc<Worker>,
}

pub struct PromptQueue {
    queue: Mutex<VecDeque<PromptRequest>>,
}

impl PromptQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }
    
    /// Add prompt request to queue
    pub async fn enqueue(&self, worker: Arc<Worker>) {
        let request = PromptRequest { worker };
        self.queue.lock().await.push_back(request);
    }
    
    /// Get next prompt request (FIFO)
    pub async fn dequeue(&self) -> Option<PromptRequest> {
        self.queue.lock().await.pop_front()
    }
    
    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }
    
    /// Get current queue length
    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }
}
```

## InputHandler

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs`

### Structure
```rust
use rustyline::{Editor, history::FileHistory};
use std::path::{Path, PathBuf};

pub struct InputHandler {
    editor: Editor<(), FileHistory>,
    history_path: Option<PathBuf>,
}

impl InputHandler {
    pub fn new(history_path: Option<PathBuf>) -> Result<Self, eyre::Error> {
        let mut editor = Editor::<(), FileHistory>::new()?;
        
        if let Some(path) = &history_path {
            let _ = editor.load_history(path);
        }
        
        Ok(Self {
            editor,
            history_path,
        })
    }
    
    /// Read one line of input (blocking)
    /// Returns Err on Ctrl+C or Ctrl+D
    pub async fn read_line(&mut self, worker_name: &str) -> Result<String, eyre::Error> {
        let prompt = format!("{}> ", worker_name);
        
        // Run in blocking task since rustyline is not async
        let mut editor = std::mem::replace(&mut self.editor, Editor::new()?);
        let result = tokio::task::spawn_blocking(move || {
            let res = editor.readline(&prompt);
            (editor, res)
        }).await?;
        
        self.editor = result.0;
        
        match result.1 {
            Ok(line) => {
                if !line.trim().is_empty() {
                    self.editor.add_history_entry(&line)?;
                }
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
    
    pub fn save_history(&mut self) -> Result<(), eyre::Error> {
        if let Some(path) = &self.history_path {
            self.editor.save_history(path)?;
        }
        Ok(())
    }
}
```

## AgentEnvTextUi Main Loop

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs`

### Structure
```rust
use std::sync::Arc;
use tokio::sync::Notify;

use crate::agent_env::{Session, WorkerToHostInterface};
use super::input_handler::InputHandler;
use super::prompt_queue::PromptQueue;
use super::text_ui_worker_to_host_interface::TextUiWorkerToHostInterface;

pub struct AgentEnvTextUi {
    session: Arc<Session>,
    input_handler: InputHandler,
    prompt_queue: Arc<PromptQueue>,
    shutdown_signal: Arc<Notify>,
}

impl AgentEnvTextUi {
    pub fn new(
        session: Arc<Session>,
        history_path: Option<PathBuf>,
    ) -> Result<Self, eyre::Error> {
        Ok(Self {
            session,
            input_handler: InputHandler::new(history_path)?,
            prompt_queue: Arc::new(PromptQueue::new()),
            shutdown_signal: Arc::new(Notify::new()),
        })
    }
    
    /// Get continuation function for job completion
    /// Call this when launching a job to re-queue prompt when done
    pub fn create_agent_completion_continuation(&self) -> WorkerJobContinuationFn {
        let prompt_queue = self.prompt_queue.clone();
        
        Continuations::boxed(move |worker, completion_type, error_msg| {
            let prompt_queue = prompt_queue.clone();
            
            async move {
                match completion_type {
                    WorkerJobCompletionType::Failed => {
                        if let Some(msg) = error_msg {
                            eprintln!("Agent failed: {}", msg);
                        }
                    }
                    WorkerJobCompletionType::Cancelled => {
                        println!("Task cancelled");
                    }
                    WorkerJobCompletionType::Normal => {}
                }
                
                // Re-queue prompt
                prompt_queue.enqueue(worker).await;
            }
        })
    }
    
    /// Create UI interface for worker
    /// Streams output to terminal
    pub fn create_ui_interface(&self) -> Arc<dyn WorkerToHostInterface> {
        Arc::new(TextUiWorkerToHostInterface::new())
    }
    
    pub async fn run(mut self) -> Result<(), eyre::Error> {
        // Setup Ctrl+C handler
        let ctrl_c_handler = Arc::new(CtrlCHandler::new(
            self.shutdown_signal.clone(),
            self.session.clone(),
        ));
        ctrl_c_handler.start_listening();
        
        loop {
            // Check shutdown
            tokio::select! {
                _ = self.shutdown_signal.notified() => break,
                else => {}
            }
            
            // Process next prompt in queue
            let request = match self.prompt_queue.dequeue().await {
                Some(req) => req,
                None => {
                    // No prompts queued, wait
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
            
            // Read user input
            let input = match self.input_handler.read_line(&request.worker.name).await {
                Ok(input) => input,
                Err(e) => {
                    // Ctrl+C or Ctrl+D during prompt = exit
                    eprintln!("Input error: {}", e);
                    break;
                }
            };
            
            // Skip empty input
            if input.trim().is_empty() {
                self.prompt_queue.enqueue(request.worker.clone()).await;
                continue;
            }
            
            // Handle commands
            if input.trim() == "/quit" {
                break;
            }
            
            // Push input to conversation history
            request.worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
            
            // Launch AgentLoop
            let agent_input = AgentLoopInput { prompt: input };
            let ui_interface = self.create_ui_interface();
            
            let job = match self.session.run_agent_loop(
                request.worker.clone(),
                agent_input,
                ui_interface,
            ) {
                Ok(job) => job,
                Err(e) => {
                    eprintln!("Failed to launch agent: {}", e);
                    break;
                }
            };
            
            // Set up continuation to re-queue prompt
            let continuation = self.create_agent_completion_continuation();
            job.worker_job_continuations.add_or_run_now(
                "agent_to_prompt",
                continuation,
                request.worker.clone(),
            ).await;
        }
        
        // Cleanup
        self.session.cancel_all_jobs();
        self.input_handler.save_history()?;
        
        Ok(())
    }
}
```

## TextUiWorkerToHostInterface

### Location
`crates/chat-cli/src/cli/chat/agent_env_ui/text_ui_worker_to_host_interface.rs`

### Structure
```rust
use std::io::{self, Write};
use uuid::Uuid;
use tokio_util::sync::CancellationToken;
use crate::agent_env::{WorkerToHostInterface, WorkerStates, ModelResponseChunk};

pub struct TextUiWorkerToHostInterface {}

impl TextUiWorkerToHostInterface {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl WorkerToHostInterface for TextUiWorkerToHostInterface {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
        log::info!("Worker {} switched to state {:?}", worker_id, new_state);
        
        match new_state {
            WorkerStates::Working => print!("\n"),
            _ => {}
        }
    }
    
    fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk) {
        if let Some(text) = chunk.text {
            print!("{}", text);
            io::stdout().flush().unwrap();
        }
        
        if chunk.stop_reason.is_some() {
            println!("\n");
        }
    }
    
    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, eyre::Error> {
        // For now, auto-approve all tools
        // TODO: Implement interactive confirmation
        Ok("approved".to_string())
    }
}
```

## Entry Point Pattern

### Location
`crates/chat-cli/src/cli/chat/mod.rs`

### Structure
```rust
impl ChatArgs {
    pub async fn execute(&self, os: &Os, telemetry: &Telemetry) -> Result<()> {
        // Setup model providers
        let model_providers = vec![
            Arc::new(BedrockConverseStreamModelProvider::new(/* ... */))
        ];
        
        // Create session
        let session = Arc::new(Session::new(model_providers));
        
        // Get history path
        let history_path = get_history_path()?;
        
        // Create UI
        let ui = AgentEnvTextUi::new(session.clone(), history_path)?;
        
        // Build worker
        let worker = session.build_worker();
        
        // Check if input was provided
        if let Some(input) = &self.input {
            // Stage input in worker's context
            worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
            
            // Launch job with continuation
            let worker_host_ui = ui.create_ui_interface();
            let job = session.run_agent_loop(
                worker.clone(),
                AgentLoopInput {},
                worker_host_ui,
            )?;
            
            let continuation = ui.create_agent_completion_continuation();
            job.worker_job_continuations.add_or_run_now(
                "agent_to_prompt",
                continuation,
                worker.clone(),
            ).await;
        } else {
            // No input - start with prompt
            let continuation = ui.create_agent_completion_continuation();
            continuation(worker, WorkerJobCompletionType::Normal, None).await;
        }
        
        // Run UI (blocks until shutdown)
        ui.run().await
    }
}
```

## Ctrl+C Behavior

### During Prompt (in read_line)
- Ctrl+C causes `read_line()` to return error
- AgentEnvTextUi main loop catches error and exits
- Application shuts down gracefully

### During AgentLoop
- Ctrl+C cancels the job via CancellationToken (handled by ctrl_c_handler)
- AgentLoop completes with Cancelled status
- Continuation re-queues prompt
- User can continue or type /quit

## Multi-Worker Support (Future)

Workers can be added to Session on the fly. When a job completes, its continuation re-queues the prompt:

```rust
// Somewhere in the application, create new worker
let worker2 = session.build_worker();

// Launch job for worker2
let ui_interface = ui.create_ui_interface();
let job = session.run_agent_loop(
    worker2.clone(),
    AgentLoopInput { prompt: "Analyze logs".to_string() },
    ui_interface,
)?;

// Use UI's continuation to re-queue prompt when done
let continuation = ui.create_agent_completion_continuation();
job.worker_job_continuations.add_or_run_now(
    "agent_to_prompt",
    continuation,
    worker2.clone(),
).await;

// worker2's prompt will be queued when job completes
// UI main loop will process it in FIFO order
```

Key points:
- UI doesn't track workers - Session does
- Prompt queue ensures one prompt at a time
- Worker ID/name displayed so user knows which worker
- FIFO ordering ensures fair processing
- Jobs can be running while prompts are queued

## Benefits of This Design

1. **Clean separation** - Session manages jobs, AgentEnvTextUi manages UI
2. **No task for prompting** - prompting is UI concern, not a job
3. **Parallel-ready** - prompt queue naturally handles multiple workers
4. **Web API compatible** - can replace AgentEnvTextUi with AgentEnvWebApi that handles async requests
5. **Simple state** - no complex state machine, just a queue
6. **Testable** - UI logic isolated from job management
7. **Continuation-based** - jobs notify UI when complete via continuations

## Implementation Order

1. Create `PromptQueue` with enqueue/dequeue
2. Create `InputHandler` with readline
3. Create `AgentEnvTextUi` with main loop
4. Wire up continuation from AgentLoop to re-queue prompt
5. Test single iteration (prompt → agent → prompt)
6. Add /quit command handling
7. Add Ctrl+C handling during prompt
8. Add history persistence on shutdown
9. Test with multiple workers (future iteration)
