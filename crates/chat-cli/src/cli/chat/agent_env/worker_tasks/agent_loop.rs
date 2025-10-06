use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, debug, error};

use crate::cli::chat::agent_env::{
    Worker, WorkerTask, WorkerStates, WorkerToHostInterface,
    ModelRequest, ModelResponse, ModelProvider,
};

pub struct AgentLoopInput {
    pub prompt: String,
}

pub struct AgentLoop {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    input: AgentLoopInput,
    host_interface: Arc<dyn WorkerToHostInterface>,
}

impl AgentLoop {
    pub fn new(
        worker: Arc<Worker>,
        input: AgentLoopInput,
        host_interface: Arc<dyn WorkerToHostInterface>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            worker,
            input,
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
        
        let request = ModelRequest {
            prompt: self.input.prompt.clone(),
        };

        self.worker.set_state(WorkerStates::Requesting, &*self.host_interface);
        
        let response = self.worker.model_provider.request(
            request,
            || {
                self.worker.set_state(WorkerStates::Receiving, &*self.host_interface);
            },
            |chunk| {
                self.host_interface.response_chunk_received(self.worker.id, chunk);
            },
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
