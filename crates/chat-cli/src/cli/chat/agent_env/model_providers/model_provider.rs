use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub enum ModelResponseChunk {
    AssistantMessage(String),
    ToolUseRequest { tool_name: String, parameters: String },
}

#[derive(Debug, Clone)]
pub struct ModelResponse {
    pub content: String,
    pub tool_requests: Vec<ToolRequest>,
}

#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub tool_name: String,
    pub parameters: String,
}

#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    async fn request(
        &self,
        request: ModelRequest,
        when_receiving_begin: impl Fn() + Send,
        when_received: impl Fn(ModelResponseChunk) + Send,
        cancellation_token: CancellationToken,
    ) -> Result<ModelResponse, eyre::Error>;
}
