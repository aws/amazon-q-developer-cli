use crate::mcp_client::ModelPreferences;

/// Represents a pending sampling request that needs user approval
#[derive(Debug)]
pub struct PendingSamplingRequest {
    pub server_name: String,
    pub prompt_content: String,
    pub system_prompt: Option<String>,
    pub model_preferences: Option<ModelPreferences>,
    pub max_tokens: Option<u32>,
    /// Channel to send approval result back to MCP client
    pub response_sender: Option<tokio::sync::oneshot::Sender<SamplingApprovalResult>>,
}

impl PendingSamplingRequest {
    pub fn new(
        server_name: String,
        prompt_content: String,
        system_prompt: Option<String>,
        model_preferences: Option<ModelPreferences>,
        max_tokens: Option<u32>,
        response_sender: tokio::sync::oneshot::Sender<SamplingApprovalResult>,
    ) -> Self {
        Self {
            server_name,
            prompt_content,
            system_prompt,
            model_preferences,
            max_tokens,
            response_sender: Some(response_sender),
        }
    }

    /// Get a human-readable description of this sampling request for approval UI
    pub fn get_description(&self) -> String {
        format!(
            "🤖 MCP Sampling Request from '{}'\n📝 Prompt: {}\n{}{}{}\n",
            self.server_name,
            self.prompt_content,
            self.system_prompt.as_ref()
                .map(|s| format!("🎯 System: {}\n", s))
                .unwrap_or_default(),
            self.model_preferences.as_ref()
                .map(|p| format!("⚙️  Model: {:?}\n", p))
                .unwrap_or_default(),
            self.max_tokens
                .map(|t| format!("📊 Max tokens: {}\n", t))
                .unwrap_or_default()
        )
    }

    /// Send approval result back to MCP client
    pub fn send_approval_result(&mut self, result: SamplingApprovalResult) {
        if let Some(sender) = self.response_sender.take() {
            if let Err(_) = sender.send(result) {
                tracing::warn!(target: "mcp", "Failed to send sampling approval result - receiver may have been dropped");
            }
        }
    }
}

/// Result of sampling approval process
#[derive(Debug)]
pub struct SamplingApprovalResult {
    pub approved: bool,
    pub error_message: Option<String>,
}

impl SamplingApprovalResult {
    pub fn approved() -> Self {
        Self {
            approved: true,
            error_message: None,
        }
    }

    pub fn rejected(reason: String) -> Self {
        Self {
            approved: false,
            error_message: Some(reason),
        }
    }
}
