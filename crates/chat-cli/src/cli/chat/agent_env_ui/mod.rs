mod prompt_queue;
mod input_handler;
mod text_ui_worker_to_host_interface;
mod ctrl_c_handler;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::agent_env::{Session, WorkerToHostInterface, WorkerJobCompletionType};
use crate::agent_env::worker_tasks::AgentLoopInput;
use crate::agent_env::worker_job_continuations::WorkerJobContinuationFn;
use prompt_queue::PromptQueue;
use input_handler::InputHandler;
use text_ui_worker_to_host_interface::TextUiWorkerToHostInterface;
use ctrl_c_handler::CtrlCHandler;

pub struct AgentEnvTextUi {
    session: Arc<Session>,
    input_handler: InputHandler,
    prompt_queue: Arc<PromptQueue>,
    shutdown_signal: Arc<Notify>,
    worker_interfaces: Arc<Mutex<HashMap<Uuid, Arc<dyn WorkerToHostInterface>>>>,
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
            worker_interfaces: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Get continuation function for job completion
    /// Call this when launching a job to re-queue prompt when done
    pub fn create_agent_completion_continuation(&self) -> WorkerJobContinuationFn {
        let prompt_queue = self.prompt_queue.clone();
        
        tracing::debug!("Creating agent completion continuation");
        
        crate::agent_env::Continuations::boxed(move |worker, completion_type, error_msg| {
            let prompt_queue = prompt_queue.clone();
            
            async move {
                tracing::debug!("Agent completion continuation invoked for worker {}", worker.id);
                
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
    
    /// Get or create UI interface for worker
    /// If worker_id is provided, checks map first and reuses existing interface
    /// If color is provided, creates colored interface
    pub fn get_worker_interface(
        &self,
        worker_id: Option<Uuid>,
        color: Option<&'static str>,
    ) -> Arc<dyn WorkerToHostInterface> {
        if let Some(id) = worker_id {
            let mut interfaces = self.worker_interfaces.lock().unwrap();
            
            // Check if interface already exists
            if let Some(interface) = interfaces.get(&id) {
                return interface.clone();
            }
            
            // Create and store new interface
            let interface = Arc::new(TextUiWorkerToHostInterface::new(color));
            interfaces.insert(id, interface.clone());
            interface
        } else {
            // No worker_id, create without storing
            Arc::new(TextUiWorkerToHostInterface::new(color))
        }
    }
    
    pub async fn run(mut self) -> Result<(), eyre::Error> {
        tracing::debug!("Running AgentEnvTextUi");
        // Setup Ctrl+C handler
        let ctrl_c_handler = Arc::new(CtrlCHandler::new(
            self.shutdown_signal.clone(),
            self.session.clone(),
        ));
        ctrl_c_handler.start_listening();
        tracing::debug!("CtrlCHandler Launched");

        loop {
            // Wait for items in queue or shutdown signal
            tokio::select! {
                _ = self.shutdown_signal.notified() => {
                    tracing::debug!("Shutdown signal received");
                    break;
                }
                _ = self.prompt_queue.wait_for_items() => {
                    // Items available, continue to dequeue
                }
            }
            
            // Process next prompt in queue
            let request = match self.prompt_queue.dequeue().await {
                Some(req) => {
                    tracing::debug!("Dequeued prompt request for worker {}", req.worker.id);
                    req
                },
                None => {
                    // Race condition: item was dequeued by another task
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
            
            // Cleanup old jobs before spawning new one
            self.session.cleanup_inactive_jobs();
            
            // Push input to conversation history
            request.worker.context_container
                .conversation_history
                .lock()
                .unwrap()
                .push_input_message(input.clone());
            
            // Launch AgentLoop
            let agent_input = AgentLoopInput {};
            let ui_interface = self.get_worker_interface(Some(request.worker.id), None);
            
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
