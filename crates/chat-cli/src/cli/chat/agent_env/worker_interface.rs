use uuid::Uuid;
use tokio_util::sync::CancellationToken;

use super::worker::WorkerStates;
use super::model_providers::ModelResponseChunk;

#[async_trait::async_trait]
pub trait WorkerToHostInterface: Send + Sync {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates);
    fn response_chunk_received(&self, worker_id: Uuid, chunk: ModelResponseChunk);
    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, eyre::Error>;
}
