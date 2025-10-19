use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{info, debug, error};

use crate::agent_env::{
    Worker, WorkerTask, WorkerStates,
    ModelRequest, ModelResponse,
    EventBus, AgentEnvironmentEvent, JobEvent, AgentLoopEvent, OutputChunk,
};
use crate::agent_env::worker::task_metadata_keys;
use crate::cli::chat::message::AssistantMessage;

pub struct AgentLoopInput {
    // Empty - all context comes from Worker
}

pub struct AgentLoop {
    worker: Arc<Worker>,
    event_bus: EventBus,
    cancellation_token: CancellationToken,
}

impl AgentLoop {
    pub fn new(
        worker: Arc<Worker>,
        _input: AgentLoopInput,
        event_bus: EventBus,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            event_bus,
            cancellation_token,
        }
    }

    fn check_cancellation(&self) -> Result<(), eyre::Error> {
        if self.cancellation_token.is_cancelled() {
            debug!(worker_id = %self.worker.id, "Cancelled");
            Err(eyre::eyre!("Cancelled"))
        } else {
            Ok(())
        }
    }

    async fn query_llm(&self) -> Result<ModelResponse, eyre::Error> {
        self.check_cancellation()?;
        
        // Get Os from worker
        let os = self.worker.get_os()
            .ok_or_else(|| eyre::eyre!("Os not available in worker"))?;
        
        // Build request using ContextBuilder
        let request = crate::agent_env::ContextBuilder::build_request(
            &self.worker.context_container,
            &os
        ).await?;

        self.worker.set_state(WorkerStates::Requesting);
        
        let worker = self.worker.clone();
        let worker_id = self.worker.id;
        let event_bus = self.event_bus.clone();
        let job_id = uuid::Uuid::new_v4();
        
        let model_provider = self.worker.model_provider.as_ref()
            .ok_or_else(|| eyre::eyre!("model_provider not available"))?;
        
        let response = model_provider.request(
            request,
            Box::new(move || {
                worker.set_state(WorkerStates::Receiving);
            }),
            Box::new(move |chunk| {
                use crate::agent_env::model_providers::ModelResponseChunk;
                if let ModelResponseChunk::AssistantMessage(text) = chunk {
                    event_bus.publish(AgentEnvironmentEvent::Job(
                        JobEvent::OutputChunk {
                            worker_id,
                            job_id,
                            chunk: OutputChunk::AssistantResponse(text),
                            timestamp: Instant::now(),
                        }
                    ));
                }
            }),
            self.cancellation_token.clone(),
        ).await.map_err(|e| {
            if !self.cancellation_token.is_cancelled() {
                let error_msg = format!("LLM request failed: {}", e);
                error!(worker_id = %self.worker.id, error = %e, "LLM request failed");
                self.worker.set_failure(error_msg);
                self.worker.set_state(WorkerStates::InactiveFailed);
            } else {
                self.worker.set_state(WorkerStates::Inactive);
            }
            e
        })?;

        debug!(
            worker_id = %self.worker.id,
            content_len = response.content.len(),
            tool_count = response.tool_requests.len(),
            "LLM response received"
        );

        Ok(response)
    }
}

#[async_trait::async_trait]
impl WorkerTask for AgentLoop {
    fn get_worker(&self) -> &Worker {
        &self.worker
    }

    async fn run(&self) -> Result<(), eyre::Error> {
        let start = Instant::now();
        let job_id = uuid::Uuid::new_v4(); // TODO: Get from WorkerJob
        info!(worker_id = %self.worker.id, "Agent loop started");

        self.check_cancellation()?;
        self.worker.set_failure("".to_string());
        self.worker.set_state(WorkerStates::Working);

        let response = self.query_llm().await?;

        // Publish events for tool use requests FIRST
        for tool_request in &response.tool_requests {
            // Parse parameters as JSON
            let tool_input: serde_json::Value = serde_json::from_str(&tool_request.parameters)
                .unwrap_or_else(|_| serde_json::Value::String(tool_request.parameters.clone()));

            // Publish OutputChunk event for tool use
            self.event_bus.publish(AgentEnvironmentEvent::Job(
                JobEvent::OutputChunk {
                    worker_id: self.worker.id,
                    job_id,
                    chunk: OutputChunk::ToolUse {
                        tool_name: tool_request.tool_name.clone(),
                        tool_input: tool_input.clone(),
                    },
                    timestamp: Instant::now(),
                }
            ));

            // Publish AgentLoopEvent for tool use request
            self.event_bus.publish(AgentEnvironmentEvent::AgentLoop(
                AgentLoopEvent::ToolUseRequestReceived {
                    worker_id: self.worker.id,
                    job_id,
                    tool_name: tool_request.tool_name.clone(),
                    tool_input,
                    timestamp: Instant::now(),
                }
            ));
        }

        // Publish AgentLoopEvent for complete response AFTER tool events
        self.event_bus.publish(AgentEnvironmentEvent::AgentLoop(
            AgentLoopEvent::ResponseReceived {
                worker_id: self.worker.id,
                job_id,
                text: response.content.clone(),
                timestamp: Instant::now(),
            }
        ));

        // Create assistant message and add to history
        let assistant_message = if response.tool_requests.is_empty() {
            AssistantMessage::new_response(None, response.content.clone())
        } else {
            // For now, create a simple response. Tool support will be added later.
            AssistantMessage::new_response(None, response.content.clone())
        };

        self.worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_assistant_message(assistant_message);

        // Set completion state metadata
        if !response.tool_requests.is_empty() {
            // Tool approval needed (for future implementation)
            self.worker.set_task_metadata(
                task_metadata_keys::AGENT_LOOP_COMPLETION_STATE,
                serde_json::Value::String("completed_with_tool_request".to_string()),
            );
            info!(
                worker_id = %self.worker.id,
                tool_count = response.tool_requests.len(),
                "Tool requests accumulated - approval needed"
            );
        } else {
            // Normal completion - ready for new prompt
            self.worker.set_task_metadata(
                task_metadata_keys::AGENT_LOOP_COMPLETION_STATE,
                serde_json::Value::String("completed_ready_for_prompt".to_string()),
            );
        }

        self.worker.set_state(WorkerStates::Inactive);
        
        let elapsed = start.elapsed();
        info!(
            worker_id = %self.worker.id,
            duration_ms = elapsed.as_millis(),
            "Agent loop completed"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::{
        EventBus, Session, ModelProvider, ModelRequest, ModelResponse,
        model_providers::{ModelResponseChunk, ToolRequest},
    };
    use crate::cli::chat::message::{UserMessage, UserMessageContent};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    /// Mock model provider for testing
    struct MockModelProvider {
        response: Arc<TokioMutex<Option<ModelResponse>>>,
    }

    impl MockModelProvider {
        fn new(response: ModelResponse) -> Self {
            Self {
                response: Arc::new(TokioMutex::new(Some(response))),
            }
        }
    }

    #[async_trait]
    impl ModelProvider for MockModelProvider {
        async fn request(
            &self,
            _request: ModelRequest,
            on_start: Box<dyn Fn() + Send>,
            on_chunk: Box<dyn Fn(ModelResponseChunk) + Send>,
            _cancellation_token: CancellationToken,
        ) -> Result<ModelResponse, eyre::Error> {
            on_start();
            
            let response = self.response.lock().await.take()
                .ok_or_else(|| eyre::eyre!("Response already consumed"))?;
            
            // Simulate streaming chunks
            for chunk in response.content.chars().collect::<Vec<_>>().chunks(10) {
                let chunk_str: String = chunk.iter().collect();
                on_chunk(ModelResponseChunk::AssistantMessage(chunk_str));
            }
            
            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_agent_loop_publishes_output_chunk_events() {
        // Create EventBus and Session
        let event_bus = EventBus::default();
        let mock_provider = Arc::new(MockModelProvider::new(ModelResponse {
            content: "Hello, this is a test response!".to_string(),
            tool_requests: vec![],
        }));
        let session = Arc::new(Session::new(event_bus.clone(), vec![mock_provider]));
        
        // Create worker and add initial message
        let worker = session.build_worker("test".to_string());
        let os = crate::os::Os::new().await.unwrap();
        worker.set_os(Arc::new(os));
        worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message("Test prompt".to_string());
        
        // Subscribe to events
        let mut receiver = event_bus.subscribe();
        
        // Create and run AgentLoop
        let agent_loop = AgentLoop::new(
            worker.clone(),
            AgentLoopInput {},
            event_bus.clone(),
            CancellationToken::new(),
        );
        
        // Run agent loop in background
        let worker_id = worker.id;
        tokio::spawn(async move {
            let _ = agent_loop.run().await;
        });
        
        // Collect events
        let mut output_chunks = Vec::new();
        let mut response_received = false;
        
        // Wait for events with timeout
        let timeout = tokio::time::Duration::from_secs(5);
        let start = tokio::time::Instant::now();
        
        while start.elapsed() < timeout {
            match tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                receiver.recv()
            ).await {
                Ok(Ok(event)) => {
                    if let Some(wid) = event.worker_id() {
                        if wid == worker_id {
                            match event {
                                AgentEnvironmentEvent::Job(JobEvent::OutputChunk { chunk, .. }) => {
                                    if let OutputChunk::AssistantResponse(text) = chunk {
                                        output_chunks.push(text);
                                    }
                                }
                                AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ResponseReceived { .. }) => {
                                    response_received = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => continue,
            }
        }
        
        // Verify events were published
        assert!(!output_chunks.is_empty(), "Should have received output chunks");
        assert!(response_received, "Should have received ResponseReceived event");
        
        // Verify chunks combine to full response
        let combined: String = output_chunks.join("");
        assert!(combined.contains("Hello"), "Combined chunks should contain response text");
    }

    #[tokio::test]
    async fn test_agent_loop_publishes_tool_use_events() {
        // Create EventBus and Session with tool use response
        let event_bus = EventBus::default();
        let mock_provider = Arc::new(MockModelProvider::new(ModelResponse {
            content: "I'll use a tool to help.".to_string(),
            tool_requests: vec![
                ToolRequest {
                    tool_name: "test_tool".to_string(),
                    parameters: r#"{"arg": "value"}"#.to_string(),
                },
            ],
        }));
        let session = Arc::new(Session::new(event_bus.clone(), vec![mock_provider]));
        
        // Create worker and add initial message
        let worker = session.build_worker("test".to_string());
        let os = crate::os::Os::new().await.unwrap();
        worker.set_os(Arc::new(os));
        worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message("Test prompt".to_string());
        
        // Subscribe to events
        let mut receiver = event_bus.subscribe();
        
        // Create and run AgentLoop
        let agent_loop = AgentLoop::new(
            worker.clone(),
            AgentLoopInput {},
            event_bus.clone(),
            CancellationToken::new(),
        );
        
        // Run agent loop in background
        let worker_id = worker.id;
        tokio::spawn(async move {
            let _ = agent_loop.run().await;
        });
        
        // Collect events
        let mut tool_use_chunks = Vec::new();
        let mut tool_use_requests = Vec::new();
        
        // Wait for events with timeout
        let timeout = tokio::time::Duration::from_secs(5);
        let start = tokio::time::Instant::now();
        
        while start.elapsed() < timeout {
            match tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                receiver.recv()
            ).await {
                Ok(Ok(event)) => {
                    if let Some(wid) = event.worker_id() {
                        if wid == worker_id {
                            match event {
                                AgentEnvironmentEvent::Job(JobEvent::OutputChunk { chunk, .. }) => {
                                    if let OutputChunk::ToolUse { tool_name, .. } = chunk {
                                        tool_use_chunks.push(tool_name);
                                    }
                                }
                                AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ToolUseRequestReceived { tool_name, .. }) => {
                                    tool_use_requests.push(tool_name);
                                }
                                AgentEnvironmentEvent::AgentLoop(AgentLoopEvent::ResponseReceived { .. }) => {
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => continue,
            }
        }
        
        // Verify tool use events were published
        assert_eq!(tool_use_chunks.len(), 1, "Should have received 1 ToolUse OutputChunk");
        assert_eq!(tool_use_requests.len(), 1, "Should have received 1 ToolUseRequestReceived event");
        assert_eq!(tool_use_chunks[0], "test_tool");
        assert_eq!(tool_use_requests[0], "test_tool");
    }

    #[tokio::test]
    async fn test_agent_loop_sets_completion_state_metadata() {
        // Test with normal completion (no tools)
        let event_bus = EventBus::default();
        let mock_provider = Arc::new(MockModelProvider::new(ModelResponse {
            content: "Simple response".to_string(),
            tool_requests: vec![],
        }));
        let session = Arc::new(Session::new(event_bus.clone(), vec![mock_provider]));
        
        let worker = session.build_worker("test".to_string());
        let os = crate::os::Os::new().await.unwrap();
        worker.set_os(Arc::new(os));
        worker.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message("Test prompt".to_string());
        
        let agent_loop = AgentLoop::new(
            worker.clone(),
            AgentLoopInput {},
            event_bus.clone(),
            CancellationToken::new(),
        );
        
        // Run agent loop
        agent_loop.run().await.expect("Agent loop should complete");
        
        // Check completion state metadata
        let completion_state = worker.get_task_metadata_string(
            task_metadata_keys::AGENT_LOOP_COMPLETION_STATE
        );
        assert_eq!(
            completion_state,
            Some("completed_ready_for_prompt".to_string()),
            "Should set completion state to ready_for_prompt"
        );
        
        // Test with tool request
        let event_bus2 = EventBus::default();
        let mock_provider2 = Arc::new(MockModelProvider::new(ModelResponse {
            content: "Using tool".to_string(),
            tool_requests: vec![
                ToolRequest {
                    tool_name: "test_tool".to_string(),
                    parameters: "{}".to_string(),
                },
            ],
        }));
        let session2 = Arc::new(Session::new(event_bus2.clone(), vec![mock_provider2]));
        
        let worker2 = session2.build_worker("test2".to_string());
        let os2 = crate::os::Os::new().await.unwrap();
        worker2.set_os(Arc::new(os2));
        worker2.context_container
            .conversation_history
            .lock()
            .unwrap()
            .push_input_message("Test prompt".to_string());
        
        let agent_loop2 = AgentLoop::new(
            worker2.clone(),
            AgentLoopInput {},
            event_bus2.clone(),
            CancellationToken::new(),
        );
        
        // Run agent loop
        agent_loop2.run().await.expect("Agent loop should complete");
        
        // Check completion state metadata
        let completion_state2 = worker2.get_task_metadata_string(
            task_metadata_keys::AGENT_LOOP_COMPLETION_STATE
        );
        assert_eq!(
            completion_state2,
            Some("completed_with_tool_request".to_string()),
            "Should set completion state to with_tool_request"
        );
    }
}

