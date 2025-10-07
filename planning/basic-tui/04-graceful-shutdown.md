# Graceful Shutdown Design (Queue-Based)

## Problem
When user exits (Ctrl+C at prompt, /quit, Ctrl+D), we need to:
1. Stop processing prompt queue
2. Cancel all active jobs
3. Wait for jobs to finish cancellation
4. Save history
5. Return from `ChatArgs.execute()`

## Architecture

AgentEnvTextUi main loop checks shutdown signal and exits cleanly.

### Shutdown Flow
```
Shutdown Trigger (Ctrl+C at prompt, /quit, Ctrl+D)
    ↓
AgentEnvTextUi main loop exits
    ↓
Cancel all active jobs
    ↓
Save history
    ↓
Return from AgentEnvTextUi.run()
```

## AgentEnvTextUi Structure

```rust
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
            
            // Read input (Ctrl+C here exits via error)
            let input = match self.input_handler.read_line(&request.worker.name).await {
                Ok(input) => input,
                Err(_) => break,  // Ctrl+C or Ctrl+D
            };
            
            if input.trim().is_empty() {
                self.prompt_queue.enqueue(request.worker.clone()).await;
                continue;
            }
            
            if input.trim() == "/quit" {
                break;
            }
            
            // Push to conversation history
            request.worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
            
            // Launch job
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
            
            // Set continuation to re-queue prompt
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
