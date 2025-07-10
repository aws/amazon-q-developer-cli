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
    AmazonqMessageResponseError,
    AmazonqProfileState,
    AmazonqStartChat,
    CodewhispererterminalAddChatMessage,
    CodewhispererterminalChatSlashCommandExecuted,
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
    pub sso_region: Option<String>,
    #[serde(flatten)]
    pub ty: EventType,
}

impl Event {
    pub fn new(ty: EventType) -> Self {
        Self {
            ty,
            created_time: Some(SystemTime::now()),
            credential_start_url: None,
            sso_region: None,
        }
    }

    pub fn set_start_url(&mut self, start_url: String) {
        self.credential_start_url = Some(start_url);
    }

    pub fn set_sso_region(&mut self, sso_region: String) {
        self.sso_region = Some(sso_region);
    }

    pub fn into_metric_datum(self) -> Option<MetricDatum> {
        match self.ty {
            EventType::UserLoggedIn {} => Some(
                CodewhispererterminalUserLoggedIn {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_in_cloudshell: None,
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
                    codewhispererterminal_in_cloudshell: None,
                }
                .into_metric_datum(),
            ),
            EventType::CliSubcommandExecuted { subcommand } => Some(
                CodewhispererterminalCliSubcommandExecuted {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_subcommand: Some(subcommand.into()),
                    codewhispererterminal_in_cloudshell: None,
                }
                .into_metric_datum(),
            ),
            EventType::ChatSlashCommandExecuted {
                conversation_id,
                command,
                subcommand,
                result,
                failure_reason,
            } => Some(
                CodewhispererterminalChatSlashCommandExecuted {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_chat_slash_command: Some(command.into()),
                    codewhispererterminal_chat_slash_subcommand: subcommand.map(Into::into),
                    codewhispererterminal_chat_slash_command_result: Some(result.to_string().into()),
                    codewhispererterminal_chat_slash_command_failure_reason: failure_reason.map(Into::into),
                    codewhispererterminal_in_cloudshell: None,
                }
                .into_metric_datum(),
            ),
            EventType::ChatStart { conversation_id, model } => Some(
                AmazonqStartChat {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_in_cloudshell: None,
                    codewhispererterminal_model: model.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::ChatEnd { conversation_id, model } => Some(
                AmazonqEndChat {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_in_cloudshell: None,
                    codewhispererterminal_model: model.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::ChatAddedMessage {
                conversation_id,
                context_file_length,
                message_id,
                request_id,
                result,
                reason,
                reason_desc,
                status_code,
                model,
                ..
            } => Some(
                CodewhispererterminalAddChatMessage {
                    create_time: self.created_time,
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    request_id: request_id.map(Into::into),
                    codewhispererterminal_utterance_id: message_id.map(Into::into),
                    credential_start_url: self.credential_start_url.map(Into::into),
                    sso_region: self.sso_region.map(Into::into),
                    codewhispererterminal_in_cloudshell: None,
                    codewhispererterminal_context_file_length: context_file_length.map(|l| l as i64).map(Into::into),
                    result: result.to_string().into(),
                    reason: reason.map(Into::into),
                    reason_desc: reason_desc.map(Into::into),
                    status_code: status_code.map(|v| v as i64).map(Into::into),
                    codewhispererterminal_model: model.map(Into::into),
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
                model,
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
                    codewhispererterminal_model: model.map(Into::into),
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
            EventType::MessageResponseError {
                conversation_id,
                context_file_length,
                result,
                reason,
                reason_desc,
                status_code,
            } => Some(
                AmazonqMessageResponseError {
                    create_time: self.created_time,
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_context_file_length: context_file_length.map(|l| l as i64).map(Into::into),
                    credential_start_url: self.credential_start_url.map(Into::into),
                    sso_region: self.sso_region.map(Into::into),
                    result: Some(result.to_string().into()),
                    reason: reason.map(Into::into),
                    reason_desc: reason_desc.map(Into::into),
                    status_code: status_code.map(|v| v as i64).map(Into::into),
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
    ChatSlashCommandExecuted {
        conversation_id: String,
        command: String,
        subcommand: Option<String>,
        result: TelemetryResult,
        failure_reason: Option<String>,
    },
    ChatStart {
        conversation_id: String,
        model: Option<String>,
    },
    ChatEnd {
        conversation_id: String,
        model: Option<String>,
    },
    ChatAddedMessage {
        conversation_id: String,
        message_id: Option<String>,
        request_id: Option<String>,
        context_file_length: Option<usize>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        model: Option<String>,
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
        model: Option<String>,
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
    MessageResponseError {
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        conversation_id: String,
        context_file_length: Option<usize>,
    },
}

#[derive(Debug)]
pub struct ToolUseEventBuilder {
    pub conversation_id: String,
    pub utterance_id: Option<String>,
    pub user_input_id: Option<String>,
    pub tool_use_id: Option<String>,
    pub tool_name: Option<String>,
    pub is_accepted: bool,
    pub is_success: Option<bool>,
    pub is_valid: Option<bool>,
    pub is_custom_tool: bool,
    pub input_token_size: Option<usize>,
    pub output_token_size: Option<usize>,
    pub custom_tool_call_latency: Option<usize>,
    pub model: Option<String>,
}

impl ToolUseEventBuilder {
    pub fn new(conv_id: String, tool_use_id: String, model: Option<String>) -> Self {
        Self {
            conversation_id: conv_id,
            utterance_id: None,
            user_input_id: None,
            tool_use_id: Some(tool_use_id),
            tool_name: None,
            is_accepted: false,
            is_success: None,
            is_valid: None,
            is_custom_tool: false,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model,
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
