use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::cli::chat::agent_env::{
    Worker, WorkerTask, WorkerStates, WorkerToHostInterface,
    ModelRequest, ModelResponse, ModelResponseChunk, ModelProvider,
};

#[derive(Debug, Clone)]
pub struct WorkerInput {
    pub prompt: String,
}

pub struct WorkerProtoLoop {
    worker: Arc<Worker>,
    cancellation_token: CancellationToken,
    input: WorkerInput,
    host_interface: Arc<dyn WorkerToHostInterface>,
}

impl WorkerProtoLoop {
    pub fn new(
        worker: Arc<Worker>,
        input: WorkerInput,
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

    fn check_cancellation(&self, stage: &str) -> Result<(), eyre::Error> {
        if self.cancellation_token.is_cancelled() {
            println!("Worker {} cancelled {}", self.worker.id, stage);
            return Err(eyre::eyre!("Operation cancelled"));
        }
        Ok(())
    }

    fn reset_worker(&self) {
        self.worker.set_failure("".to_string());
        self.worker.set_state(WorkerStates::Working, &*self.host_interface);
    }

    fn build_request(&self) -> ModelRequest {
        ModelRequest {
            prompt: self.input.prompt.clone(),
        }
    }

    async fn make_model_request(&self, model_request: ModelRequest) -> Result<ModelResponse, eyre::Error> {
        self.worker.set_state(WorkerStates::Requesting, &*self.host_interface);
        
        let response = self.worker.model_provider.request(
            model_request,
            || {
                self.worker.set_state(WorkerStates::Receiving, &*self.host_interface);
            },
            |chunk| {
                self.host_interface.response_chunk_received(self.worker.id, chunk);
            },
            self.cancellation_token.clone(),
        ).await.map_err(|e| {
            if !self.cancellation_token.is_cancelled() {
                let error_msg = format!("Model request failed: {}", e);
                eprintln!("Error in worker {}: {}", self.worker.id, error_msg);
                self.worker.set_failure(error_msg);
                self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
            } else {
                self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
            }
            e
        })?;
        
        Ok(response)
    }

    async fn demo_tool_confirmation(&self, model_response: ModelResponse) -> Result<(), eyre::Error> {
        self.check_cancellation("before tool confirmation")?;
        self.worker.set_state(WorkerStates::Waiting, &*self.host_interface);
        
        let user_response = self.host_interface.get_tool_confirmation(
            self.worker.id,
            format!("Hello from worker! MR={}", 
                model_response.content.chars().take(50).collect::<String>()),
            self.cancellation_token.clone(),
        ).await.map_err(|e| {
            if !self.cancellation_token.is_cancelled() {
                let error_msg = format!("Tool confirmation failed: {}", e);
                eprintln!("Error in worker {}: {}", self.worker.id, error_msg);
                self.worker.set_failure(error_msg);
                self.worker.set_state(WorkerStates::InactiveFailed, &*self.host_interface);
            } else {
                self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
            }
            e
        })?;
        
        self.host_interface.response_chunk_received(
            self.worker.id,
            ModelResponseChunk::AssistantMessage(format!("\n\nUser said: {}\n", user_response))
        );
        
        Ok(())
    }

    fn complete_successfully(&self) -> Result<(), eyre::Error> {
        self.worker.set_state(WorkerStates::Inactive, &*self.host_interface);
        println!("Worker {} completed successfully", self.worker.id);
        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkerTask for WorkerProtoLoop {
    fn get_worker(&self) -> &Worker {
        &self.worker
    }

    async fn run(&self) -> Result<(), eyre::Error> {
        self.check_cancellation("before starting")?;
        self.reset_worker();
        
        let model_request = self.build_request();
        let model_response = self.make_model_request(model_request).await?;

        self.demo_tool_confirmation(model_response).await?;
        
        self.complete_successfully()
    }
}
