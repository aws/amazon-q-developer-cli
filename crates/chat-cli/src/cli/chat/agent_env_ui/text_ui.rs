use crate::agent_env::{
    AgentEnvironmentEvent, AgentEnvironmentCommand, Command, PromptResult,
    Session, WorkerEvent, JobEvent, OutputChunk, WorkerLifecycleState,
    CommandParser,
};
use super::{InputHandler, ui_utils};
use tokio::sync::{mpsc, Notify};
use std::sync::Arc;
use std::path::PathBuf;
use std::io::{self, Write};
use uuid::Uuid;
use async_trait::async_trait;
use tokio::task::JoinHandle;

/// Text-based UI with readline-style input and streaming output
pub struct TextUi {
    session: Arc<Session>,
    main_worker_id: Uuid,
    input_handler: Arc<tokio::sync::Mutex<InputHandler>>,
    cmd_sender: mpsc::Sender<PromptResult>,
    cmd_receiver: Arc<std::sync::Mutex<Option<mpsc::Receiver<PromptResult>>>>,
    prompt_ready: Arc<Notify>,
    shutdown_signal: Arc<Notify>,
    interactive: bool,
}

impl TextUi {
    /// Create new TextUi
    pub fn new(
        session: Arc<Session>,
        main_worker_id: Uuid,
        history_path: Option<PathBuf>,
        interactive: bool,
    ) -> Result<Self, eyre::Error> {
        let (cmd_sender, cmd_receiver) = mpsc::channel(10);
        
        Ok(Self {
            session,
            main_worker_id,
            input_handler: Arc::new(tokio::sync::Mutex::new(InputHandler::new(history_path)?)),
            cmd_sender,
            cmd_receiver: Arc::new(std::sync::Mutex::new(Some(cmd_receiver))),
            prompt_ready: Arc::new(Notify::new()),
            shutdown_signal: Arc::new(Notify::new()),
            interactive,
        })
    }
    
    /// Spawn prompt loop task
    fn spawn_prompt_loop(&self) -> JoinHandle<()> {
        let input_handler = self.input_handler.clone();
        let cmd_sender = self.cmd_sender.clone();
        let session = self.session.clone();
        let main_worker_id = self.main_worker_id;
        let prompt_ready = self.prompt_ready.clone();
        let shutdown = self.shutdown_signal.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Wait for worker to be ready for input
                    _ = prompt_ready.notified() => {
                        // Read input (blocking, but in separate task)
                        let mut handler = input_handler.lock().await;
                        let input = match handler.read_line("You").await {
                            Ok(input) => input,
                            Err(e) => {
                                eprintln!("Error reading input: {}", e);
                                continue;
                            }
                        };
                        drop(handler); // Release lock
                        
                        // Parse command
                        let command = match CommandParser::parse(&input) {
                            Ok(cmd) => cmd,
                            Err(e) => {
                                eprintln!("Error parsing command: {}", e);
                                // Re-signal for next prompt
                                prompt_ready.notify_one();
                                continue;
                            }
                        };
                        
                        match command {
                            // UI-specific commands - handle internally
                            Command::Ui(ui_cmd) => {
                                use crate::agent_env::UiCommand;
                                match ui_cmd {
                                    UiCommand::Usage => {
                                        if let Some(worker) = session.get_worker(main_worker_id) {
                                            let usage = ui_utils::calculate_token_usage(&worker);
                                            println!("Token usage:");
                                            println!("  Input:  {}", usage.input_tokens);
                                            println!("  Output: {}", usage.output_tokens);
                                            println!("  Total:  {}", usage.total_tokens);
                                        }
                                        // Re-signal for next prompt
                                        prompt_ready.notify_one();
                                    }
                                    UiCommand::Context => {
                                        if let Some(worker) = session.get_worker(main_worker_id) {
                                            let info = ui_utils::format_context_info(&worker);
                                            println!("{}", info);
                                        }
                                        // Re-signal for next prompt
                                        prompt_ready.notify_one();
                                    }
                                    UiCommand::Status => {
                                        if let Some(worker) = session.get_worker(main_worker_id) {
                                            let state = worker.lifecycle_state.lock().unwrap();
                                            println!("Worker status: {:?}", *state);
                                        }
                                        // Re-signal for next prompt
                                        prompt_ready.notify_one();
                                    }
                                    UiCommand::Workers => {
                                        println!("Workers:");
                                        println!("  Main worker: {}", main_worker_id);
                                        // Re-signal for next prompt
                                        prompt_ready.notify_one();
                                    }
                                }
                            }
                            
                            // Task-spawning commands - send to AgentEnvironment
                            Command::Agent(mut agent_cmd) => {
                                // Fill in worker_id
                                match &mut agent_cmd {
                                    AgentEnvironmentCommand::Prompt { worker_id, .. } => {
                                        *worker_id = main_worker_id;
                                    }
                                    AgentEnvironmentCommand::Compact { worker_id, .. } => {
                                        *worker_id = main_worker_id;
                                    }
                                    AgentEnvironmentCommand::Quit => {}
                                }
                                
                                // Send command
                                if cmd_sender.send(PromptResult::Command(agent_cmd)).await.is_err() {
                                    break; // Channel closed
                                }
                            }
                        }
                    }
                    _ = shutdown.notified() => break,
                }
            }
        })
    }
}

#[async_trait]
impl crate::agent_env::UserInterface for TextUi {
    async fn start(&self) -> Result<(), eyre::Error> {
        // Spawn prompt loop task only in interactive mode
        if self.interactive {
            tracing::info!("TextUi: Starting prompt loop");
            self.spawn_prompt_loop();
            
            // Check if worker is already Idle and signal prompt_ready if so
            let worker = self.session.get_worker(self.main_worker_id)
                .ok_or_else(|| eyre::eyre!("Worker not found"))?;
            let current_state = *worker.lifecycle_state.lock().unwrap();
            if current_state == WorkerLifecycleState::Idle {
                tracing::info!("TextUi: Worker is idle, enabling prompt");
                self.prompt_ready.notify_one();
            }
        } else {
            tracing::info!("TextUi: Non-interactive mode, skipping prompt loop");
        }
        
        Ok(())
    }
    
    fn command_receiver(&self) -> mpsc::Receiver<PromptResult> {
        self.cmd_receiver
            .lock()
            .unwrap()
            .take()
            .expect("command_receiver() called more than once")
    }
    
    async fn handle_event(&self, event: AgentEnvironmentEvent) {
        // Filter to main worker
        if let Some(wid) = event.worker_id() {
            if wid != self.main_worker_id {
                return;
            }
        }
        
        // Handle event
        match event {
            AgentEnvironmentEvent::Job(JobEvent::OutputChunk { chunk, .. }) => {
                match chunk {
                    OutputChunk::AssistantResponse(text) => {
                        print!("{}", text);
                        io::stdout().flush().unwrap();
                    }
                    OutputChunk::ToolUse { tool_name, .. } => {
                        println!("\n[Using tool: {}]", tool_name);
                    }
                    OutputChunk::ToolResult { tool_name, .. } => {
                        println!("[Tool {} completed]", tool_name);
                    }
                }
            }
            AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged { new_state, .. }) => {
                match new_state {
                    WorkerLifecycleState::Busy => {
                        // Worker started job - don't prompt
                    }
                    WorkerLifecycleState::Idle => {
                        println!(); // New line after completion
                        // Signal prompt loop to read input
                        self.prompt_ready.notify_one();
                    }
                    WorkerLifecycleState::IdleFailed => {
                        println!("\n[Task failed]");
                        // Still allow prompt
                        self.prompt_ready.notify_one();
                    }
                }
            }
            _ => {}
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::{EventBus, WorkerLifecycleState, UserInterface};
    use crate::agent_env::model_providers::{ModelProvider, ModelRequest, ModelResponse, ModelResponseChunk};
    use async_trait::async_trait;
    use tokio_util::sync::CancellationToken;

    struct MockModelProvider;

    #[async_trait]
    impl ModelProvider for MockModelProvider {
        async fn request(
            &self,
            _request: ModelRequest,
            _when_receiving_begin: Box<dyn Fn() + Send>,
            _when_received: Box<dyn Fn(ModelResponseChunk) + Send>,
            _cancellation_token: CancellationToken,
        ) -> Result<ModelResponse, eyre::Error> {
            Ok(ModelResponse {
                content: "test".to_string(),
                tool_requests: vec![],
            })
        }
    }

    fn create_test_session() -> Arc<Session> {
        let event_bus = EventBus::default();
        let model_provider = Arc::new(MockModelProvider);
        Arc::new(Session::new(event_bus, vec![model_provider]))
    }

    #[tokio::test]
    async fn test_event_filtering() {
        let session = create_test_session();
        let worker1 = session.build_worker("worker1".to_string());
        let worker2 = session.build_worker("worker2".to_string());
        
        let text_ui = TextUi::new(session.clone(), worker1.id, None, true).unwrap();
        
        // Event for worker1 should be processed (main_worker_id)
        let event1 = AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged {
            worker_id: worker1.id,
            old_state: WorkerLifecycleState::Idle,
            new_state: WorkerLifecycleState::Busy,
            timestamp: std::time::Instant::now(),
        });
        
        // Event for worker2 should be filtered out
        let event2 = AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged {
            worker_id: worker2.id,
            old_state: WorkerLifecycleState::Idle,
            new_state: WorkerLifecycleState::Busy,
            timestamp: std::time::Instant::now(),
        });
        
        // Both calls should succeed (filtering happens inside handle_event)
        text_ui.handle_event(event1).await;
        text_ui.handle_event(event2).await;
    }

    #[tokio::test]
    async fn test_output_chunk_display() {
        let session = create_test_session();
        let worker = session.build_worker("test".to_string());
        
        let text_ui = TextUi::new(session.clone(), worker.id, None, true).unwrap();
        
        // Test assistant response chunk
        let event = AgentEnvironmentEvent::Job(crate::agent_env::JobEvent::OutputChunk {
            worker_id: worker.id,
            job_id: uuid::Uuid::new_v4(),
            chunk: OutputChunk::AssistantResponse("Hello".to_string()),
            timestamp: std::time::Instant::now(),
        });
        
        text_ui.handle_event(event).await;
        
        // Test tool use chunk
        let event = AgentEnvironmentEvent::Job(crate::agent_env::JobEvent::OutputChunk {
            worker_id: worker.id,
            job_id: uuid::Uuid::new_v4(),
            chunk: OutputChunk::ToolUse {
                tool_name: "test_tool".to_string(),
                tool_input: serde_json::json!({}),
            },
            timestamp: std::time::Instant::now(),
        });
        
        text_ui.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_lifecycle_state_transitions() {
        let session = create_test_session();
        let worker = session.build_worker("test".to_string());
        
        let text_ui = TextUi::new(session.clone(), worker.id, None, true).unwrap();
        
        // Test transition to Busy (should not signal prompt)
        let event = AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged {
            worker_id: worker.id,
            old_state: WorkerLifecycleState::Idle,
            new_state: WorkerLifecycleState::Busy,
            timestamp: std::time::Instant::now(),
        });
        text_ui.handle_event(event).await;
        
        // Test transition to Idle (should signal prompt)
        let event = AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged {
            worker_id: worker.id,
            old_state: WorkerLifecycleState::Busy,
            new_state: WorkerLifecycleState::Idle,
            timestamp: std::time::Instant::now(),
        });
        text_ui.handle_event(event).await;
        
        // Test transition to IdleFailed (should signal prompt)
        let event = AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged {
            worker_id: worker.id,
            old_state: WorkerLifecycleState::Busy,
            new_state: WorkerLifecycleState::IdleFailed,
            timestamp: std::time::Instant::now(),
        });
        text_ui.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_command_receiver_single_use() {
        let session = create_test_session();
        let worker = session.build_worker("test".to_string());
        
        let text_ui = TextUi::new(session.clone(), worker.id, None, true).unwrap();
        
        // First call should succeed
        let _receiver = text_ui.command_receiver();
        
        // Note: Second call would panic, but we can't test that easily without UnwindSafe
        // The panic behavior is documented in the method
    }
}
