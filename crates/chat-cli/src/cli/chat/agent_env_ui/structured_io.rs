use crate::agent_env::{
    AgentEnvironmentCommand, AgentEnvironmentEvent, AgentLoopEvent, JobCompletionResult, JobEvent,
    PromptResult, Session, UserInterface, WorkerEvent, WorkerLifecycleState,
};
use async_trait::async_trait;
use eyre::Result;
use owo_colors::OwoColorize;
use serde_json::json;
use std::io::Write;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, Mutex as TokioMutex, Notify};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// StructuredIO UI implementation
///
/// Reads single-line prompts from stdin and outputs structured JSON events.
/// Suitable for scripting and automation where commands may be piped in.
pub struct StructuredIO {
    session: Arc<Session>,
    cmd_sender: mpsc::Sender<PromptResult>,
    cmd_receiver: Arc<TokioMutex<Option<mpsc::Receiver<PromptResult>>>>,
    output_writer: Arc<TokioMutex<Box<dyn Write + Send>>>,
    interactive: bool,
    shutdown_signal: Arc<Notify>,
}

impl StructuredIO {
    /// Create new StructuredIO instance
    ///
    /// Returns tuple of (StructuredIO, Receiver) following Option C pattern from design.
    /// The receiver should be passed to AgentEnvironment.
    pub fn new(session: Arc<Session>, interactive: bool) -> Result<Self> {
        let (cmd_sender, cmd_receiver) = mpsc::channel(10);

        Ok(Self {
            session,
            cmd_sender,
            cmd_receiver: Arc::new(TokioMutex::new(Some(cmd_receiver))),
            output_writer: Arc::new(TokioMutex::new(Box::new(std::io::stdout()))),
            interactive,
            shutdown_signal: Arc::new(Notify::new()),
        })
    }

    /// Spawn input reader task with reader task pattern for responsive quit
    fn spawn_input_reader(&self) -> JoinHandle<()> {
        let cmd_sender = self.cmd_sender.clone();
        let session = self.session.clone();
        let shutdown = self.shutdown_signal.clone();

        tokio::spawn(async move {
            tracing::info!("StructuredIO input reader: task started");
            
            // Create channel for line communication
            let (line_tx, mut line_rx) = mpsc::channel::<String>(10);
            
            // Spawn dedicated stdin reader task (this will block on stdin)
            let reader_task = tokio::spawn(async move {
                let stdin = tokio::io::stdin();
                let reader = BufReader::new(stdin);
                let mut lines = reader.lines();
                
                while let Ok(Some(line)) = lines.next_line().await {
                    if line_tx.send(line).await.is_err() {
                        break; // Channel closed
                    }
                }
                tracing::info!("StructuredIO stdin reader: EOF reached or channel closed");
            });
            
            // Processor loop with tokio::select! for responsive shutdown
            loop {
                tokio::select! {
                    // Branch 1: Process incoming lines
                    Some(line) = line_rx.recv() => {
                        let line = line.trim();

                        if line.is_empty() {
                            continue;
                        }

                        // Try to parse as JSON command, otherwise treat as plain text prompt
                        let result = if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                            if let Some(cmd_str) = json.get("command").and_then(|v| v.as_str()) {
                                match cmd_str {
                                    "quit" => {
                                        // Send shutdown command
                                        if cmd_sender.send(PromptResult::Shutdown).await.is_ok() {
                                            tracing::info!("StructuredIO: Quit command sent");
                                        }
                                        // Trigger internal shutdown
                                        shutdown.notify_waiters();
                                        break;
                                    }
                                    "prompt" => {
                                        // Extract worker_id (optional) and text (required)
                                        let worker_id = json
                                            .get("worker_id")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| Uuid::parse_str(s).ok())
                                            .or_else(|| {
                                                // Default to first worker
                                                session.get_workers().first().map(|w| w.id)
                                            });

                                        let text = json
                                            .get("text")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();

                                        if let Some(wid) = worker_id {
                                            PromptResult::Command(AgentEnvironmentCommand::Prompt {
                                                worker_id: wid,
                                                text,
                                            })
                                        } else {
                                            // No workers available - skip
                                            continue;
                                        }
                                    }
                                    _ => {
                                        // Unknown command - treat whole line as prompt
                                        if let Some(worker_id) = session.get_workers().first().map(|w| w.id)
                                        {
                                            PromptResult::Command(AgentEnvironmentCommand::Prompt {
                                                worker_id,
                                                text: line.to_string(),
                                            })
                                        } else {
                                            continue;
                                        }
                                    }
                                }
                            } else {
                                // No command field - treat as prompt, extract text/prompt and worker_id
                                let worker_id = json
                                    .get("worker_id")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| Uuid::parse_str(s).ok())
                                    .or_else(|| {
                                        // Default to first worker
                                        session.get_workers().first().map(|w| w.id)
                                    });
                                
                                let text = json
                                    .get("text")
                                    .or_else(|| json.get("prompt"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(line)
                                    .to_string();
                                
                                if let Some(wid) = worker_id {
                                    PromptResult::Command(AgentEnvironmentCommand::Prompt {
                                        worker_id: wid,
                                        text,
                                    })
                                } else {
                                    continue;
                                }
                            }
                        } else {
                            // Not JSON - treat as plain text prompt
                            if let Some(worker_id) = session.get_workers().first().map(|w| w.id) {
                                PromptResult::Command(AgentEnvironmentCommand::Prompt {
                                    worker_id,
                                    text: line.to_string(),
                                })
                            } else {
                                continue;
                            }
                        };

                        if cmd_sender.send(result).await.is_err() {
                            break; // Channel closed
                        }
                    }
                    
                    // Branch 2: Handle shutdown signal
                    _ = shutdown.notified() => {
                        tracing::info!("StructuredIO input reader: shutdown signal received");
                        break;
                    }
                }
            }
            
            // Abort reader task on shutdown
            reader_task.abort();
            let _ = reader_task.await; // Ignore JoinError from abort
            tracing::info!("StructuredIO input reader: task exited");
        })
    }
}

#[async_trait]
impl UserInterface for StructuredIO {
    async fn start(&self) -> Result<()> {
        if self.interactive {
            tracing::info!("StructuredIO: Starting input reader");
            let _handle = self.spawn_input_reader();
        } else {
            tracing::info!("StructuredIO: Non-interactive mode, skipping input reader");
        }
        Ok(())
    }

    fn command_receiver(&self) -> mpsc::Receiver<PromptResult> {
        // Use try_lock since we're in sync context but called from async
        self.cmd_receiver
            .try_lock()
            .expect("command_receiver() called while locked")
            .take()
            .expect("command_receiver() called more than once")
    }

    async fn handle_event(&self, event: AgentEnvironmentEvent) {
        // Handle events
        match event {
            AgentEnvironmentEvent::Worker(WorkerEvent::Created {
                worker_id,
                name,
                timestamp,
            }) => {
                let json = json!({
                    "event": "worker_created",
                    "worker_id": worker_id,
                    "name": name,
                    "timestamp": format!("{:?}", timestamp),
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().white().dimmed()).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::Worker(WorkerEvent::Deleted {
                worker_id,
                timestamp,
            }) => {
                let json = json!({
                    "event": "worker_deleted",
                    "worker_id": worker_id,
                    "timestamp": format!("{:?}", timestamp),
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().white().dimmed()).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::Worker(WorkerEvent::LifecycleStateChanged {
                worker_id,
                new_state,
                ..
            }) => {
                let state_str = match new_state {
                    WorkerLifecycleState::Idle => "idle",
                    WorkerLifecycleState::Busy => "busy",
                    WorkerLifecycleState::IdleFailed => "idle_failed",
                };

                let json = json!({
                    "worker_id": worker_id,
                    "lifecycle_state": state_str,
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().white().dimmed()).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::Job(JobEvent::Started {
                worker_id,
                job_id,
                task_type,
                timestamp,
            }) => {
                let json = json!({
                    "event": "job_started",
                    "worker_id": worker_id,
                    "job_id": job_id,
                    "task_type": task_type,
                    "timestamp": format!("{:?}", timestamp),
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().white().dimmed()).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::Job(JobEvent::Completed {
                worker_id,
                job_id,
                result,
                timestamp,
            }) => {
                let result_str = match result {
                    JobCompletionResult::Success { .. } => "success",
                    JobCompletionResult::Cancelled => "cancelled",
                    JobCompletionResult::Failed { .. } => "failed",
                };

                let json = json!({
                    "event": "job_completed",
                    "worker_id": worker_id,
                    "job_id": job_id,
                    "result": result_str,
                    "timestamp": format!("{:?}", timestamp),
                });

                let mut writer = self.output_writer.lock().await;
                let output = match result {
                    JobCompletionResult::Failed { .. } => json.to_string().red().to_string(),
                    _ => json.to_string().white().dimmed().to_string(),
                };
                writeln!(writer, "{}", output).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ResponseReceived {
                worker_id,
                text,
                ..
            }) => {
                let json = json!({
                    "worker_id": worker_id,
                    "assistant_response": text,
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ToolUseRequestReceived {
                worker_id,
                tool_name,
                tool_input,
                ..
            }) => {
                let json = json!({
                    "worker_id": worker_id,
                    "tool_use_request": {
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                    }
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::WebUI(crate::agent_env::events::WebUIEvent::PromptReceived {
                worker_id,
                text,
                ..
            }) => {
                let json = json!({
                    "event": "webui_prompt",
                    "worker_id": worker_id,
                    "text": text,
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().cyan()).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::WebUI(crate::agent_env::events::WebUIEvent::ServerStarted {
                address,
                ..
            }) => {
                let json = json!({
                    "event": "webui_server_started",
                    "address": address,
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().bright_blue()).unwrap();
                writer.flush().unwrap();
            }
            AgentEnvironmentEvent::WebUI(crate::agent_env::events::WebUIEvent::WebSocketConnected {
                ..
            }) => {
                let json = json!({
                    "event": "webui_websocket_connected",
                });

                let mut writer = self.output_writer.lock().await;
                writeln!(writer, "{}", json.to_string().bright_blue()).unwrap();
                writer.flush().unwrap();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::{
        events::AgentLoopEvent, model_providers::ModelProvider, EventBus, ModelResponseChunk,
    };
    use async_trait::async_trait;
    use eyre::Result;

    // Mock ModelProvider for testing
    struct MockModelProvider;

    #[async_trait]
    impl ModelProvider for MockModelProvider {
        async fn request(
            &self,
            _request: crate::agent_env::ModelRequest,
            _when_receiving_begin: Box<dyn Fn() + Send>,
            _when_received: Box<dyn Fn(ModelResponseChunk) + Send>,
            _cancellation_token: tokio_util::sync::CancellationToken,
        ) -> Result<crate::agent_env::ModelResponse, eyre::Error> {
            Ok(crate::agent_env::ModelResponse {
                content: String::new(),
                tool_requests: vec![],
            })
        }
    }

    fn create_test_session() -> Arc<Session> {
        let event_bus = EventBus::default();
        let model_provider: Arc<dyn ModelProvider> = Arc::new(MockModelProvider);
        Arc::new(Session::new(event_bus, vec![model_provider]))
    }

    #[tokio::test]
    async fn test_structured_io_outputs_events_for_all_workers() {
        let session = create_test_session();
        let main_worker = session.build_worker("main".to_string());
        let other_worker = session.build_worker("other".to_string());

        let structured_io = StructuredIO::new(session.clone(), true).unwrap();

        // Event for main worker - should be processed
        let event1 = AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ResponseReceived {
            worker_id: main_worker.id,
            job_id: Uuid::new_v4(),
            text: "Response for main".to_string(),
            timestamp: std::time::Instant::now(),
        });

        // Event for other worker - should also be processed
        let event2 = AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ResponseReceived {
            worker_id: other_worker.id,
            job_id: Uuid::new_v4(),
            text: "Response for other".to_string(),
            timestamp: std::time::Instant::now(),
        });

        // Both should complete without error
        structured_io.handle_event(event1).await;
        structured_io.handle_event(event2).await;
    }

    #[tokio::test]
    async fn test_structured_io_outputs_json_for_response() {
        let session = create_test_session();
        let main_worker = session.build_worker("main".to_string());

        let structured_io = StructuredIO::new(session.clone(), true).unwrap();

        let event = AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ResponseReceived {
            worker_id: main_worker.id,
            job_id: Uuid::new_v4(),
            text: "Test response".to_string(),
            timestamp: std::time::Instant::now(),
        });

        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_structured_io_outputs_json_for_tool_use() {
        let session = create_test_session();
        let main_worker = session.build_worker("main".to_string());

        let structured_io = StructuredIO::new(session.clone(), true).unwrap();

        let event = AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ToolUseRequestReceived {
            worker_id: main_worker.id,
            job_id: Uuid::new_v4(),
            tool_name: "test_tool".to_string(),
            tool_input: serde_json::json!({"arg": "value"}),
            timestamp: std::time::Instant::now(),
        });

        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_command_receiver_single_use() {
        let session = create_test_session();
        let _main_worker = session.build_worker("main".to_string());

        let structured_io = StructuredIO::new(session.clone(), true).unwrap();

        // First call should succeed
        let _receiver = structured_io.command_receiver();

        // Test passes - we verified first call works
        // Second call would panic but we can't easily test that
    }

    #[tokio::test]
    async fn test_worker_created_event_handler() {
        use crate::agent_env::events::WorkerEvent;
        
        let session = create_test_session();
        let structured_io = StructuredIO::new(session.clone(), true).unwrap();
        
        let worker_id = Uuid::new_v4();
        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Created {
            worker_id,
            name: "test_worker".to_string(),
            timestamp: std::time::Instant::now(),
        });
        
        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_worker_deleted_event_handler() {
        use crate::agent_env::events::WorkerEvent;
        
        let session = create_test_session();
        let structured_io = StructuredIO::new(session.clone(), true).unwrap();
        
        let worker_id = Uuid::new_v4();
        let event = AgentEnvironmentEvent::Worker(WorkerEvent::Deleted {
            worker_id,
            timestamp: std::time::Instant::now(),
        });
        
        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_job_started_event_handler() {
        use crate::agent_env::events::JobEvent;
        
        let session = create_test_session();
        let structured_io = StructuredIO::new(session.clone(), true).unwrap();
        
        let event = AgentEnvironmentEvent::Job(JobEvent::Started {
            worker_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            task_type: "AgentLoop".to_string(),
            timestamp: std::time::Instant::now(),
        });
        
        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_job_completed_event_handler_success() {
        use crate::agent_env::events::{JobEvent, JobCompletionResult, UserInteractionRequired};
        
        let session = create_test_session();
        let structured_io = StructuredIO::new(session.clone(), true).unwrap();
        
        let event = AgentEnvironmentEvent::Job(JobEvent::Completed {
            worker_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            result: JobCompletionResult::Success {
                task_metadata: std::collections::HashMap::new(),
                user_interaction_required: UserInteractionRequired::None,
            },
            timestamp: std::time::Instant::now(),
        });
        
        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_job_completed_event_handler_failed() {
        use crate::agent_env::events::{JobEvent, JobCompletionResult};
        
        let session = create_test_session();
        let structured_io = StructuredIO::new(session.clone(), true).unwrap();
        
        let event = AgentEnvironmentEvent::Job(JobEvent::Completed {
            worker_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            result: JobCompletionResult::Failed {
                error: "test error".to_string(),
            },
            timestamp: std::time::Instant::now(),
        });
        
        // Should output JSON without error
        structured_io.handle_event(event).await;
    }

    #[tokio::test]
    async fn test_job_completed_event_handler_cancelled() {
        use crate::agent_env::events::{JobEvent, JobCompletionResult};
        
        let session = create_test_session();
        let structured_io = StructuredIO::new(session.clone(), true).unwrap();
        
        let event = AgentEnvironmentEvent::Job(JobEvent::Completed {
            worker_id: Uuid::new_v4(),
            job_id: Uuid::new_v4(),
            result: JobCompletionResult::Cancelled,
            timestamp: std::time::Instant::now(),
        });
        
        // Should output JSON without error
        structured_io.handle_event(event).await;
    }
}
