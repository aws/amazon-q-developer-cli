use std::time::Duration;

use kiro_telemetry::metric;
pub use kiro_telemetry_host::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    ChatConversationType,
    EmptyResponseRetryOutcome,
    Event,
    EventType,
    MessageMetaTag,
    QProfileSwitchIntent,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    TelemetryResult,
};

#[derive(Debug)]
pub struct ToolUseEventBuilder {
    pub conversation_id: String,
    pub utterance_id: Option<String>,
    pub user_input_id: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub mcp_server_name: Option<String>,
    pub is_accepted: bool,
    pub is_trusted: bool,
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
    /// Coarse location of the resolved filesystem target. Never a path.
    pub path_scope: Option<metric::PathScope>,
}

impl ToolUseEventBuilder {
    pub fn new(conv_id: String, tool_use_id: String, model: Option<String>) -> Self {
        Self {
            conversation_id: conv_id,
            utterance_id: None,
            user_input_id: None,
            tool_use_id: Some(tool_use_id),
            tool_name: None,
            mcp_server_name: None,
            is_accepted: false,
            is_trusted: false,
            approval_path: Some(metric::ApprovalPath::Unknown),
            path_scope: None,
            is_success: None,
            reason_desc: None,
            is_valid: None,
            is_custom_tool: false,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model,
            execution_duration: None,
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        }
    }

    pub fn utterance_id(mut self, id: Option<String>) -> Self {
        self.utterance_id = id;
        self
    }

    pub fn set_tool_use_id(mut self, id: String) -> Self {
        self.tool_use_id.replace(id);
        self
    }

    pub fn set_tool_name(mut self, name: String) -> Self {
        self.tool_name.replace(name);
        self
    }

    pub fn set_target_scope(&mut self, scope: agent::protocol::ToolTargetScope) {
        self.path_scope = Some(match scope {
            agent::protocol::ToolTargetScope::Workspace => metric::PathScope::Workspace,
            agent::protocol::ToolTargetScope::OutsideWorkspace => metric::PathScope::OutsideWorkspace,
            agent::protocol::ToolTargetScope::Home => metric::PathScope::Home,
            agent::protocol::ToolTargetScope::System => metric::PathScope::System,
            agent::protocol::ToolTargetScope::NotApplicable => metric::PathScope::NotApplicable,
            agent::protocol::ToolTargetScope::Unknown => metric::PathScope::Unknown,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SuggestionState {
    Accept,
    Discard,
    Empty,
    Reject,
}

impl SuggestionState {
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accept)
    }
}

impl From<SuggestionState> for amzn_codewhisperer_client::types::SuggestionState {
    fn from(value: SuggestionState) -> Self {
        match value {
            SuggestionState::Accept => Self::Accept,
            SuggestionState::Discard => Self::Discard,
            SuggestionState::Empty => Self::Empty,
            SuggestionState::Reject => Self::Reject,
        }
    }
}
