# Complete Flow Example (Queue-Based)

## Main Entry Point

```rust
// In crates/chat-cli/src/cli/chat/mod.rs
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

## AgentEnvTextUi Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs

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
                
                prompt_queue.enqueue(worker).await;
            }
        })
    }
    
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
            
            // Process next prompt
            let request = match self.prompt_queue.dequeue().await {
                Some(req) => req,
                None => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
            
            // Read input
            let input = match self.input_handler.read_line(&request.worker.name).await {
                Ok(input) => input,
                Err(e) => {
                    eprintln!("Input error: {}", e);
                    break;
                }
            };
            
            // Skip empty
            if input.trim().is_empty() {
                self.prompt_queue.enqueue(request.worker.clone()).await;
                continue;
            }
            
            // Handle /quit
            if input.trim() == "/quit" {
                break;
            }
            
            // Push to conversation
            request.worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
            
            // Launch job
            let agent_input = AgentLoopInput { prompt: input };
            let worker_host_ui = self.create_ui_interface();
            
            let job = match self.session.run_agent_loop(
                request.worker.clone(),
                agent_input,
                worker_host_ui,
            ) {
                Ok(job) => job,
                Err(e) => {
                    eprintln!("Failed to launch agent: {}", e);
                    break;
                }
            };
            
            // Set continuation
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

## TextUiWorkerToHostInterface Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/text_ui_worker_to_host_interface.rs

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

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/text_ui_interface.rs

use std::io::{self, Write};
use uuid::Uuid;
use crate::agent_env::{WorkerToHostInterface, WorkerStates};

pub struct TextUiInterface {}

impl TextUiInterface {
    pub fn new() -> Self {
        Self {}
    }
}

impl WorkerToHostInterface for TextUiInterface {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
        match new_state {
            WorkerStates::Working => print!("\n"),
            _ => {}
        }
    }
    
    fn stream_text(&self, worker_id: Uuid, text: &str) {
        print!("{}", text);
        io::stdout().flush().unwrap();
    }
    
    fn stream_complete(&self, worker_id: Uuid) {
        println!("\n");
    }
}
```

## PromptQueue Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/prompt_queue.rs

use std::sync::Arc;
use std::collections::VecDeque;
use tokio::sync::Mutex;
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
    
    pub async fn enqueue(&self, worker: Arc<Worker>) {
        let request = PromptRequest { worker };
        self.queue.lock().await.push_back(request);
    }
    
    pub async fn dequeue(&self) -> Option<PromptRequest> {
        self.queue.lock().await.pop_front()
    }
}
```

## InputHandler Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs

use rustyline::{Editor, history::FileHistory};
use std::path::PathBuf;

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
    
    pub async fn read_line(&mut self, worker_name: &str) -> Result<String, eyre::Error> {
        let prompt = format!("{}> ", worker_name);
        
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
                Err(eyre::eyre!("Interrupted"))
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                Err(eyre::eyre!("EOF"))
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

## CtrlCHandler Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/ctrl_c_handler.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::signal;
use tokio::sync::Notify;
use crate::agent_env::Session;

pub struct CtrlCHandler {
    last_interrupt_time: Arc<AtomicU64>,
    shutdown_signal: Arc<Notify>,
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
    
    async fn handle_ctrl_c(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let last_time = self.last_interrupt_time.swap(now, Ordering::SeqCst);
        let time_since_last = now.saturating_sub(last_time);
        
        if time_since_last < 1000 {
            // Double Ctrl+C - force exit
            println!("\n^C (Force exit)");
            self.shutdown_signal.notify_one();
        } else {
            // First Ctrl+C - cancel jobs
            println!("\n^C (Cancelling... Press Ctrl+C again to force exit)");
            self.session.cancel_all_jobs();
        }
    }
}
```

## Example Session

```
$ q chat

Q> analyze main.rs
[Agent processes...]
✓ Complete

Q> what functions?
[Agent processes...]
✓ Complete

Q> ^C
Shutting down...
Goodbye!
```

## Example with Job Cancellation

```
Q> analyze all files
[Agent processing...]
^C (Cancelling... Press Ctrl+C again to force exit)
Task cancelled

Q> analyze main.rs only
[Agent processes...]
✓ Complete

Q> /quit
Goodbye!
```

## Example with Force Exit

```
Q> analyze all files
[Agent processing...]
^C (Cancelling... Press Ctrl+C again to force exit)
^C (Force exit)
Goodbye!
```
