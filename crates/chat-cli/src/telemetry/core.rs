use std::fmt::Debug;
use std::time::SystemTime;

pub use amzn_toolkit_telemetry_client::types::MetricDatum;
use strum::{
    Display,
    EnumString,
};

use crate::telemetry::definitions::IntoMetricDatum;
use crate::telemetry::definitions::metrics::{
    AmazonqDidSelectProfile,
    AmazonqEndChat,
    AmazonqProfileState,
    AmazonqStartChat,
    CodewhispererterminalAddChatMessage,
    CodewhispererterminalCliSubcommandExecuted,
    CodewhispererterminalMcpServerInit,
    CodewhispererterminalRefreshCredentials,
    CodewhispererterminalToolUseSuggested,
    CodewhispererterminalUserLoggedIn,
};
use crate::telemetry::definitions::types::{
    CodewhispererterminalCustomToolInputTokenSize,
    CodewhispererterminalCustomToolLatency,
    CodewhispererterminalCustomToolOutputTokenSize,
    CodewhispererterminalInCloudshell,
    CodewhispererterminalIsToolValid,
    CodewhispererterminalMcpServerInitFailureReason,
    CodewhispererterminalToolName,
    CodewhispererterminalToolUseId,
    CodewhispererterminalToolUseIsSuccess,
    CodewhispererterminalToolsPerMcpServer,
    CodewhispererterminalUserInputId,
    CodewhispererterminalUtteranceId,
};

/// A serializable telemetry event that can be sent or queued.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub created_time: Option<SystemTime>,
    pub credential_start_url: Option<String>,
    #[serde(flatten)]
    pub ty: EventType,
}

impl Event {
    pub fn new(ty: EventType) -> Self {
        Self {
            ty,
            created_time: Some(SystemTime::now()),
            credential_start_url: None,
        }
    }

    pub fn with_credential_start_url(mut self, credential_start_url: String) -> Self {
        self.credential_start_url = Some(credential_start_url);
        self
    }

    pub fn into_metric_datum(self) -> Option<MetricDatum> {
        match self.ty {
            EventType::UserLoggedIn {} => Some(
                CodewhispererterminalUserLoggedIn {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::RefreshCredentials {
                request_id,
                result,
                reason,
                oauth_flow,
            } => Some(
                CodewhispererterminalRefreshCredentials {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    request_id: Some(request_id.into()),
                    result: Some(result.to_string().into()),
                    reason: reason.map(Into::into),
                    oauth_flow: Some(oauth_flow.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::CliSubcommandExecuted { subcommand } => Some(
                CodewhispererterminalCliSubcommandExecuted {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_subcommand: Some(subcommand.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::ChatStart { conversation_id } => Some(
                AmazonqStartChat {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::ChatEnd { conversation_id } => Some(
                AmazonqEndChat {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                }
                .into_metric_datum(),
            ),
            EventType::ChatAddedMessage {
                conversation_id,
                context_file_length,
                ..
            } => Some(
                CodewhispererterminalAddChatMessage {
                    create_time: self.created_time,
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_in_cloudshell: in_cloudshell(),
                    codewhispererterminal_context_file_length: context_file_length.map(|l| l as i64).map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::ToolUseSuggested {
                conversation_id,
                utterance_id,
                user_input_id,
                tool_use_id,
                tool_name,
                is_accepted,
                is_valid,
                is_success,
                is_custom_tool,
                input_token_size,
                output_token_size,
                custom_tool_call_latency,
            } => Some(
                CodewhispererterminalToolUseSuggested {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_utterance_id: utterance_id.map(CodewhispererterminalUtteranceId),
                    codewhispererterminal_user_input_id: user_input_id.map(CodewhispererterminalUserInputId),
                    codewhispererterminal_tool_use_id: tool_use_id.map(CodewhispererterminalToolUseId),
                    codewhispererterminal_tool_name: tool_name.map(CodewhispererterminalToolName),
                    codewhispererterminal_is_tool_use_accepted: Some(is_accepted.into()),
                    codewhispererterminal_is_tool_valid: is_valid.map(CodewhispererterminalIsToolValid),
                    codewhispererterminal_tool_use_is_success: is_success.map(CodewhispererterminalToolUseIsSuccess),
                    codewhispererterminal_is_custom_tool: Some(is_custom_tool.into()),
                    codewhispererterminal_custom_tool_input_token_size: input_token_size
                        .map(|s| CodewhispererterminalCustomToolInputTokenSize(s as i64)),
                    codewhispererterminal_custom_tool_output_token_size: output_token_size
                        .map(|s| CodewhispererterminalCustomToolOutputTokenSize(s as i64)),
                    codewhispererterminal_custom_tool_latency: custom_tool_call_latency
                        .map(|l| CodewhispererterminalCustomToolLatency(l as i64)),
                }
                .into_metric_datum(),
            ),
            EventType::McpServerInit {
                conversation_id,
                init_failure_reason,
                number_of_tools,
            } => Some(
                CodewhispererterminalMcpServerInit {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_mcp_server_init_failure_reason: init_failure_reason
                        .map(CodewhispererterminalMcpServerInitFailureReason),
                    codewhispererterminal_tools_per_mcp_server: Some(CodewhispererterminalToolsPerMcpServer(
                        number_of_tools as i64,
                    )),
                }
                .into_metric_datum(),
            ),
            EventType::DidSelectProfile {
                source,
                amazonq_profile_region,
                result,
                sso_region,
                profile_count,
            } => Some(
                AmazonqDidSelectProfile {
                    create_time: self.created_time,
                    value: None,
                    source: Some(source.to_string().into()),
                    amazon_q_profile_region: Some(amazonq_profile_region.into()),
                    result: Some(result.to_string().into()),
                    sso_region: sso_region.map(Into::into),
                    credential_start_url: self.credential_start_url.map(Into::into),
                    profile_count: profile_count.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::ProfileState {
                source,
                amazonq_profile_region,
                result,
                sso_region,
            } => Some(
                AmazonqProfileState {
                    create_time: self.created_time,
                    value: None,
                    source: Some(source.to_string().into()),
                    amazon_q_profile_region: Some(amazonq_profile_region.into()),
                    result: Some(result.to_string().into()),
                    sso_region: sso_region.map(Into::into),
                    credential_start_url: self.credential_start_url.map(Into::into),
                }
                .into_metric_datum(),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum EventType {
    UserLoggedIn {},
    RefreshCredentials {
        request_id: String,
        result: TelemetryResult,
        reason: Option<String>,
        oauth_flow: String,
    },
    CliSubcommandExecuted {
        subcommand: String,
    },
    ChatStart {
        conversation_id: String,
    },
    ChatEnd {
        conversation_id: String,
    },
    ChatAddedMessage {
        conversation_id: String,
        message_id: String,
        context_file_length: Option<usize>,
    },
    ToolUseSuggested {
        conversation_id: String,
        utterance_id: Option<String>,
        user_input_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        is_accepted: bool,
        is_success: Option<bool>,
        is_valid: Option<bool>,
        is_custom_tool: bool,
        input_token_size: Option<usize>,
        output_token_size: Option<usize>,
        custom_tool_call_latency: Option<usize>,
    },
    McpServerInit {
        conversation_id: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
    },
    DidSelectProfile {
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
        profile_count: Option<i64>,
    },
    ProfileState {
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
    },
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, serde::Serialize, serde::Deserialize)]
pub enum TelemetryResult {
    Succeeded,
    Failed,
    Cancelled,
}

/// 'user' -> users change the profile through Q CLI user profile command
/// 'auth' -> users change the profile through dashboard
/// 'update' -> CLI auto select the profile on users' behalf as there is only 1 profile
/// 'reload' -> CLI will try to reload previous selected profile upon CLI is running
#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, serde::Serialize, serde::Deserialize)]
pub enum QProfileSwitchIntent {
    User,
    Auth,
    Update,
    Reload,
}

fn in_cloudshell() -> Option<CodewhispererterminalInCloudshell> {
    Some(crate::util::system_info::in_cloudshell().into())
}
