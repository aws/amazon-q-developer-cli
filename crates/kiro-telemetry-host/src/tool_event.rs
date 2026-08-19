use std::time::Duration;

use kiro_telemetry::metric;

/// Builder-shaped struct carrying the fields needed to construct a
/// `ToolUseSuggested` telemetry event. Lives on the host so harnesses share
/// the same shape when calling [`TelemetryThread::send_tool_use_suggested`].
#[derive(Debug, Default)]
pub struct ToolUseEventBuilder {
    pub conversation_id: String,
    pub utterance_id: Option<String>,
    pub user_input_id: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub mcp_server_name: Option<String>,
    pub is_accepted: bool,
    pub is_trusted: bool,
    /// Coarse location of the resolved filesystem target. Never a path.
    pub path_scope: Option<metric::PathScope>,
    /// How authorization was obtained, when the emitter knows it outright.
    /// `None` means derive it from `is_accepted`/`is_trusted`.
    pub approval_path: Option<metric::ApprovalPath>,
    pub is_success: Option<bool>,
    pub reason_desc: Option<String>,
    pub is_valid: Option<bool>,
    pub is_custom_tool: bool,
    pub input_token_size: Option<usize>,
    pub output_token_size: Option<usize>,
    pub custom_tool_call_latency: Option<usize>,
    pub model: Option<String>,
    pub execution_duration: Option<Duration>,
    pub turn_duration: Option<Duration>,
    pub aws_service_name: Option<String>,
    pub aws_operation_name: Option<String>,
}

impl ToolUseEventBuilder {
    pub fn new(conversation_id: String, tool_use_id: String, model: Option<String>) -> Self {
        Self {
            conversation_id,
            tool_use_id: Some(tool_use_id),
            model,
            ..Self::default()
        }
    }
}
