use crate::mcp_client::ModelPreferences;

/// Represents a pending sampling request that needs user approval
#[derive(Debug)]
pub struct PendingSamplingRequest {
    pub server_name: String,
    pub prompt_content: String,
    pub system_prompt: Option<String>,
    pub model_preferences: Option<ModelPreferences>,
    pub max_tokens: Option<u32>,
    pub include_context: Option<String>,
    pub temperature: Option<f64>,
    pub stop_sequences: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub approved: bool,
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
        include_context: Option<String>,
        temperature: Option<f64>,
        stop_sequences: Option<Vec<String>>,
        metadata: Option<serde_json::Value>,
        response_sender: tokio::sync::oneshot::Sender<SamplingApprovalResult>,
    ) -> Self {
        Self {
            server_name,
            prompt_content,
            system_prompt,
            model_preferences,
            max_tokens,
            include_context,
            temperature,
            stop_sequences,
            metadata,
            approved: false,
            response_sender: Some(response_sender),
        }
    }

    /// Get a human-readable description of this sampling request for approval UI
    pub fn get_description(&self) -> String {
        let mut desc = format!("MCP Server '{}' wants to make an LLM call", self.server_name);

        if let Some(ref system_prompt) = self.system_prompt {
            desc.push_str(&format!("\nSystem prompt: {}",
                if system_prompt.len() > 100 {
                    format!("{}...", &system_prompt[..100])
                } else {
                    system_prompt.clone()
                }
            ));
        }

        desc.push_str(&format!("\nPrompt: {}",
            if self.prompt_content.len() > 200 {
                format!("{}...", &self.prompt_content[..200])
            } else {
                self.prompt_content.clone()
            }
        ));

        if let Some(max_tokens) = self.max_tokens {
            desc.push_str(&format!("\nMax tokens: {}", max_tokens));
        }

        if let Some(temperature) = self.temperature {
            desc.push_str(&format!("\nTemperature: {}", temperature));
        }

        desc
    }

    /// Send approval result back to MCP client
    pub fn send_approval_result(&mut self, result: SamplingApprovalResult) {
        if let Some(sender) = self.response_sender.take() {
            if let Err(_) = sender.send(result) {
                tracing::warn!(target: "mcp", "Failed to send sampling approval result - receiver may have been dropped");
            }
        }
    }

    /// Check if this sampling request should be trusted based on server name
    pub fn requires_approval(&self, trusted_servers: &[String]) -> bool {
        !trusted_servers.contains(&self.server_name)
    }
}

/// Result of sampling approval process
#[derive(Debug)]
pub struct SamplingApprovalResult {
    pub approved: bool,
    pub modified_prompt: Option<String>,
    pub error_message: Option<String>,
}

impl SamplingApprovalResult {
    pub fn approved() -> Self {
        Self {
            approved: true,
            modified_prompt: None,
            error_message: None,
        }
    }

    pub fn rejected(reason: String) -> Self {
        Self {
            approved: false,
            modified_prompt: None,
            error_message: Some(reason),
        }
    }
}
