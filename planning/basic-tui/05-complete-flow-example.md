# Complete Flow Example

This document shows the complete implementation flow with concrete code examples.

## Main Entry Point

```rust
// In crates/chat-cli/src/cli/chat/mod.rs

impl ChatArgs {
    pub async fn execute(
        &self,
        os: &Os,
        telemetry: &Telemetry,
    ) -> Result<()> {
        // ... existing setup code ...
        
        // NEW: Check if agent_env mode is enabled
        if self.use_agent_env {
            // Setup model providers
            let model_providers = vec![
                BedrockConverseStreamModelProvider::new(/* ... */)
            ];
            
            // Create session
            let session = Session::new(model_providers);
            
            // Get history path
            let history_path = get_history_path()?;
            
            // Create and run TUI
            let ui = AgentEnvUi::new(session, history_path)?;
            return ui.run().await;
        }
        
        // ... existing chat session logic ...
        Ok(())
    }
}
```

## AgentEnvUi Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/mod.rs

use std::path::PathBuf;
use std::sync::Arc;

use crate::agent_env::{Session, AgentLoopInput};
use super::demo::CliInterface;

mod input_handler;
mod ctrl_c_handler;
mod shutdown_coordinator;

use input_handler::InputHandler;
use ctrl_c_handler::CtrlCHandler;
use shutdown_coordinator::ShutdownCoordinator;

pub struct AgentEnvUi {
    session: Session,
    input_handler: InputHandler,
    shutdown_coordinator: ShutdownCoordinator,
    ctrl_c_handler: Arc<CtrlCHandler>,
    history_path: PathBuf,
}

impl AgentEnvUi {
    pub fn new(
        session: Session,
        history_path: PathBuf,
    ) -> Result<Self, eyre::Error> {
        let shutdown_coordinator = ShutdownCoordinator::new();
        let ctrl_c_handler = Arc::new(CtrlCHandler::new(
            shutdown_coordinator.token(),
            shutdown_coordinator.hard_stop_token(),
        ));
        let input_handler = InputHandler::new(Some(history_path.clone()))?;
        
        Ok(Self {
            session,
            input_handler,
            shutdown_coordinator,
            ctrl_c_handler,
            history_path,
        })
    }
    
    pub async fn run(mut self) -> Result<(), eyre::Error> {
        // Start Ctrl+C handler
        self.ctrl_c_handler.clone().start_listening();
        
        // Build worker
        let worker = self.session.build_worker();
        let ui_interface = Arc::new(CliInterface::new("\x1b[36m"));
        
        println!("Welcome to Q Agent Environment!");
        println!("Type your request or /quit to exit\n");
        
        // Main loop
        loop {
            // Enter prompt context
            self.ctrl_c_handler.enter_prompt();
            
            // Prompt user for input
            let user_input = match self.input_handler
                .read_prompt(self.shutdown_coordinator.token())
                .await?
            {
                Some(input) => input,
                None => {
                    // User cancelled or shutdown triggered
                    break;
                }
            };
            
            // Check for special commands
            match user_input.as_str() {
                "/quit" | "/exit" => break,
                "/help" => {
                    self.print_help();
                    continue;
                }
                _ => {}
            }
            
            // Cleanup old jobs before spawning new one
            self.session.cleanup_inactive_jobs();
            
            // Spawn new job
            let input = AgentLoopInput {
                user_prompt: user_input,
            };
            
            let job = self.session.run_agent_loop(
                worker.clone(),
                input,
                ui_interface.clone(),
            )?;
            
            // Enter job context
            self.ctrl_c_handler.enter_job(job.cancellation_token.clone());
            
            // Wait for job completion
            let result = job.wait().await;
            
            // Exit job context
            self.ctrl_c_handler.exit_job();
            
            // Check if shutdown was triggered during job
            if self.shutdown_coordinator.is_shutdown() {
                break;
            }
            
            // Handle job result
            match result {
                Ok(()) => println!("\n✓ Task completed\n"),
                Err(e) => {
                    eprintln!("\n✗ Task failed: {}\n", e);
                    // Continue to next prompt unless fatal error
                }
            }
        }
        
        // Perform graceful shutdown
        self.shutdown_coordinator.shutdown(
            &self.session,
            &mut self.input_handler,
            &self.history_path,
        ).await?;
        
        Ok(())
    }
    
    fn print_help(&self) {
        println!("\nAvailable commands:");
        println!("  /quit, /exit  - Exit the application");
        println!("  /help         - Show this help message");
        println!("  Ctrl+C        - Cancel current task or exit");
        println!();
    }
}
```

## Session Enhancements

```rust
// In crates/chat-cli/src/agent_env/session.rs

pub const MAX_INACTIVE_JOBS: usize = 3;

impl Session {
    pub fn cleanup_inactive_jobs(&self) {
        let mut jobs = self.jobs.lock().unwrap();
        
        // Separate active and inactive jobs
        let (active, mut inactive): (Vec<_>, Vec<_>) = jobs
            .iter()
            .cloned()
            .partition(|job| job.is_active());
        
        // Keep only last MAX_INACTIVE_JOBS inactive jobs
        if inactive.len() > MAX_INACTIVE_JOBS {
            let keep_from = inactive.len() - MAX_INACTIVE_JOBS;
            inactive.drain(0..keep_from);
        }
        
        // Rebuild jobs list
        *jobs = active;
        jobs.extend(inactive);
    }
    
    pub fn get_job_counts(&self) -> (usize, usize) {
        let jobs = self.jobs.lock().unwrap();
        let active = jobs.iter().filter(|j| j.is_active()).count();
        let inactive = jobs.len() - active;
        (active, inactive)
    }
    
    /// Wait for all active jobs to complete
    pub async fn wait_for_active_jobs(&self) {
        loop {
            // Check if any jobs are still active
            let has_active = {
                let jobs = self.jobs.lock().unwrap();
                jobs.iter().any(|job| job.is_active())
            };
            
            if !has_active {
                break;
            }
            
            // Small yield to let jobs finish
            tokio::task::yield_now().await;
        }
    }
}
```

## WorkerJob Enhancements

```rust
// In crates/chat-cli/src/agent_env/worker_job.rs

impl WorkerJob {
    pub fn is_active(&self) -> bool {
        if self.cancellation_token.is_cancelled() {
            return false;
        }
        
        match &self.task_handle {
            Some(handle) => !handle.is_finished(),
            None => false,
        }
    }
}
```

## InputHandler Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/input_handler.rs

use std::path::{Path, PathBuf};
use rustyline::{Editor, history::FileHistory};
use tokio_util::sync::CancellationToken;

pub struct InputHandler {
    editor: Editor<(), FileHistory>,
    prompt_text: String,
}

impl InputHandler {
    pub fn new(history_path: Option<PathBuf>) -> Result<Self, eyre::Error> {
        let config = rustyline::Config::builder()
            .max_history_size(1000)?
            .history_ignore_space(true)
            .history_ignore_dups(true)?
            .build();
        
        let mut editor = Editor::with_config(config)?;
        
        if let Some(path) = history_path {
            let _ = editor.load_history(&path);
        }
        
        Ok(Self {
            editor,
            prompt_text: "Q> ".to_string(),
        })
    }
    
    pub async fn read_prompt(
        &mut self,
        cancellation_token: CancellationToken,
    ) -> Result<Option<String>, eyre::Error> {
        loop {
            if cancellation_token.is_cancelled() {
                return Ok(None);
            }
            
            // Read input in blocking task
            let prompt = self.prompt_text.clone();
            let result = {
                let editor = &mut self.editor;
                tokio::task::spawn_blocking(move || {
                    editor.readline(&prompt)
                }).await?
            };
            
            match result {
                Ok(line) => {
                    let trimmed = line.trim();
                    
                    if trimmed.is_empty() {
                        continue;
                    }
                    
                    self.editor.add_history_entry(&line)?;
                    return Ok(Some(trimmed.to_string()));
                }
                Err(rustyline::error::ReadlineError::Interrupted) => {
                    return Ok(None);
                }
                Err(rustyline::error::ReadlineError::Eof) => {
                    return Ok(None);
                }
                Err(e) => {
                    return Err(eyre::eyre!("Input error: {}", e));
                }
            }
        }
    }
    
    pub fn save_history(&mut self, path: &Path) -> Result<(), eyre::Error> {
        self.editor.save_history(path)?;
        Ok(())
    }
}
```

## CtrlCHandler Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/ctrl_c_handler.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::signal;
use tokio_util::sync::CancellationToken;

pub struct CtrlCHandler {
    in_prompt: Arc<AtomicBool>,
    job_interrupt_count: Arc<AtomicU64>,
    last_interrupt_time: Arc<AtomicU64>,
    shutdown_token: CancellationToken,
    hard_stop_token: CancellationToken,
    current_job_token: Arc<Mutex<Option<CancellationToken>>>,
}

impl CtrlCHandler {
    pub fn new(shutdown_token: CancellationToken, hard_stop_token: CancellationToken) -> Self {
        Self {
            in_prompt: Arc::new(AtomicBool::new(false)),
            job_interrupt_count: Arc::new(AtomicU64::new(0)),
            last_interrupt_time: Arc::new(AtomicU64::new(0)),
            shutdown_token,
            hard_stop_token,
            current_job_token: Arc::new(Mutex::new(None)),
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
                        eprintln!("Error in Ctrl+C handler: {}", e);
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
        
        // Check if already shutting down - trigger hard stop
        if self.shutdown_token.is_cancelled() {
            println!("\n^C (Force exit)");
            self.hard_stop_token.cancel();
            return;
        }
        
        if self.in_prompt.load(Ordering::SeqCst) {
            // In prompt - exit immediately
            println!("\n^C");
            self.shutdown_token.cancel();
        } else {
            // In job - cancel job or exit
            let count = self.job_interrupt_count.fetch_add(1, Ordering::SeqCst) + 1;
            
            if count == 1 {
                println!("\n^C (Cancelling task... Press Ctrl+C again to exit)");
                if let Some(token) = self.current_job_token.lock().unwrap().as_ref() {
                    token.cancel();
                }
            } else if time_since_last < 1000 {
                println!("\n^C (Force exit)");
                self.shutdown_token.cancel();
            } else {
                println!("\n^C (Cancelling task... Press Ctrl+C again to exit)");
                self.job_interrupt_count.store(1, Ordering::SeqCst);
                if let Some(token) = self.current_job_token.lock().unwrap().as_ref() {
                    token.cancel();
                }
            }
        }
    }
    
    pub fn enter_prompt(&self) {
        self.in_prompt.store(true, Ordering::SeqCst);
        self.job_interrupt_count.store(0, Ordering::SeqCst);
    }
    
    pub fn enter_job(&self, job_token: CancellationToken) {
        self.in_prompt.store(false, Ordering::SeqCst);
        self.job_interrupt_count.store(0, Ordering::SeqCst);
        *self.current_job_token.lock().unwrap() = Some(job_token);
    }
    
    pub fn exit_job(&self) {
        *self.current_job_token.lock().unwrap() = None;
        self.job_interrupt_count.store(0, Ordering::SeqCst);
    }
}
```

## ShutdownCoordinator Implementation

```rust
// In crates/chat-cli/src/cli/chat/agent_env_ui/shutdown_coordinator.rs

use std::path::Path;
use tokio_util::sync::CancellationToken;

use crate::agent_env::Session;
use super::input_handler::InputHandler;

pub struct ShutdownCoordinator {
    shutdown_token: CancellationToken,
    hard_stop_token: CancellationToken,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            shutdown_token: CancellationToken::new(),
            hard_stop_token: CancellationToken::new(),
        }
    }
    
    pub fn token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }
    
    pub fn hard_stop_token(&self) -> CancellationToken {
        self.hard_stop_token.clone()
    }
    
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_token.is_cancelled()
    }
    
    pub async fn shutdown(
        &self,
        session: &Session,
        input_handler: &mut InputHandler,
        history_path: &Path,
    ) -> Result<(), eyre::Error> {
        println!("\nShutting down gracefully...");
        
        // Cancel all active jobs
        session.cancel_all_jobs();
        
        // Wait for jobs to finish (or hard stop)
        tokio::select! {
            _ = session.wait_for_active_jobs() => {}
            _ = self.hard_stop_token.cancelled() => {
                println!("Force exit");
                return Ok(());
            }
        }
        
        // Save history
        if let Err(e) = input_handler.save_history(history_path) {
            eprintln!("Warning: Failed to save history: {}", e);
        }
        
        println!("Goodbye!");
        Ok(())
    }
}
```

## Example User Session

```
$ q chat --agent-env

Welcome to Q Agent Environment!
Type your request or /quit to exit

Q> analyze the code in main.rs
[Agent processes request...]
[Streams response...]
✓ Task completed

Q> what are the main functions?
[Agent processes request...]
[Streams response...]
✓ Task completed

Q> ^C
Shutting down gracefully...
Goodbye!
```

## Example with Job Cancellation

```
Q> analyze all files in the repository
[Agent starts processing...]
[Long-running task...]
^C (Cancelling task... Press Ctrl+C again to exit)
[Task cancels...]
✗ Task failed: Operation cancelled

Q> analyze just main.rs
[Agent processes request...]
✓ Task completed

Q> /quit
Shutting down gracefully...
Goodbye!
```

## Example with Force Exit

```
Q> analyze all files in the repository
[Agent starts processing...]
[Long-running task...]
^C (Cancelling task... Press Ctrl+C again to exit)
^C (Force exit)
Shutting down gracefully...
Warning: 1 jobs did not finish within timeout
Goodbye!
```
