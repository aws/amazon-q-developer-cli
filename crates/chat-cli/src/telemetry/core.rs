use std::fmt::Debug;
use std::time::{
    Duration,
    SystemTime,
};

pub use amzn_toolkit_telemetry_client::types::MetricDatum;
use strum::{
    Display,
    EnumString,
};

use super::definitions::metrics::CodewhispererterminalRecordUserTurnCompletion;
use super::definitions::types::CodewhispererterminalChatConversationType;
use crate::telemetry::definitions::IntoMetricDatum;
use crate::telemetry::definitions::metrics::{
    AmazonqDidSelectProfile,
    AmazonqEndChat,
    AmazonqMessageResponseError,
    AmazonqProfileState,
    AmazonqStartChat,
    CodewhispererterminalAddChatMessage,
    CodewhispererterminalAgentConfigInit,
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
                reason,
            } => Some(
                CodewhispererterminalChatSlashCommandExecuted {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    sso_region: self.sso_region.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_chat_slash_command: Some(command.into()),
                    codewhispererterminal_chat_slash_subcommand: subcommand.map(Into::into),
                    result: Some(result.to_string().into()),
                    reason: reason.map(Into::into),
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
                result,
                data:
                    ChatAddedMessageParams {
                        context_file_length,
                        message_id,
                        request_id,
                        reason,
                        reason_desc,
                        status_code,
                        model,
                        time_to_first_chunk_ms,
                        time_between_chunks_ms,
                        chat_conversation_type,
                        tool_name,
                        tool_use_id,
                        assistant_response_length,
                        message_meta_tags,
                    },
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
                    codewhispererterminal_time_to_first_chunks_ms: time_to_first_chunk_ms
                        .map(|v| format!("{:.3}", v))
                        .map(Into::into),
                    codewhispererterminal_time_between_chunks_ms: time_between_chunks_ms
                        .map(|v| v.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>().join(","))
                        .map(Into::into),
                    codewhispererterminal_chat_conversation_type: chat_conversation_type.map(Into::into),
                    codewhispererterminal_tool_name: tool_name.map(Into::into),
                    codewhispererterminal_tool_use_id: tool_use_id.map(Into::into),
                    codewhispererterminal_assistant_response_length: assistant_response_length
                        .map(|v| v as i64)
                        .map(Into::into),
                    codewhispererterminal_chat_message_meta_tags: Some(
                        message_meta_tags
                            .into_iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                            .into(),
                    ),
                }
                .into_metric_datum(),
            ),
            EventType::RecordUserTurnCompletion {
                conversation_id,
                result,
                args:
                    RecordUserTurnCompletionArgs {
                        message_ids,
                        request_ids,
                        reason,
                        reason_desc,
                        status_code,
                        time_to_first_chunks_ms,
                        chat_conversation_type,
                        assistant_response_length,
                        user_turn_duration_seconds,
                        follow_up_count,
                        user_prompt_length,
                        message_meta_tags,
                    },
            } => Some(
                CodewhispererterminalRecordUserTurnCompletion {
                    create_time: self.created_time,
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    request_id: Some(
                        request_ids
                            .into_iter()
                            .map(|id| id.unwrap_or("null".to_string()))
                            .collect::<Vec<_>>()
                            .join(",")
                            .into(),
                    ),
                    codewhispererterminal_utterance_id: Some(message_ids.join(",").into()),

                    credential_start_url: self.credential_start_url.map(Into::into),
                    sso_region: self.sso_region.map(Into::into),
                    codewhispererterminal_in_cloudshell: None,
                    result: result.to_string().into(),
                    reason: reason.map(Into::into),
                    reason_desc: reason_desc.map(Into::into),
                    status_code: status_code.map(|v| v as i64).map(Into::into),
                    codewhispererterminal_chat_conversation_type: chat_conversation_type.map(Into::into),
                    codewhispererterminal_time_to_first_chunks_ms: Some(
                        time_to_first_chunks_ms
                            .into_iter()
                            .map(|v| v.map_or("null".to_string(), |v| format!("{:.3}", v)))
                            .collect::<Vec<_>>()
                            .join(",")
                            .into(),
                    ),
                    codewhispererterminal_assistant_response_length: Some(assistant_response_length.into()),
                    codewhispererterminal_user_turn_duration_seconds: Some(user_turn_duration_seconds.into()),
                    codewhispererterminal_follow_up_count: Some(follow_up_count.into()),
                    codewhispererterminal_user_prompt_length: Some(user_prompt_length.into()),
                    codewhispererterminal_chat_message_meta_tags: Some(
                        message_meta_tags
                            .into_iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                            .into(),
                    ),
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
                is_trusted,
                is_valid,
                is_success,
                reason_desc,
                is_custom_tool,
                input_token_size,
                output_token_size,
                custom_tool_call_latency,
                model,
                execution_duration,
                turn_duration,
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
                    reason_desc: reason_desc.map(Into::into),
                    codewhispererterminal_is_custom_tool: Some(is_custom_tool.into()),
                    codewhispererterminal_custom_tool_input_token_size: input_token_size
                        .map(|s| CodewhispererterminalCustomToolInputTokenSize(s as i64)),
                    codewhispererterminal_custom_tool_output_token_size: output_token_size
                        .map(|s| CodewhispererterminalCustomToolOutputTokenSize(s as i64)),
                    codewhispererterminal_custom_tool_latency: custom_tool_call_latency
                        .map(|l| CodewhispererterminalCustomToolLatency(l as i64)),
                    codewhispererterminal_model: model.map(Into::into),
                    codewhispererterminal_is_tool_use_trusted: Some(is_trusted.into()),
                    codewhispererterminal_tool_execution_duration_ms: execution_duration
                        .map(|d| d.as_millis() as i64)
                        .map(Into::into),
                    codewhispererterminal_tool_turn_duration_ms: turn_duration
                        .map(|d| d.as_millis() as i64)
                        .map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::McpServerInit {
                conversation_id,
                server_name,
                init_failure_reason,
                number_of_tools,
            } => Some(
                CodewhispererterminalMcpServerInit {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_mcp_server_name: Some(server_name.into()),
                    codewhispererterminal_mcp_server_init_failure_reason: init_failure_reason
                        .map(CodewhispererterminalMcpServerInitFailureReason),
                    codewhispererterminal_tools_per_mcp_server: Some(CodewhispererterminalToolsPerMcpServer(
                        number_of_tools as i64,
                    )),
                }
                .into_metric_datum(),
            ),
            EventType::AgentConfigInit {
                conversation_id,
                args:
                    AgentConfigInitArgs {
                        agents_loaded_count,
                        agents_loaded_failed_count,
                        legacy_profile_migration_executed,
                        legacy_profile_migrated_count,
                        launched_agent,
                    },
            } => Some(
                CodewhispererterminalAgentConfigInit {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_agents_loaded_count: Some(agents_loaded_count.into()),
                    codewhispererterminal_agents_failed_to_load_count: Some(agents_loaded_failed_count.into()),
                    codewhispererterminal_legacy_profile_migration_executed: Some(
                        legacy_profile_migration_executed.into(),
                    ),
                    codewhispererterminal_legacy_profile_migrated_count: Some(legacy_profile_migrated_count.into()),
                    codewhispererterminal_launched_agent: launched_agent.map(Into::into),
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
                request_id,
                message_id,
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
                    request_id: request_id.map(Into::into),
                    codewhispererterminal_utterance_id: message_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, serde::Serialize, serde::Deserialize)]
pub enum ChatConversationType {
    // Names are as requested by science
    NotToolUse,
    ToolUse,
}

impl From<ChatConversationType> for CodewhispererterminalChatConversationType {
    fn from(value: ChatConversationType) -> Self {
        match value {
            ChatConversationType::NotToolUse => Self::NotToolUse,
            ChatConversationType::ToolUse => Self::ToolUse,
        }
    }
}

/// A metadata tag that can be used to annotate a request.
#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, serde::Serialize, serde::Deserialize)]
pub enum MessageMetaTag {
    /// A /compact request
    Compact,
}

/// Optional fields to add for a chatAddedMessage telemetry event.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct ChatAddedMessageParams {
    pub message_id: Option<String>,
    pub request_id: Option<String>,
    pub context_file_length: Option<usize>,
    pub reason: Option<String>,
    pub reason_desc: Option<String>,
    pub status_code: Option<u16>,
    pub model: Option<String>,
    pub time_to_first_chunk_ms: Option<f64>,
    pub time_between_chunks_ms: Option<Vec<f64>>,
    pub chat_conversation_type: Option<ChatConversationType>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub assistant_response_length: Option<i32>,
    pub message_meta_tags: Vec<MessageMetaTag>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct RecordUserTurnCompletionArgs {
    pub request_ids: Vec<Option<String>>,
    pub message_ids: Vec<String>,
    pub reason: Option<String>,
    pub reason_desc: Option<String>,
    pub status_code: Option<u16>,
    pub time_to_first_chunks_ms: Vec<Option<f64>>,
    pub chat_conversation_type: Option<ChatConversationType>,
    pub user_prompt_length: i64,
    pub assistant_response_length: i64,
    pub user_turn_duration_seconds: i64,
    pub follow_up_count: i64,
    pub message_meta_tags: Vec<MessageMetaTag>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct AgentConfigInitArgs {
    pub agents_loaded_count: i64,
    pub agents_loaded_failed_count: i64,
    pub legacy_profile_migration_executed: bool,
    pub legacy_profile_migrated_count: i64,
    pub launched_agent: Option<String>,
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
        reason: Option<String>,
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
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    },
    RecordUserTurnCompletion {
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    },
    ToolUseSuggested {
        conversation_id: String,
        utterance_id: Option<String>,
        user_input_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        is_accepted: bool,
        is_trusted: bool,
        is_success: Option<bool>,
        reason_desc: Option<String>,
        is_valid: Option<bool>,
        is_custom_tool: bool,
        input_token_size: Option<usize>,
        output_token_size: Option<usize>,
        custom_tool_call_latency: Option<usize>,
        model: Option<String>,
        execution_duration: Option<Duration>,
        turn_duration: Option<Duration>,
    },
    McpServerInit {
        conversation_id: String,
        server_name: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
    },
    AgentConfigInit {
        conversation_id: String,
        args: AgentConfigInitArgs,
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
        request_id: Option<String>,
        message_id: Option<String>,
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
    pub is_trusted: bool,
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
            is_trusted: false,
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
