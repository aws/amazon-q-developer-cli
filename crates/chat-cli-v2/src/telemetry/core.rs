#[allow(unused_imports)]
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
    ToolUseEventBuilder,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SuggestionState {
    Accept,
    Discard,
    Empty,
    Reject,
}

impl SuggestionState {
    pub fn is_accepted(&self) -> bool {
        matches!(self, SuggestionState::Accept)
    }
}

impl From<SuggestionState> for amzn_codewhisperer_client::types::SuggestionState {
    fn from(value: SuggestionState) -> Self {
        match value {
            SuggestionState::Accept => amzn_codewhisperer_client::types::SuggestionState::Accept,
            SuggestionState::Discard => amzn_codewhisperer_client::types::SuggestionState::Discard,
            SuggestionState::Empty => amzn_codewhisperer_client::types::SuggestionState::Empty,
            SuggestionState::Reject => amzn_codewhisperer_client::types::SuggestionState::Reject,
        }
    }
}
