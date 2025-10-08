use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, debug, error};

use crate::agent_env::{
    Worker, WorkerTask, WorkerStates, WorkerToHostInterface,
    ModelRequest, ModelResponse, ModelProvider,
};
use crate::cli::chat::message::{AssistantMessage, UserMessageContent};

pub struct AgentLoopInput {
    // Empty - all context comes from Worker
}

pub struct AgentLoop {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    host_interface: Arc<dyn WorkerToHostInterface>,
}

impl AgentLoop {
    pub fn new(
        worker: Arc<Worker>,
        _input: AgentLoopInput,
        host_interface: Arc<dyn WorkerToHostInterface>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            host_interface,
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
        
        // Get prompt from worker's context
        let prompt = {
            let history = self.worker.context_container
                .conversation_history
                .lock()
                .unwrap();
            
            let last_entry = history.get_entries().last()
                .ok_or_else(|| eyre::eyre!("No messages in history"))?;
            
            match &last_entry.user {
                Some(user_msg) => match user_msg.content() {
                    UserMessageContent::Prompt { prompt } => prompt.clone(),
                    _ => return Err(eyre::eyre!("Expected prompt message")),
                },
                None => return Err(eyre::eyre!("Last entry is not a user message")),
            }
        };  // Lock dropped here

        let request = ModelRequest { prompt };

        self.worker.set_state(WorkerStates::Requesting, &*self.host_interface);
        
        let worker = self.worker.clone();
        let host_interface = self.host_interface.clone();
        let worker_id = self.worker.id;
        let host_interface2 = self.host_interface.clone();
        
        let response = self.worker.model_provider.request(
            request,
            Box::new(move || {
                worker.set_state(WorkerStates::Receiving, &*host_interface);
            }),
            Box::new(move |chunk| {
                host_interface2.response_chunk_received(worker_id, chunk);
            }),
            self.cancellation_token.clone(),
        ).await.map_err(|e| {
            if !self.cancellation_token.is_cancelled() {
                let error_msg = format!("LLM request failed: {}", e);
                error!(worker_id = %self.worker.id, error = %e, "LLM request failed");
                self.worker.set_failure(error_msg);
                self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
            } else {
                self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
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
        let start = std::time::Instant::now();
        info!(worker_id = %self.worker.id, "Agent loop started");

        self.check_cancellation()?;
        self.worker.set_failure("".to_string());
        self.worker.set_state(WorkerStates::Working, &*self.host_interface);

        let response = self.query_llm().await?;

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

        if !response.tool_requests.is_empty() {
            info!(
                worker_id = %self.worker.id,
                tool_count = response.tool_requests.len(),
                "Tool requests accumulated"
            );
        }

        self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
        
        let elapsed = start.elapsed();
        info!(
            worker_id = %self.worker.id,
            duration_ms = elapsed.as_millis(),
            "Agent loop completed"
        );

        Ok(())
    }
}
