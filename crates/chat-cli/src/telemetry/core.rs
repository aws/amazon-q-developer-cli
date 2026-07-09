use std::fmt::Debug;
use std::time::{
    Duration,
    SystemTime,
};

pub use amzn_toolkit_telemetry_client::types::MetricDatum;
use kiro_telemetry::{
    EventClass,
    FieldClass,
    LegacyEventType,
    MetricRecord,
    PiiRedactor,
    TelemetryLogRecord,
    log as telemetry_log,
    metric,
};
use kiro_telemetry_legacy::{
    legacy_log_record,
    legacy_metric_record,
};
use strum::{
    Display,
    EnumString,
};

use super::definitions::metrics::{
    CodewhispererterminalRecordUserTurnCompletion,
    KirocliSubagentInvocation,
    KirocliVoiceInput,
};
use super::definitions::types::CodewhispererterminalChatConversationType;
use crate::telemetry::definitions::IntoMetricDatum;
use crate::telemetry::definitions::metrics::{
    AmazonqDidSelectProfile,
    AmazonqEndChat,
    AmazonqMessageResponseError,
    AmazonqProfileState,
    AmazonqStartChat,
    AmazonqcliDailyHeartbeat,
    CodewhispererterminalAddChatMessage,
    CodewhispererterminalAgentConfigInit,
    CodewhispererterminalAgentContribution,
    CodewhispererterminalAuthFailed,
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
    CodewhispererterminalMcpServerAllToolsCount,
    CodewhispererterminalMcpServerInitFailureReason,
    CodewhispererterminalToolName,
    CodewhispererterminalToolUseId,
    CodewhispererterminalToolUseIsSuccess,
    CodewhispererterminalToolsPerMcpServer,
    CodewhispererterminalUserInputId,
    CodewhispererterminalUtteranceId,
    KirocliVoiceBackend,
    KirocliVoiceInputMethod,
};

/// A serializable telemetry event that can be sent or queued.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub created_time: Option<SystemTime>,
    pub credential_start_url: Option<String>,
    pub sso_region: Option<String>,
    pub client_application: Option<String>,
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
            client_application: None,
        }
    }

    pub fn set_start_url(&mut self, start_url: String) {
        self.credential_start_url = Some(start_url);
    }

    pub fn set_sso_region(&mut self, sso_region: String) {
        self.sso_region = Some(sso_region);
    }

    pub fn set_client_application(&mut self, client_application: String) {
        self.client_application = Some(client_application);
    }

    pub fn set_client_application_kind(&mut self, client_application: metric::ClientApplication) {
        self.client_application = Some(client_application.as_str().to_string());
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
                    codewhispererterminal_client_application: self.client_application.map(Into::into),
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
                        request_duration_seconds: _,
                        time_between_chunks_ms,
                        chat_conversation_type,
                        tool_name,
                        tool_use_id,
                        assistant_response_length,
                        message_meta_tags,
                        total_tokens: _,
                        uncached_input_tokens: _,
                        output_tokens: _,
                        cache_read_input_tokens: _,
                        cache_write_input_tokens: _,
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
                    reason_desc: redact_optional_telemetry_field(FieldClass::Other, reason_desc).map(Into::into),
                    status_code: status_code.map(|v| v as i64).map(Into::into),
                    codewhispererterminal_model: model.map(Into::into),
                    codewhispererterminal_time_to_first_chunks_ms: time_to_first_chunk_ms
                        .map(|v| format!("{v:.3}"))
                        .map(Into::into),
                    codewhispererterminal_time_between_chunks_ms: time_between_chunks_ms
                        .map(|v| v.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(","))
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
                    codewhispererterminal_client_application: self.client_application.map(Into::into),
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
                        model: _,
                        total_tokens: _,
                        uncached_input_tokens: _,
                        output_tokens: _,
                        cache_read_input_tokens: _,
                        cache_write_input_tokens: _,
                        user_turn_duration_seconds,
                        follow_up_count,
                        user_prompt_length,
                        message_meta_tags,
                        is_subagent,
                        parent_tool_use_id,
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
                    reason_desc: redact_optional_telemetry_field(FieldClass::Other, reason_desc).map(Into::into),
                    status_code: status_code.map(|v| v as i64).map(Into::into),
                    codewhispererterminal_chat_conversation_type: chat_conversation_type.map(Into::into),
                    codewhispererterminal_time_to_first_chunks_ms: Some(
                        time_to_first_chunks_ms
                            .into_iter()
                            .map(|v| v.map_or("null".to_string(), |v| format!("{v:.3}")))
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
                    codewhispererterminal_is_subagent: Some(is_subagent.into()),
                    codewhispererterminal_parent_tool_use_id: parent_tool_use_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::TangentModeSession {
                conversation_id,
                result,
                args:
                    TangentModeSessionArgs {
                        duration_seconds,
                        is_forget,
                        entries_removed,
                    },
            } => Some(
                CodewhispererterminalChatSlashCommandExecuted {
                    create_time: self.created_time,
                    value: Some(if is_forget {
                        entries_removed.unwrap_or(0) as f64
                    } else {
                        duration_seconds as f64
                    }),
                    credential_start_url: self.credential_start_url.map(Into::into),
                    sso_region: self.sso_region.map(Into::into),
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_chat_slash_command: Some("tangent".to_string().into()),
                    codewhispererterminal_chat_slash_subcommand: Some(
                        if is_forget { "forget" } else { "exit" }.to_string().into(),
                    ),
                    result: Some(result.to_string().into()),
                    reason: None,
                    codewhispererterminal_in_cloudshell: None,
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
                aws_service_name,
                aws_operation_name,
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
                    reason_desc: redact_optional_telemetry_field(FieldClass::Other, reason_desc).map(Into::into),
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
                    codewhispererterminal_client_application: self.client_application.map(Into::into),
                    codewhispererterminal_aws_service_name: aws_service_name.map(Into::into),
                    codewhispererterminal_aws_operation_name: aws_operation_name.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::AgentContribution {
                conversation_id,
                utterance_id,
                tool_use_id,
                tool_name,
                lines_by_agent,
                lines_by_user,
            } => Some(
                CodewhispererterminalAgentContribution {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_utterance_id: utterance_id.map(CodewhispererterminalUtteranceId),
                    codewhispererterminal_tool_use_id: tool_use_id.map(CodewhispererterminalToolUseId),
                    codewhispererterminal_tool_name: tool_name.map(CodewhispererterminalToolName),
                    codewhispererterminal_lines_by_agent: lines_by_agent.map(|count| count as i64).map(Into::into),
                    codewhispererterminal_lines_by_user: lines_by_user.map(|count| count as i64).map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::McpServerInit {
                conversation_id,
                server_name,
                init_failure_reason,
                number_of_tools,
                all_tool_names,
                loaded_tool_names,
                all_tools_count,
            } => Some(
                CodewhispererterminalMcpServerInit {
                    create_time: self.created_time,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    value: None,
                    amazonq_conversation_id: Some(conversation_id.into()),
                    codewhispererterminal_mcp_server_name: Some(server_name.into()),
                    codewhispererterminal_mcp_server_init_failure_reason: redact_optional_telemetry_field(
                        FieldClass::Other,
                        init_failure_reason,
                    )
                    .map(CodewhispererterminalMcpServerInitFailureReason),
                    codewhispererterminal_tools_per_mcp_server: Some(CodewhispererterminalToolsPerMcpServer(
                        number_of_tools as i64,
                    )),
                    codewhispererterminal_client_application: self.client_application.map(Into::into),
                    codewhispererterminal_mcp_server_all_tool_names: redact_optional_telemetry_field(
                        FieldClass::Context,
                        all_tool_names,
                    )
                    .map(Into::into),
                    codewhispererterminal_mcp_server_loaded_tool_names: redact_optional_telemetry_field(
                        FieldClass::Context,
                        loaded_tool_names,
                    )
                    .map(Into::into),
                    codewhispererterminal_mcp_server_all_tools_count: Some(
                        CodewhispererterminalMcpServerAllToolsCount(all_tools_count as i64),
                    ),
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
                    codewhispererterminal_launched_agent: Some(launched_agent.into()),
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
                model: _,
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
                    reason_desc: redact_optional_telemetry_field(FieldClass::Other, reason_desc).map(Into::into),
                    status_code: status_code.map(|v| v as i64).map(Into::into),
                    request_id: request_id.map(Into::into),
                    codewhispererterminal_utterance_id: message_id.map(Into::into),
                    codewhispererterminal_client_application: self.client_application.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::AuthFailed {
                auth_method,
                oauth_flow,
                error_type,
                error_code,
            } => Some(
                CodewhispererterminalAuthFailed {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_in_cloudshell: None,
                    codewhispererterminal_auth_method: Some(auth_method.into()),
                    oauth_flow: Some(oauth_flow.into()),
                    codewhispererterminal_error_type: Some(
                        redact_telemetry_field(FieldClass::Other, error_type).into(),
                    ),
                    codewhispererterminal_error_code: error_code.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::DailyHeartbeat {} => Some(
                AmazonqcliDailyHeartbeat {
                    create_time: self.created_time,
                    value: None,
                    source: None,
                }
                .into_metric_datum(),
            ),
            EventType::SubagentInvocation {
                parent_conversation_id,
                subagent_name,
                builtin_tool_uses,
                mcp_tool_uses,
                parent_tool_use_id,
            } => Some(
                KirocliSubagentInvocation {
                    amazonq_conversation_id: Some(parent_conversation_id.into()),
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_subagent_name: subagent_name.into(),
                    codewhispererterminal_builtin_tool_uses: (builtin_tool_uses as i64).into(),
                    codewhispererterminal_mcp_tool_uses: (mcp_tool_uses as i64).into(),
                    codewhispererterminal_parent_tool_use_id: parent_tool_use_id.into(),
                }
                .into_metric_datum(),
            ),
            EventType::VoiceInput {
                conversation_id,
                result,
                reason,
                reason_desc,
                backend,
                input_method,
                recording_duration_ms,
                transcription_duration_ms,
                text_length,
                model_size,
                auto_submit,
            } => {
                let voice_backend = match backend.as_str() {
                    "LocalWhisper" => KirocliVoiceBackend::LocalWhisper,
                    _ => KirocliVoiceBackend::RemoteServer,
                };
                let voice_input_method = match input_method.as_str() {
                    "SlashCommand" => KirocliVoiceInputMethod::SlashCommand,
                    "PTT" => KirocliVoiceInputMethod::Ptt,
                    "ContinuousVoice" => KirocliVoiceInputMethod::ContinuousVoice,
                    _ => KirocliVoiceInputMethod::Standalone,
                };
                Some(
                    KirocliVoiceInput {
                        create_time: self.created_time,
                        value: None,
                        amazonq_conversation_id: conversation_id.map(Into::into),
                        credential_start_url: self.credential_start_url.map(Into::into),
                        result: result.to_string().into(),
                        reason: reason.map(Into::into),
                        reason_desc: redact_optional_telemetry_field(FieldClass::Other, reason_desc).map(Into::into),
                        kirocli_voice_backend: voice_backend,
                        kirocli_voice_input_method: voice_input_method,
                        kirocli_voice_recording_duration_ms: recording_duration_ms.map(Into::into),
                        kirocli_voice_transcription_duration_ms: transcription_duration_ms.map(Into::into),
                        kirocli_voice_text_length: text_length.map(Into::into),
                        kirocli_voice_model_size: model_size.map(Into::into),
                        kirocli_voice_auto_submit: auto_submit.map(Into::into),
                    }
                    .into_metric_datum(),
                )
            },
            EventType::CliSessionStarted { .. }
            | EventType::CliSessionCompleted { .. }
            | EventType::MeteringEvent { .. }
            | EventType::EmptyResponseRetry { .. } => None,
        }
    }

    pub fn redaction_metric_records(&self, channel: metric::TelemetryChannel) -> Vec<MetricRecord> {
        self.ty.redaction_metric_records(channel)
    }

    #[allow(dead_code)]
    pub fn otel_metric_record(&self) -> Option<MetricRecord> {
        self.otel_metric_records().into_iter().next()
    }

    pub fn otel_metric_records(&self) -> Vec<MetricRecord> {
        match &self.ty {
            EventType::UserLoggedIn {} => vec![metric::user_logged_in_record(metric::UserLoggedIn::from_context(
                self.client_application.as_deref(),
                self.credential_start_url.as_deref(),
            ))],
            EventType::AuthFailed {
                auth_method,
                error_code,
                ..
            } => vec![metric::auth_failed_login_from_names(auth_method, error_code.as_deref())],
            // V1 DailyHeartbeat carries no install_method; reach V2 fidelity by
            // emitting the dimensioned daily_heartbeat metric with the client
            // application and InstallSource::Unknown (until V1 plumbs install method).
            EventType::DailyHeartbeat {} => vec![metric::daily_heartbeat_record(metric::DailyHeartbeat::from_names(
                self.client_application.as_deref(),
                None,
            ))],
            // Conscious drop: lines_by_agent/lines_by_user have no OTEL home.
            // Falling through to the legacy `_ =>` arm would emit an attribute-less
            // counter(1) onto kiro_cli_tokens_consumed and pollute the real token
            // metric. Whitelisted in the V1 parity regression test (ALLOWED_SILENT).
            EventType::AgentContribution { .. } => Vec::new(),
            EventType::ChatEnd { .. } => Vec::new(),
            EventType::CliSessionStarted {
                os_type,
                install_source,
            } => vec![cli_session_started_metric_record(
                *os_type,
                *install_source,
                &self.client_application,
            )],
            EventType::CliSessionCompleted {
                exit_reason,
                agent_kind,
            } => vec![cli_session_completed_metric_record(*exit_reason, *agent_kind)],
            EventType::ChatAddedMessage { result, data, .. } => {
                let mut records = self
                    .ty
                    .legacy_event_type()
                    .and_then(legacy_metric_record)
                    .into_iter()
                    .collect::<Vec<_>>();
                if let Some(record) = stream_ttft_metric_record(data) {
                    records.push(record);
                }
                if let Some(record) = request_duration_metric_record(data, result) {
                    records.push(record);
                }
                records.extend(token_metric_records(&data.model, &self.client_application, false, data));
                records
            },
            EventType::EmptyResponseRetry { model, outcome } => {
                vec![empty_response_retry_metric_record(model, *outcome)]
            },
            EventType::MessageResponseError {
                model,
                reason,
                status_code,
                ..
            } => vec![request_error_metric_record(model, reason, *status_code)],
            _ => self
                .ty
                .legacy_event_type()
                .and_then(legacy_metric_record)
                .into_iter()
                .collect(),
        }
    }

    pub fn otel_log_record(&self) -> Option<TelemetryLogRecord> {
        if let EventType::MeteringEvent {
            request_id,
            model,
            usage,
            unit,
            unit_plural,
        } = &self.ty
        {
            return Some(metering_log_record(
                request_id,
                model,
                &self.client_application,
                *usage,
                unit,
                unit_plural,
            ));
        }

        let record = legacy_log_record(self.ty.legacy_event_type()?)?;
        Some(match &self.ty {
            EventType::ChatEnd { conversation_id, .. } => conversation_completed_log_record(record, conversation_id),
            EventType::RecordUserTurnCompletion {
                conversation_id,
                result,
                args,
            } => turn_completion_log_record(conversation_id, result, args, &self.client_application),
            EventType::SubagentInvocation { subagent_name, .. } => {
                telemetry_log::subagent_invoked(subagent_name.as_str()).build()
            },
            _ => record,
        })
    }
}

fn conversation_completed_log_record(_record: TelemetryLogRecord, conversation_id: &str) -> TelemetryLogRecord {
    telemetry_log::conversation_completed(
        conversation_id.to_string(),
        conversation_id.to_string(),
        telemetry_log::CompletionReason::Stop,
    )
}

fn metering_log_record(
    request_id: &Option<String>,
    model: &Option<String>,
    client_application: &Option<String>,
    usage: f64,
    unit: &str,
    unit_plural: &str,
) -> TelemetryLogRecord {
    telemetry_log::metering_event(
        request_id.as_deref(),
        model.as_deref(),
        client_application
            .as_deref()
            .map(|value| metric::ClientApplication::from_name(Some(value))),
        usage,
        unit,
        unit_plural,
    )
}

fn turn_completion_log_record(
    conversation_id: &str,
    result: &TelemetryResult,
    args: &RecordUserTurnCompletionArgs,
    client_application: &Option<String>,
) -> TelemetryLogRecord {
    let failure_reason = args
        .reason
        .clone()
        .map(|reason| redact_telemetry_field(FieldClass::Other, reason));

    telemetry_log::user_turn_completed(
        conversation_id,
        telemetry_result_kind(result),
        args.is_subagent,
        args.user_prompt_length,
        args.assistant_response_length,
        args.user_turn_duration_seconds,
        args.follow_up_count,
    )
    .request_id(comma_join(args.request_ids.iter().filter_map(|id| id.as_deref())))
    .message_id(comma_join(args.message_ids.iter().map(String::as_str)))
    .model_id(args.model.as_deref())
    .client_application(
        client_application
            .as_deref()
            .map(|value| metric::ClientApplication::from_name(Some(value))),
    )
    .turn_failure_reason(failure_reason)
    .reason_desc(redact_optional_telemetry_field(
        FieldClass::Other,
        args.reason_desc.clone(),
    ))
    .status_code(args.status_code)
    .time_to_first_chunks_ms(format_optional_f64s(&args.time_to_first_chunks_ms))
    .message_meta_tags(comma_join(args.message_meta_tags.iter().map(ToString::to_string)))
    .parent_tool_use_id(args.parent_tool_use_id.clone())
    .total_tokens(args.total_tokens)
    .uncached_input_tokens(args.uncached_input_tokens)
    .output_tokens(args.output_tokens)
    .cache_read_input_tokens(args.cache_read_input_tokens)
    .cache_write_input_tokens(args.cache_write_input_tokens)
    .build()
}

fn comma_join(values: impl IntoIterator<Item = impl AsRef<str>>) -> Option<String> {
    let values = values
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.join(","))
}

fn format_optional_f64s(values: &[Option<f64>]) -> Option<String> {
    comma_join(values.iter().map(|value| match value {
        Some(value) => format!("{value:.3}"),
        None => "null".to_string(),
    }))
}

fn empty_response_retry_metric_record(model: &Option<String>, outcome: EmptyResponseRetryOutcome) -> MetricRecord {
    let outcome = match outcome {
        EmptyResponseRetryOutcome::Recovered => metric::Outcome::Recovered,
        EmptyResponseRetryOutcome::StillEmpty => metric::Outcome::StillEmpty,
    };
    metric::empty_response_retry(model.as_deref(), outcome)
}

fn request_error_metric_record(
    model: &Option<String>,
    reason: &Option<String>,
    status_code: Option<u16>,
) -> MetricRecord {
    metric::bedrock_request_error(
        model.as_deref(),
        metric::Operation::Stream,
        metric::ErrorKind::from_reason(reason.as_deref(), status_code),
        metric::StatusClass::from_status_code(status_code),
        reason.as_deref(),
    )
}

fn cli_session_started_metric_record(
    os_type: metric::OsType,
    install_source: metric::InstallSource,
    client_application: &Option<String>,
) -> MetricRecord {
    metric::cli_session_started_record(metric::CliSessionStarted::new(
        os_type,
        install_source,
        metric::ClientApplication::from_name(client_application.as_deref()),
    ))
}

fn cli_session_completed_metric_record(exit_reason: metric::ExitReason, agent_kind: metric::AgentKind) -> MetricRecord {
    metric::cli_session_completed_record(metric::CliSessionCompleted::new(exit_reason, agent_kind))
}

fn stream_ttft_metric_record(data: &ChatAddedMessageParams) -> Option<MetricRecord> {
    let milliseconds = data.time_to_first_chunk_ms?;
    if !milliseconds.is_finite() || milliseconds <= 0.0 {
        return None;
    }

    Some(metric::bedrock_stream_ttft(
        milliseconds / 1000.0,
        data.model.as_deref(),
        metric::PromptSizeBucket::from_context_file_length(data.context_file_length),
        tools_enabled(&data.chat_conversation_type),
    ))
}

fn request_duration_metric_record(data: &ChatAddedMessageParams, result: &TelemetryResult) -> Option<MetricRecord> {
    let seconds = data.request_duration_seconds?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return None;
    }

    Some(metric::bedrock_request_duration(
        seconds,
        data.model.as_deref(),
        metric::Operation::Stream,
        request_outcome(result),
    ))
}

fn tools_enabled(chat_conversation_type: &Option<ChatConversationType>) -> bool {
    matches!(chat_conversation_type, Some(ChatConversationType::ToolUse))
}

fn token_metric_records(
    model: &Option<String>,
    client_application: &Option<String>,
    is_subagent: bool,
    data: &ChatAddedMessageParams,
) -> Vec<MetricRecord> {
    let mut records = Vec::new();
    push_token_metric(
        &mut records,
        model,
        client_application,
        is_subagent,
        metric::TokenType::InputUncached,
        data.uncached_input_tokens,
    );
    push_token_metric(
        &mut records,
        model,
        client_application,
        is_subagent,
        metric::TokenType::InputCacheRead,
        data.cache_read_input_tokens,
    );
    push_token_metric(
        &mut records,
        model,
        client_application,
        is_subagent,
        metric::TokenType::InputCacheWrite,
        data.cache_write_input_tokens,
    );
    push_token_metric(
        &mut records,
        model,
        client_application,
        is_subagent,
        metric::TokenType::Output,
        data.output_tokens,
    );
    records
}

fn push_token_metric(
    records: &mut Vec<MetricRecord>,
    model: &Option<String>,
    client_application: &Option<String>,
    is_subagent: bool,
    token_type: metric::TokenType,
    value: Option<i32>,
) {
    let Some(value) = value else {
        return;
    };
    if value <= 0 {
        return;
    }

    records.push(metric::tokens_consumed(
        value as u64,
        model.as_deref(),
        token_type,
        metric::ClientApplication::from_name(client_application.as_deref()),
        is_subagent,
    ));
}

fn telemetry_result_kind(result: &TelemetryResult) -> metric::ResultKind {
    match result {
        TelemetryResult::Succeeded => metric::ResultKind::Success,
        TelemetryResult::Failed => metric::ResultKind::Failed,
        TelemetryResult::Cancelled => metric::ResultKind::Cancelled,
    }
}

fn request_outcome(result: &TelemetryResult) -> metric::Outcome {
    match result {
        TelemetryResult::Succeeded => metric::Outcome::Success,
        TelemetryResult::Failed => metric::Outcome::Error,
        TelemetryResult::Cancelled => metric::Outcome::Cancelled,
    }
}

fn redact_telemetry_field(field_class: FieldClass, value: String) -> String {
    PiiRedactor.redact(field_class, &value).text
}

fn redact_optional_telemetry_field(field_class: FieldClass, value: Option<String>) -> Option<String> {
    value.map(|value| redact_telemetry_field(field_class, value))
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
    GenerateAgent,
    /// A /tangent request
    TangentMode,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EmptyResponseRetryOutcome {
    Recovered,
    StillEmpty,
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
    pub request_duration_seconds: Option<f64>,
    pub time_between_chunks_ms: Option<Vec<f64>>,
    pub chat_conversation_type: Option<ChatConversationType>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub assistant_response_length: Option<i32>,
    pub message_meta_tags: Vec<MessageMetaTag>,
    #[serde(default)]
    pub total_tokens: Option<i32>,
    #[serde(default)]
    pub uncached_input_tokens: Option<i32>,
    #[serde(default)]
    pub output_tokens: Option<i32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i32>,
    #[serde(default)]
    pub cache_write_input_tokens: Option<i32>,
}

/// Optional fields for tangent mode session telemetry event.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct TangentModeSessionArgs {
    /// Duration of tangent mode session in seconds
    pub duration_seconds: i64,
    /// Whether this is a forget command (true) or tangent mode session (false)
    #[serde(default)]
    pub is_forget: bool,
    /// Number of conversation entries removed (only for forget command)
    #[serde(default)]
    pub entries_removed: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct RecordUserTurnCompletionArgs {
    pub request_ids: Vec<Option<String>>,
    pub message_ids: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub reason: Option<String>,
    pub reason_desc: Option<String>,
    pub status_code: Option<u16>,
    pub time_to_first_chunks_ms: Vec<Option<f64>>,
    pub chat_conversation_type: Option<ChatConversationType>,
    pub user_prompt_length: i64,
    pub assistant_response_length: i64,
    #[serde(default)]
    pub total_tokens: Option<i64>,
    #[serde(default)]
    pub uncached_input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(default)]
    pub cache_write_input_tokens: Option<i64>,
    pub user_turn_duration_seconds: i64,
    pub follow_up_count: i64,
    pub message_meta_tags: Vec<MessageMetaTag>,
    pub is_subagent: bool,
    pub parent_tool_use_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct AgentConfigInitArgs {
    pub agents_loaded_count: i64,
    pub agents_loaded_failed_count: i64,
    pub legacy_profile_migration_executed: bool,
    pub legacy_profile_migrated_count: i64,
    pub launched_agent: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum EventType {
    UserLoggedIn {},
    CliSessionStarted {
        os_type: metric::OsType,
        install_source: metric::InstallSource,
    },
    CliSessionCompleted {
        exit_reason: metric::ExitReason,
        agent_kind: metric::AgentKind,
    },
    AuthFailed {
        auth_method: String,
        oauth_flow: String,
        error_type: String,
        error_code: Option<String>,
    },
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
    TangentModeSession {
        conversation_id: String,
        result: TelemetryResult,
        args: TangentModeSessionArgs,
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
        aws_service_name: Option<String>,
        aws_operation_name: Option<String>,
    },
    AgentContribution {
        conversation_id: String,
        utterance_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        lines_by_agent: Option<isize>,
        lines_by_user: Option<isize>,
    },
    McpServerInit {
        conversation_id: String,
        server_name: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
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
        #[serde(default)]
        model: Option<String>,
    },
    DailyHeartbeat {},
    SubagentInvocation {
        parent_conversation_id: String,
        subagent_name: String,
        builtin_tool_uses: u32,
        mcp_tool_uses: u32,
        parent_tool_use_id: String,
    },
    VoiceInput {
        conversation_id: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        backend: String,
        input_method: String,
        recording_duration_ms: Option<i64>,
        transcription_duration_ms: Option<i64>,
        text_length: Option<i64>,
        model_size: Option<String>,
        auto_submit: Option<bool>,
    },
    MeteringEvent {
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
    },
    EmptyResponseRetry {
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    },
}

impl EventType {
    pub fn legacy_event_type(&self) -> Option<LegacyEventType> {
        match self {
            Self::UserLoggedIn {} => Some(LegacyEventType::UserLoggedIn),
            Self::CliSessionStarted { .. } => None,
            Self::CliSessionCompleted { .. } => None,
            Self::AuthFailed { .. } => Some(LegacyEventType::AuthFailed),
            Self::RefreshCredentials { .. } => Some(LegacyEventType::RefreshCredentials),
            Self::CliSubcommandExecuted { .. } => Some(LegacyEventType::CliSubcommandExecuted),
            Self::ChatSlashCommandExecuted { .. } => Some(LegacyEventType::ChatSlashCommandExecuted),
            Self::ChatStart { .. } => Some(LegacyEventType::ChatStart),
            Self::ChatEnd { .. } => Some(LegacyEventType::ChatEnd),
            Self::ChatAddedMessage { .. } => Some(LegacyEventType::ChatAddedMessage),
            Self::RecordUserTurnCompletion { .. } => Some(LegacyEventType::RecordUserTurnCompletion),
            Self::TangentModeSession { .. } => Some(LegacyEventType::TangentModeSession),
            Self::ToolUseSuggested { .. } => Some(LegacyEventType::ToolUseSuggested),
            Self::AgentContribution { .. } => Some(LegacyEventType::AgentContribution),
            Self::McpServerInit { .. } => Some(LegacyEventType::McpServerInit),
            Self::AgentConfigInit { .. } => Some(LegacyEventType::AgentConfigInit),
            Self::DidSelectProfile { .. } => Some(LegacyEventType::DidSelectProfile),
            Self::ProfileState { .. } => Some(LegacyEventType::ProfileState),
            Self::MessageResponseError { .. } => Some(LegacyEventType::MessageResponseError),
            Self::DailyHeartbeat {} => Some(LegacyEventType::DailyHeartbeat),
            Self::SubagentInvocation { .. } => Some(LegacyEventType::SubagentInvocation),
            Self::VoiceInput { .. } => Some(LegacyEventType::VoiceInput),
            Self::MeteringEvent { .. } => None,
            Self::EmptyResponseRetry { .. } => None,
        }
    }

    fn redaction_metric_records(&self, channel: metric::TelemetryChannel) -> Vec<MetricRecord> {
        match self {
            Self::ChatAddedMessage {
                data: ChatAddedMessageParams { reason_desc, .. },
                ..
            }
            | Self::RecordUserTurnCompletion {
                args: RecordUserTurnCompletionArgs { reason_desc, .. },
                ..
            }
            | Self::ToolUseSuggested { reason_desc, .. }
            | Self::MessageResponseError { reason_desc, .. }
            | Self::VoiceInput { reason_desc, .. } => {
                redaction_records_for_fields(channel, &[(FieldClass::Other, reason_desc.as_deref())])
            },
            Self::McpServerInit {
                init_failure_reason,
                all_tool_names,
                loaded_tool_names,
                ..
            } => redaction_records_for_fields(channel, &[
                (FieldClass::Other, init_failure_reason.as_deref()),
                (FieldClass::Context, all_tool_names.as_deref()),
                (FieldClass::Context, loaded_tool_names.as_deref()),
            ]),
            Self::AuthFailed { error_type, .. } => {
                redaction_records_for_fields(channel, &[(FieldClass::Other, Some(error_type.as_str()))])
            },
            _ => Vec::new(),
        }
    }
}

fn redaction_records_for_fields(
    channel: metric::TelemetryChannel,
    fields: &[(FieldClass, Option<&str>)],
) -> Vec<MetricRecord> {
    let mut records = Vec::new();
    for (field_class, value) in fields {
        let Some(value) = value else {
            continue;
        };
        records.extend(
            PiiRedactor
                .redact(*field_class, value)
                .metric_records(EventClass::LegacyEvent, channel),
        );
    }
    records
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
    pub aws_service_name: Option<String>,
    pub aws_operation_name: Option<String>,
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use kiro_telemetry::testing::{
        expect_metric_record as metric_record,
        metric_attr,
    };

    use super::*;

    fn metadata_value<'a>(datum: &'a MetricDatum, key: &str) -> Option<&'a str> {
        datum
            .metadata()
            .iter()
            .find(|entry| entry.key() == Some(key))
            .and_then(|entry| entry.value())
    }

    fn log_attr<'a>(record: &'a TelemetryLogRecord, key: &str) -> Option<&'a str> {
        record
            .attributes
            .iter()
            .find(|attr| attr.key == key)
            .map(|attr| attr.value.as_str())
    }

    #[test]
    fn redacts_reason_desc_before_metric_datum() {
        let event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Failed,
            data: ChatAddedMessageParams {
                reason_desc: Some("failed for dev@example.com with AKIA1234567890ABCDEF".to_string()),
                ..Default::default()
            },
        });

        let datum = event.into_metric_datum().expect("metric datum should be produced");
        let reason_desc = metadata_value(&datum, "reasonDesc").expect("reasonDesc should be present");

        assert!(!reason_desc.contains("dev@example.com"));
        assert!(!reason_desc.contains("AKIA1234567890ABCDEF"));
        assert!(reason_desc.contains("[REDACTED:email]"));
        assert!(reason_desc.contains("[REDACTED:aws_access_key]"));
    }

    #[test]
    fn redacts_mcp_free_text_before_metric_datum() {
        let event = Event::new(EventType::McpServerInit {
            conversation_id: "conversation".to_string(),
            server_name: "server".to_string(),
            init_failure_reason: Some("failed under /Users/alice/.kiro/config".to_string()),
            number_of_tools: 2,
            all_tool_names: Some("safe_tool, arn:aws:iam::123456789012:user/test".to_string()),
            loaded_tool_names: Some("AKIA1234567890ABCDEF".to_string()),
            all_tools_count: 2,
        });

        let datum = event.into_metric_datum().expect("metric datum should be produced");

        let init_failure = metadata_value(&datum, "codewhispererterminal_mcpServerInitFailureReason")
            .expect("init failure reason should be present");
        assert!(!init_failure.contains("/Users/alice"));
        assert!(init_failure.contains("[REDACTED:home_path]"));

        let all_tool_names = metadata_value(&datum, "codewhispererterminal_mcpServerAllToolNames")
            .expect("all tool names should be present");
        assert!(!all_tool_names.contains("arn:aws:iam::123456789012:user/test"));
        assert!(all_tool_names.contains("[REDACTED:arn]"));

        let loaded_tool_names = metadata_value(&datum, "codewhispererterminal_mcpServerLoadedToolNames")
            .expect("loaded tool names should be present");
        assert!(!loaded_tool_names.contains("AKIA1234567890ABCDEF"));
        assert!(loaded_tool_names.contains("[REDACTED:aws_access_key]"));
    }

    #[test]
    fn redacts_auth_error_type_before_metric_datum() {
        let event = Event::new(EventType::AuthFailed {
            auth_method: "builder_id".to_string(),
            oauth_flow: "device".to_string(),
            error_type: "token for user@example.com was rejected".to_string(),
            error_code: None,
        });

        let datum = event.into_metric_datum().expect("metric datum should be produced");
        let error_type =
            metadata_value(&datum, "codewhispererterminal_errorType").expect("error type should be present");

        assert!(!error_type.contains("user@example.com"));
        assert!(error_type.contains("[REDACTED:email]"));
    }

    #[test]
    fn produces_redaction_accounting_records() {
        let event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Failed,
            data: ChatAddedMessageParams {
                reason_desc: Some("failed for dev@example.com".to_string()),
                ..Default::default()
            },
        });

        let records = event.redaction_metric_records(metric::TelemetryChannel::LegacyToolkit);

        assert!(records.iter().any(|record| {
            record.name == "kiro_cli_pii_redaction_runs_total"
                && record
                    .attributes
                    .iter()
                    .any(|attr| attr.key == "channel" && attr.value == "legacy_toolkit")
        }));
        assert!(records.iter().any(|record| {
            record.name == "kiro_cli_pii_redaction_matches_total"
                && record
                    .attributes
                    .iter()
                    .any(|attr| attr.key == "pii_type" && attr.value == "email")
        }));
    }

    #[test]
    fn produces_schema_backed_otel_metric_record() {
        let event = Event::new(EventType::EmptyResponseRetry {
            model: None,
            outcome: EmptyResponseRetryOutcome::Recovered,
        });

        let record = event.otel_metric_record().expect("schema-backed metric event");

        assert_eq!(record.name, "kiro_cli.bedrock.empty_response.retries");
        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
    }

    #[test]
    fn emits_cli_session_started_metric_with_bounded_dimensions() {
        let mut event = Event::new(EventType::CliSessionStarted {
            os_type: metric::OsType::Macos,
            install_source: metric::InstallSource::Internal,
        });
        event.client_application = Some("chat_cli_v2".to_string());

        let records = event.otel_metric_records();
        let record = metric_record(&records, "kiro_cli_session_started_total");

        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert_eq!(metric_attr(record, "version_minor_bucket"), Some("current"));
        assert_eq!(metric_attr(record, "os_type"), Some("macos"));
        assert_eq!(metric_attr(record, "install_source"), Some("internal"));
        assert_eq!(metric_attr(record, "client_application"), Some("chat_cli_v2"));
    }

    #[test]
    fn user_login_does_not_count_as_cli_session_start() {
        let event = Event::new(EventType::UserLoggedIn {});

        // It now emits its own dedicated metric, but must never be the CLI
        // session-start counter (that would double-count session starts).
        let records = event.otel_metric_records();
        assert!(!records.is_empty());
        assert!(
            records
                .iter()
                .all(|record| record.name != "kiro_cli_session_started_total")
        );
    }

    #[test]
    fn emits_cli_session_completed_metric_with_bounded_dimensions() {
        let event = Event::new(EventType::CliSessionCompleted {
            exit_reason: metric::ExitReason::Clean,
            agent_kind: metric::AgentKind::Kas,
        });

        let records = event.otel_metric_records();
        let record = metric_record(&records, "kiro_cli.session.completed");

        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert_eq!(metric_attr(record, "exit_reason"), Some("clean"));
        assert_eq!(metric_attr(record, "agent_kind"), Some("kas"));
    }

    #[test]
    fn does_not_count_chat_end_as_cli_session_completion() {
        let event = Event::new(EventType::ChatEnd {
            conversation_id: "conversation".to_string(),
            model: None,
        });

        assert!(event.otel_metric_records().is_empty());
    }

    #[test]
    fn emits_conversation_completed_log_from_chat_end() {
        let event = Event::new(EventType::ChatEnd {
            conversation_id: "conversation".to_string(),
            model: None,
        });

        let record = event.otel_log_record().expect("conversation completion log");

        assert_eq!(record.name, "kiro_cli_conversation_completed");
        assert_eq!(log_attr(&record, "conversation_id"), Some("conversation"));
        assert_eq!(log_attr(&record, "session_id"), Some("conversation"));
        assert_eq!(log_attr(&record, "completion_reason"), Some("stop"));
    }

    #[test]
    fn emits_stream_ttft_metric_from_chat_added_message() {
        let event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                context_file_length: Some(2_000),
                model: Some("claude-4-sonnet".to_string()),
                time_to_first_chunk_ms: Some(250.0),
                request_duration_seconds: Some(1.2),
                chat_conversation_type: Some(ChatConversationType::ToolUse),
                uncached_input_tokens: Some(10),
                cache_read_input_tokens: Some(2),
                cache_write_input_tokens: Some(3),
                output_tokens: Some(5),
                ..Default::default()
            },
        });

        let records = event.otel_metric_records();
        let ttft = metric_record(&records, "kiro_cli.bedrock.stream.ttft");

        assert_eq!(ttft.value, kiro_telemetry::MetricValue::Histogram(0.25));
        assert_eq!(metric_attr(ttft, "model"), Some("claude-4-sonnet"));
        assert_eq!(metric_attr(ttft, "prompt_size_bucket"), Some("small"));
        assert_eq!(metric_attr(ttft, "tools_enabled"), Some("true"));
        let duration = metric_record(&records, "kiro_cli.bedrock.request.duration");
        assert_eq!(duration.value, kiro_telemetry::MetricValue::Histogram(1.2));
        assert_eq!(metric_attr(duration, "model"), Some("claude-4-sonnet"));
        assert_eq!(metric_attr(duration, "operation"), Some("stream"));
        assert_eq!(metric_attr(duration, "outcome"), Some("success"));
        assert!(records.iter().any(|record| record.name == "kiro_cli_user_turns"));

        let token_records = records
            .iter()
            .filter(|record| record.name == "kiro_cli_tokens_consumed")
            .collect::<Vec<_>>();
        assert_eq!(token_records.len(), 4);
        assert!(token_records.iter().any(|record| {
            record.value == kiro_telemetry::MetricValue::Counter(10)
                && metric_attr(record, "token_type") == Some("input_uncached")
        }));
        assert!(token_records.iter().any(|record| {
            record.value == kiro_telemetry::MetricValue::Counter(2)
                && metric_attr(record, "token_type") == Some("input_cache_read")
        }));
        assert!(token_records.iter().any(|record| {
            record.value == kiro_telemetry::MetricValue::Counter(3)
                && metric_attr(record, "token_type") == Some("input_cache_write")
        }));
        assert!(token_records.iter().any(|record| {
            record.value == kiro_telemetry::MetricValue::Counter(5)
                && metric_attr(record, "token_type") == Some("output")
        }));
    }

    #[test]
    fn produces_schema_backed_otel_log_record() {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Failed,
            args: RecordUserTurnCompletionArgs {
                request_ids: vec![Some("request-1".to_string())],
                message_ids: vec!["message-1".to_string()],
                model: Some("claude-4-sonnet".to_string()),
                reason: Some("ServiceFailure".to_string()),
                reason_desc: Some("failed for dev@example.com".to_string()),
                status_code: Some(500),
                time_to_first_chunks_ms: vec![Some(25.0), None],
                user_prompt_length: 7,
                assistant_response_length: 11,
                total_tokens: Some(20),
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_input_tokens: Some(2),
                cache_write_input_tokens: Some(3),
                user_turn_duration_seconds: 3,
                follow_up_count: 1,
                message_meta_tags: vec![MessageMetaTag::Compact],
                parent_tool_use_id: Some("parent-tool".to_string()),
                ..Default::default()
            },
        });
        event.client_application = Some("chat_cli".to_string());

        let record = event.otel_log_record().expect("log-backed legacy event");

        assert_eq!(record.name, "kiro_cli_user_turn_completed");
        assert_eq!(log_attr(&record, "conversation_id"), Some("conversation"));
        assert_eq!(log_attr(&record, "request_id"), Some("request-1"));
        assert_eq!(log_attr(&record, "message_id"), Some("message-1"));
        assert_eq!(log_attr(&record, "model"), Some("claude-4-sonnet"));
        assert_eq!(log_attr(&record, "client_application"), Some("chat_cli"));
        assert_eq!(log_attr(&record, "result"), Some("failed"));
        assert_eq!(log_attr(&record, "turn_failure_reason"), Some("ServiceFailure"));
        assert_eq!(log_attr(&record, "status_code"), Some("500"));
        assert_eq!(log_attr(&record, "time_to_first_chunks_ms"), Some("25.000,null"));
        assert_eq!(log_attr(&record, "user_prompt_length"), Some("7"));
        assert_eq!(log_attr(&record, "assistant_response_length"), Some("11"));
        assert_eq!(log_attr(&record, "user_turn_duration_seconds"), Some("3"));
        assert_eq!(log_attr(&record, "follow_up_count"), Some("1"));
        assert_eq!(log_attr(&record, "message_meta_tags"), Some("Compact"));
        assert_eq!(log_attr(&record, "parent_tool_use_id"), Some("parent-tool"));
        assert_eq!(log_attr(&record, "is_subagent"), Some("false"));
        assert_eq!(log_attr(&record, "total_tokens"), Some("20"));
        assert_eq!(log_attr(&record, "uncached_input_tokens"), Some("10"));
        assert_eq!(log_attr(&record, "output_tokens"), Some("5"));
        assert_eq!(log_attr(&record, "cache_read_input_tokens"), Some("2"));
        assert_eq!(log_attr(&record, "cache_write_input_tokens"), Some("3"));
        assert!(log_attr(&record, "reason_desc").is_some_and(|value| value.contains("[REDACTED:email]")));
    }

    #[test]
    fn emits_subagent_invoked_fact_log() {
        let event = Event::new(EventType::SubagentInvocation {
            parent_conversation_id: "parent-conversation".to_string(),
            subagent_name: "code-review".to_string(),
            builtin_tool_uses: 2,
            mcp_tool_uses: 1,
            parent_tool_use_id: "parent-tool".to_string(),
        });

        let record = event.otel_log_record().expect("subagent fact log");

        assert_eq!(record.name, "kiro_cli_subagent_invoked");
        assert_eq!(log_attr(&record, "subagent_name"), Some("code-review"));
    }

    #[test]
    fn emits_bounded_request_error_metric() {
        let event = Event::new(EventType::MessageResponseError {
            conversation_id: "conversation".to_string(),
            context_file_length: None,
            result: TelemetryResult::Failed,
            reason: Some("ThrottlingException".to_string()),
            reason_desc: None,
            status_code: Some(429),
            request_id: Some("request".to_string()),
            message_id: Some("message".to_string()),
            model: Some("claude-4-sonnet".to_string()),
        });

        let records = event.otel_metric_records();
        let record = metric_record(&records, "kiro_cli.bedrock.request.errors");

        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert_eq!(metric_attr(record, "model"), Some("claude-4-sonnet"));
        assert_eq!(metric_attr(record, "operation"), Some("stream"));
        assert_eq!(metric_attr(record, "error_kind"), Some("throttling"));
        assert_eq!(metric_attr(record, "status_class"), Some("4xx"));
    }

    #[test]
    fn exposes_canonical_legacy_event_type() {
        let event = EventType::VoiceInput {
            conversation_id: None,
            result: TelemetryResult::Succeeded,
            reason: None,
            reason_desc: None,
            backend: "LocalWhisper".to_string(),
            input_method: "PTT".to_string(),
            recording_duration_ms: None,
            transcription_duration_ms: None,
            text_length: None,
            model_size: None,
            auto_submit: None,
        };

        assert_eq!(event.legacy_event_type(), Some(LegacyEventType::VoiceInput));
    }

    #[test]
    fn emits_metering_event_as_otel_log_only() {
        let mut event = Event::new(EventType::MeteringEvent {
            request_id: Some("request".to_string()),
            model: Some("claude-4-sonnet".to_string()),
            usage: 1.25,
            unit: "credit".to_string(),
            unit_plural: "credits".to_string(),
        });
        event.client_application = Some("chat_cli".to_string());

        assert!(event.into_metric_datum().is_none());

        let mut event = Event::new(EventType::MeteringEvent {
            request_id: Some("request".to_string()),
            model: Some("claude-4-sonnet".to_string()),
            usage: 1.25,
            unit: "credit".to_string(),
            unit_plural: "credits".to_string(),
        });
        event.client_application = Some("chat_cli".to_string());

        let record = event.otel_log_record().expect("metering event log record");

        assert_eq!(record.name, "kiro_cli_metering_event");
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "request_id" && attr.value == "request")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "model" && attr.value == "claude-4-sonnet")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "metering_usage" && attr.value == "1.25")
        );
    }

    #[test]
    fn emits_empty_response_retry_as_otel_metric_only() {
        let event = Event::new(EventType::EmptyResponseRetry {
            model: Some("claude-4-sonnet".to_string()),
            outcome: EmptyResponseRetryOutcome::Recovered,
        });

        assert!(event.clone().into_metric_datum().is_none());

        let record = event.otel_metric_record().expect("empty-response retry metric");

        assert_eq!(record.name, "kiro_cli.bedrock.empty_response.retries");
        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "model" && attr.value == "claude-4-sonnet")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "outcome" && attr.value == "recovered")
        );
        assert!(event.otel_log_record().is_none());
    }

    #[test]
    fn user_login_emits_dedicated_metric_in_v1() {
        let mut event = Event::new(EventType::UserLoggedIn {});
        event.client_application = Some("chat_cli".to_string());
        event.credential_start_url = Some("https://my-corp.awsapps.com/start".to_string());

        let records = event.otel_metric_records();

        assert!(
            records
                .iter()
                .all(|record| record.name != "kiro_cli_session_started_total")
        );
        let record = records.first().expect("user login metric");
        assert_eq!(record.name, "kiro_cli_user_logged_in_total");
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "credential_kind" && attr.value == "idc")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "client_application" && attr.value == "chat_cli")
        );
    }

    #[test]
    fn auth_failed_emits_rich_credential_failure_metric_in_v1() {
        let event = Event::new(EventType::AuthFailed {
            auth_method: "sso".to_string(),
            oauth_flow: "device".to_string(),
            error_type: "rejected".to_string(),
            error_code: Some("ExpiredToken".to_string()),
        });

        let record = event.otel_metric_record().expect("auth credential failure metric");

        assert_eq!(record.name, "kiro_cli_auth_credential_failure_total");
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "auth_provider" && attr.value == "sso")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "operation" && attr.value == "login")
        );
        // `error_code` is a dynamic name: the constructor lowercases it for
        // cardinality hygiene (`normalized_dynamic_name`), so "ExpiredToken"
        // is emitted as "expiredtoken".
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "error_code" && attr.value == "expiredtoken")
        );
    }

    #[test]
    fn daily_heartbeat_emits_dimensioned_metric_in_v1() {
        let mut event = Event::new(EventType::DailyHeartbeat {});
        event.client_application = Some("chat_cli".to_string());

        let record = event.otel_metric_record().expect("daily heartbeat metric");

        assert_eq!(record.name, "kiro_cli_daily_heartbeat");
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "client_application" && attr.value == "chat_cli")
        );
        // install_method is not plumbed on the V1 event yet; defaults to unknown.
        assert!(
            record
                .attributes
                .iter()
                .any(|attr| attr.key == "install_method" && attr.value == "unknown")
        );
    }

    /// Compile-time exhaustiveness guard for the V1 [`EventType`].
    ///
    /// This `match` has NO wildcard (`_`) arm on purpose: a new variant fails to
    /// COMPILE here, forcing the author to add it to [`sample_v1_event_types`].
    /// DO NOT add a `_ => {}` arm — it would silently defeat the dark-gap regression
    /// test once the legacy MetricDatum sink is removed.
    #[allow(dead_code)]
    fn _v1_exhaustiveness_guard(ty: &EventType) {
        match ty {
            EventType::UserLoggedIn { .. } => {},
            EventType::CliSessionStarted { .. } => {},
            EventType::CliSessionCompleted { .. } => {},
            EventType::AuthFailed { .. } => {},
            EventType::RefreshCredentials { .. } => {},
            EventType::CliSubcommandExecuted { .. } => {},
            EventType::ChatSlashCommandExecuted { .. } => {},
            EventType::ChatStart { .. } => {},
            EventType::ChatEnd { .. } => {},
            EventType::ChatAddedMessage { .. } => {},
            EventType::RecordUserTurnCompletion { .. } => {},
            EventType::TangentModeSession { .. } => {},
            EventType::ToolUseSuggested { .. } => {},
            EventType::AgentContribution { .. } => {},
            EventType::McpServerInit { .. } => {},
            EventType::AgentConfigInit { .. } => {},
            EventType::DidSelectProfile { .. } => {},
            EventType::ProfileState { .. } => {},
            EventType::MessageResponseError { .. } => {},
            EventType::DailyHeartbeat { .. } => {},
            EventType::SubagentInvocation { .. } => {},
            EventType::VoiceInput { .. } => {},
            EventType::MeteringEvent { .. } => {},
            EventType::EmptyResponseRetry { .. } => {},
        }
    }

    fn sample_v1_event_types() -> Vec<EventType> {
        vec![
            EventType::UserLoggedIn {},
            EventType::CliSessionStarted {
                os_type: metric::OsType::Macos,
                install_source: metric::InstallSource::Brew,
            },
            EventType::CliSessionCompleted {
                exit_reason: metric::ExitReason::Clean,
                agent_kind: metric::AgentKind::Kas,
            },
            EventType::AuthFailed {
                auth_method: "builder_id".to_string(),
                oauth_flow: "device".to_string(),
                error_type: "rejected".to_string(),
                error_code: Some("InvalidGrant".to_string()),
            },
            EventType::RefreshCredentials {
                request_id: "req".to_string(),
                result: TelemetryResult::Failed,
                reason: None,
                oauth_flow: "device".to_string(),
            },
            EventType::CliSubcommandExecuted {
                subcommand: "chat".to_string(),
            },
            EventType::ChatSlashCommandExecuted {
                conversation_id: "conversation".to_string(),
                command: "help".to_string(),
                subcommand: None,
                result: TelemetryResult::Succeeded,
                reason: None,
            },
            EventType::ChatStart {
                conversation_id: "conversation".to_string(),
                model: None,
            },
            EventType::ChatEnd {
                conversation_id: "conversation".to_string(),
                model: None,
            },
            EventType::ChatAddedMessage {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                data: ChatAddedMessageParams::default(),
            },
            EventType::RecordUserTurnCompletion {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                args: RecordUserTurnCompletionArgs::default(),
            },
            EventType::TangentModeSession {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                args: TangentModeSessionArgs::default(),
            },
            EventType::ToolUseSuggested {
                conversation_id: "conversation".to_string(),
                utterance_id: None,
                user_input_id: None,
                tool_use_id: Some("tool_use".to_string()),
                tool_name: Some("fs_read".to_string()),
                is_accepted: true,
                is_trusted: true,
                is_success: Some(true),
                reason_desc: None,
                is_valid: Some(true),
                is_custom_tool: false,
                input_token_size: None,
                output_token_size: None,
                custom_tool_call_latency: None,
                model: None,
                execution_duration: Some(Duration::from_millis(10)),
                turn_duration: None,
                aws_service_name: None,
                aws_operation_name: None,
            },
            EventType::AgentContribution {
                conversation_id: "conversation".to_string(),
                utterance_id: None,
                tool_use_id: None,
                tool_name: None,
                lines_by_agent: Some(1),
                lines_by_user: Some(1),
            },
            EventType::McpServerInit {
                conversation_id: "conversation".to_string(),
                server_name: "server".to_string(),
                init_failure_reason: None,
                number_of_tools: 1,
                all_tool_names: None,
                loaded_tool_names: None,
                all_tools_count: 1,
            },
            EventType::AgentConfigInit {
                conversation_id: "conversation".to_string(),
                args: AgentConfigInitArgs::default(),
            },
            EventType::DidSelectProfile {
                source: QProfileSwitchIntent::User,
                amazonq_profile_region: "us-east-1".to_string(),
                result: TelemetryResult::Succeeded,
                sso_region: None,
                profile_count: None,
            },
            EventType::ProfileState {
                source: QProfileSwitchIntent::User,
                amazonq_profile_region: "us-east-1".to_string(),
                result: TelemetryResult::Succeeded,
                sso_region: None,
            },
            EventType::MessageResponseError {
                result: TelemetryResult::Failed,
                reason: None,
                reason_desc: None,
                status_code: Some(500),
                conversation_id: "conversation".to_string(),
                request_id: None,
                message_id: None,
                context_file_length: None,
                model: None,
            },
            EventType::DailyHeartbeat {},
            EventType::SubagentInvocation {
                parent_conversation_id: "conversation".to_string(),
                subagent_name: "agent".to_string(),
                builtin_tool_uses: 0,
                mcp_tool_uses: 0,
                parent_tool_use_id: "tool_use".to_string(),
            },
            EventType::VoiceInput {
                conversation_id: None,
                result: TelemetryResult::Succeeded,
                reason: None,
                reason_desc: None,
                backend: "local".to_string(),
                input_method: "standalone".to_string(),
                recording_duration_ms: None,
                transcription_duration_ms: None,
                text_length: None,
                model_size: None,
                auto_submit: None,
            },
            EventType::MeteringEvent {
                request_id: None,
                model: None,
                usage: 1.0,
                unit: "credit".to_string(),
                unit_plural: "credits".to_string(),
            },
            EventType::EmptyResponseRetry {
                model: None,
                outcome: EmptyResponseRetryOutcome::Recovered,
            },
        ]
    }

    /// V1 parity regression test: mirrors the V2 test for the separate, smaller V1
    /// [`EventType`] enum. Every variant MUST emit >=1 OTEL metric OR >=1 OTEL log,
    /// unless whitelisted in `ALLOWED_SILENT`. Guards the V1 path against going dark
    /// when the legacy MetricDatum sink is removed.
    #[test]
    fn every_v1_event_type_emits_an_otel_metric_or_log() {
        for ty in sample_v1_event_types() {
            // AgentContribution (lines_by_agent/lines_by_user) has no OTEL home;
            // the legacy fallback would pollute kiro_cli_tokens_consumed. See the
            // explicit drop arm. It is the only accepted parity hole.
            let allowed_silent = matches!(ty, EventType::AgentContribution { .. });
            let event = Event::new(ty);
            let emits_metric = !event.otel_metric_records().is_empty();
            let emits_log = event.otel_log_record().is_some();

            if allowed_silent {
                continue;
            }

            assert!(
                emits_metric || emits_log,
                "V1 {:?} emits no OTEL metric or log — add a constructor/wiring \
                 or whitelist it with justification",
                event.ty,
            );
        }
    }
}
