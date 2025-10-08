use std::io::{self, Write};
use uuid::Uuid;
use tokio_util::sync::CancellationToken;

use crate::agent_env::{WorkerToHostInterface, WorkerStates, ModelResponseChunk};

pub struct TextUiWorkerToHostInterface {
    color_code: Option<&'static str>,
}

impl TextUiWorkerToHostInterface {
    pub fn new(color_code: Option<&'static str>) -> Self {
        Self { color_code }
    }
}

#[async_trait::async_trait]
impl WorkerToHostInterface for TextUiWorkerToHostInterface {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
        tracing::debug!("Worker {} state changed to {:?}", worker_id, new_state);
        
        match new_state {
            WorkerStates::Working => print!("\n"),
            _ => {}
        }
    }
    
    fn response_chunk_received(&self, _worker_id: Uuid, chunk: ModelResponseChunk) {
        match chunk {
            ModelResponseChunk::AssistantMessage(text) => {
                if let Some(color) = self.color_code {
                    print!("{}{}\x1b[0m", color, text);
                } else {
                    print!("{}", text);
                }
                io::stdout().flush().unwrap();
            }
            ModelResponseChunk::ToolUseRequest { tool_name, parameters } => {
                println!("\n[Tool: {} with params: {}]", tool_name, parameters);
            }
        }
    }
    
    async fn get_tool_confirmation(
        &self,
        _worker_id: Uuid,
        _request: String,
        _cancellation_token: CancellationToken,
    ) -> Result<String, eyre::Error> {
        // For now, auto-approve all tools
        // TODO: Implement interactive confirmation
        Ok("approved".to_string())
    }
}
