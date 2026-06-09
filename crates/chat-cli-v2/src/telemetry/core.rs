use std::fmt::Debug;
use std::time::Duration;

pub use amzn_toolkit_telemetry_client::types::MetricDatum;
use kiro_telemetry::{
    FieldClass,
    MetricRecord,
    PiiRedactor,
    TelemetryLogRecord,
    TokenUsage,
    legacy_log_record,
    legacy_metric_record,
    log as telemetry_log,
    metric,
};
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

use super::definitions::metrics::{
    CodewhispererterminalRecordUserTurnCompletion,
    KirocliGoalCompleted,
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
    CodewhispererterminalModeChanged,
    CodewhispererterminalProcessHealthSnapshot,
    CodewhispererterminalRefreshCredentials,
    CodewhispererterminalToolUseSuggested,
    CodewhispererterminalUiModeChanged,
    CodewhispererterminalUiModeDefaultChanged,
    CodewhispererterminalUiModeSessionStart,
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
    KirocliAppType,
    KirocliGoalTerminalState,
    KirocliVoiceBackend,
    KirocliVoiceInputMethod,
};

/// V2-specific extension methods for [`Event`] that translate to legacy
/// CloudWatch/Toolkit datums and OTel metric/log records. These live on a
/// trait (rather than `impl Event`) because [`Event`] is now defined in
/// `kiro-telemetry-host` and the orphan rule forbids inherent methods on
/// foreign types.
pub trait EventLegacyExt {
    fn into_metric_datum(self) -> Option<MetricDatum>;
    fn otel_metric_record(&self) -> Option<MetricRecord>;
    fn otel_metric_records(&self) -> Vec<MetricRecord>;
    fn otel_log_record(&self) -> Option<TelemetryLogRecord>;
}

impl EventLegacyExt for Event {
    fn into_metric_datum(self) -> Option<MetricDatum> {
        let app_type_enum = self.app_type.as_deref().and_then(|s| match s {
            "V1" => Some(KirocliAppType::V1),
            "V2" => Some(KirocliAppType::V2),
            "ACP" => Some(KirocliAppType::Acp),
            _ => None,
        });
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
                        total_tokens,
                        uncached_input_tokens,
                        output_tokens,
                        cache_read_input_tokens,
                        cache_write_input_tokens,
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
                    kirocli_app_type: app_type_enum.clone(),
                    kirocli_acp_client_name: self.acp_client_name.map(Into::into),
                    kirocli_acp_client_version: self.acp_client_version.map(Into::into),
                    codewhispererterminal_total_tokens: total_tokens.map(|v| v as i64).map(Into::into),
                    codewhispererterminal_uncached_input_tokens: uncached_input_tokens
                        .map(|v| v as i64)
                        .map(Into::into),
                    codewhispererterminal_output_tokens: output_tokens.map(|v| v as i64).map(Into::into),
                    codewhispererterminal_cache_read_input_tokens: cache_read_input_tokens
                        .map(|v| v as i64)
                        .map(Into::into),
                    codewhispererterminal_cache_write_input_tokens: cache_write_input_tokens
                        .map(|v| v as i64)
                        .map(Into::into),
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
                        total_tokens: _,
                        uncached_input_tokens: _,
                        output_tokens: _,
                        cache_read_input_tokens: _,
                        cache_write_input_tokens: _,
                        estimated_cost_usd: _,
                        user_turn_duration_seconds,
                        follow_up_count,
                        user_prompt_length,
                        message_meta_tags,
                        is_subagent,
                        emit_user_turn_counter: _,
                        parent_tool_use_id,
                        request_attempts,
                        model: _,
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
                    kirocli_app_type: app_type_enum.clone(),
                    kirocli_acp_client_name: self.acp_client_name.map(Into::into),
                    kirocli_acp_client_version: self.acp_client_version.map(Into::into),
                    codewhispererterminal_request_attempts: request_attempts.map(|v| v as i64).map(Into::into),
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
                mcp_server_name: _,
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
                    kirocli_app_type: app_type_enum.clone(),
                    kirocli_acp_client_name: self.acp_client_name.map(Into::into),
                    kirocli_acp_client_version: self.acp_client_version.map(Into::into),
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
                    kirocli_app_type: app_type_enum,
                    kirocli_acp_client_name: self.acp_client_name.map(Into::into),
                    kirocli_acp_client_version: self.acp_client_version.map(Into::into),
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
            EventType::DailyHeartbeat { .. } => Some(
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
            EventType::ProcessHealthMetric {
                agent_kind: _,
                rss_mb,
                heap_used_mb,
                peak_rss_mb,
                cpu_user_pct,
                cpu_system_pct,
                last_render_ms,
                max_render_ms,
                renders_per_min,
                full_redraws_per_min,
                yoga_node_count,
                event_loop_p99_ms,
                input_latency_p95_ms,
                session_duration_sec,
                cpu_cores,
                total_memory_mb,
                terminal,
                session_id,
                version,
                platform,
            } => Some(
                CodewhispererterminalProcessHealthSnapshot {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_tui_version: Some(version.into()),
                    codewhispererterminal_platform: Some(platform.into()),
                    codewhispererterminal_rss_mb: Some(rss_mb.into()),
                    codewhispererterminal_heap_used_mb: Some(heap_used_mb.into()),
                    codewhispererterminal_peak_rss_mb: Some(peak_rss_mb.into()),
                    codewhispererterminal_cpu_user_pct: Some(cpu_user_pct.into()),
                    codewhispererterminal_cpu_system_pct: Some(cpu_system_pct.into()),
                    codewhispererterminal_last_render_ms: Some(last_render_ms.into()),
                    codewhispererterminal_max_render_ms: Some(max_render_ms.into()),
                    codewhispererterminal_renders_per_min: Some(renders_per_min.into()),
                    codewhispererterminal_full_redraws_per_min: Some(full_redraws_per_min.into()),
                    codewhispererterminal_yoga_node_count: Some(yoga_node_count.into()),
                    codewhispererterminal_event_loop_p_99_ms: event_loop_p99_ms.map(Into::into),
                    codewhispererterminal_input_latency_p_95_ms: input_latency_p95_ms.map(Into::into),
                    codewhispererterminal_session_duration_sec: Some(session_duration_sec.into()),
                    codewhispererterminal_cpu_cores: Some(cpu_cores.into()),
                    codewhispererterminal_total_memory_mb: Some(total_memory_mb.into()),
                    codewhispererterminal_terminal_name: Some(terminal.into()),
                    amazonq_conversation_id: session_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::ModeChanged {
                from_mode,
                to_mode,
                source,
                session_id,
            } => Some(
                CodewhispererterminalModeChanged {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_mode_from_agent: Some(from_mode.into()),
                    codewhispererterminal_mode_to_agent: Some(to_mode.into()),
                    codewhispererterminal_mode_change_source: Some(source.to_string().into()),
                    amazonq_conversation_id: session_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::UiModeSessionStart {
                ui_mode,
                ui_mode_source,
                ui_mode_default,
                session_id,
            } => Some(
                CodewhispererterminalUiModeSessionStart {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_ui_mode: Some(ui_mode.into()),
                    codewhispererterminal_ui_mode_source: Some(ui_mode_source.to_string().into()),
                    codewhispererterminal_ui_mode_default: Some(ui_mode_default.into()),
                    amazonq_conversation_id: session_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::UiModeChanged {
                from,
                to,
                source,
                session_id,
            } => Some(
                CodewhispererterminalUiModeChanged {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_ui_mode_from: Some(from.into()),
                    codewhispererterminal_ui_mode_to: Some(to.into()),
                    codewhispererterminal_ui_mode_change_source: Some(source.to_string().into()),
                    amazonq_conversation_id: session_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::UiModeDefaultChanged { from, to, session_id } => Some(
                CodewhispererterminalUiModeDefaultChanged {
                    create_time: self.created_time,
                    value: None,
                    credential_start_url: self.credential_start_url.map(Into::into),
                    codewhispererterminal_ui_mode_default_from: Some(from.into()),
                    codewhispererterminal_ui_mode_default_to: Some(to.into()),
                    amazonq_conversation_id: session_id.map(Into::into),
                }
                .into_metric_datum(),
            ),
            EventType::GoalCompleted {
                conversation_id,
                terminal_state,
                iterations,
                max_iterations,
                duration_sec,
            } => {
                let goal_state = match terminal_state.as_str() {
                    "completed" => KirocliGoalTerminalState::Completed,
                    "exhausted" => KirocliGoalTerminalState::Exhausted,
                    _ => KirocliGoalTerminalState::Cancelled,
                };
                Some(
                    KirocliGoalCompleted {
                        create_time: self.created_time,
                        value: None,
                        amazonq_conversation_id: conversation_id.map(Into::into),
                        credential_start_url: self.credential_start_url.map(Into::into),
                        kirocli_goal_terminal_state: goal_state,
                        kirocli_goal_iterations: iterations.into(),
                        kirocli_goal_max_iterations: max_iterations.into(),
                        kirocli_goal_duration_sec: duration_sec.into(),
                    }
                    .into_metric_datum(),
                )
            },
            EventType::ContextUsagePercentage { .. }
            | EventType::MeteringEvent { .. }
            | EventType::EmptyResponseRetry { .. }
            | EventType::RetryAttempt { .. }
            | EventType::RetryExhausted { .. }
            | EventType::ModelInvocation { .. }
            | EventType::CliSessionStarted { .. }
            | EventType::CliSessionCompleted { .. }
            | EventType::ChatSessionStarted { .. } => None,
        }
    }

    fn otel_metric_record(&self) -> Option<MetricRecord> {
        self.otel_metric_records().into_iter().next()
    }

    fn otel_metric_records(&self) -> Vec<MetricRecord> {
        let legacy_event_type = self.ty.legacy_event_type();
        match &self.ty {
            EventType::UserLoggedIn {} => Vec::new(),
            EventType::ChatEnd { .. } => Vec::new(),
            EventType::CliSessionStarted {
                os_type,
                install_source,
            } => vec![metric::cli_session_started_record(metric::CliSessionStarted::new(
                *os_type,
                *install_source,
                metric::ClientApplication::from_name(self.client_application.as_deref()),
            ))],
            EventType::CliSessionCompleted {
                exit_reason,
                agent_kind,
            } => vec![metric::cli_session_completed_record(metric::CliSessionCompleted::new(
                *exit_reason,
                *agent_kind,
            ))],
            EventType::DailyHeartbeat { install_method } => {
                vec![metric::daily_heartbeat_record(metric::DailyHeartbeat::from_names(
                    self.client_application.as_deref(),
                    install_method.as_deref(),
                ))]
            },
            EventType::ChatAddedMessage { result, data, .. } => {
                let tools_enabled = matches!(data.chat_conversation_type, Some(ChatConversationType::ToolUse));
                let response = metric::ModelResponseMetrics::from_turn_context(
                    turn_metric_context(
                        &data.model,
                        &self.client_application,
                        &self.app_type,
                        self.is_subagent,
                        &data.message_meta_tags,
                    ),
                    (*result).into(),
                    tools_enabled,
                )
                .legacy_event(legacy_event_type)
                .context_file_length(data.context_file_length)
                .time_to_first_chunk_ms(data.time_to_first_chunk_ms)
                .time_between_chunks_ms(data.time_between_chunks_ms.as_deref())
                .request_duration_seconds(data.request_duration_seconds)
                .token_usage(token_usage_from_chat_added_message(data));
                metric::model_response_records(response)
            },
            EventType::RecordUserTurnCompletion { result, args, .. } => {
                let completion = metric::UserTurnCompletionMetrics::from_turn_context(
                    turn_metric_context(
                        &args.model,
                        &self.client_application,
                        &self.app_type,
                        args.is_subagent,
                        &args.message_meta_tags,
                    ),
                    (*result).into(),
                )
                .emit_user_turn_counter(args.emit_user_turn_counter)
                .token_usage(token_usage_from_turn_args(args))
                .estimated_cost_usd(args.estimated_cost_usd)
                .duration_seconds(Some(args.user_turn_duration_seconds as f64));
                metric::user_turn_completion_records(completion)
            },
            EventType::ContextUsagePercentage { model, percentage } => {
                metric::context_usage_percentage_record(metric::ContextUsageMetric::from_names(
                    *percentage,
                    model.as_deref(),
                    self.client_application.as_deref(),
                    self.is_subagent,
                ))
                .into_iter()
                .collect()
            },
            EventType::ModelInvocation { model } => vec![metric::model_invocation_record(
                metric::ModelInvocation::from_id(model.as_deref()),
            )],
            EventType::ToolUseSuggested {
                tool_name,
                is_accepted,
                is_success,
                is_valid,
                is_custom_tool,
                execution_duration,
                aws_service_name,
                ..
            } => {
                let tool_use = metric::ToolUseMetrics::from_tool_context(
                    tool_name.as_deref(),
                    aws_service_name.as_deref(),
                    *is_custom_tool,
                    *is_accepted,
                    *is_valid,
                    *is_success,
                )
                .legacy_event(legacy_event_type)
                .execution_duration(*execution_duration);
                metric::tool_use_records(tool_use)
            },
            EventType::McpServerInit {
                server_name,
                init_failure_reason,
                ..
            } => metric::mcp_server_init_records(metric::McpServerInit::from_name(
                server_name,
                init_failure_reason.as_deref(),
            )),
            EventType::EmptyResponseRetry { model, outcome } => {
                vec![metric::empty_response_retry_record(
                    metric::EmptyResponseRetry::from_id(model.as_deref(), (*outcome).into()),
                )]
            },
            EventType::RetryAttempt {
                upstream,
                retry_reason,
                attempt,
            } => vec![metric::retry_attempt_record(metric::RetryAttempt::from_attempt(
                *upstream,
                *retry_reason,
                *attempt,
            ))],
            EventType::RetryExhausted {
                upstream,
                final_error_kind,
            } => vec![metric::retry_exhausted_record(metric::RetryExhausted::new(
                *upstream,
                *final_error_kind,
            ))],
            EventType::MessageResponseError {
                model,
                reason,
                status_code,
                ..
            } => vec![metric::bedrock_request_error_record(
                metric::BedrockRequestError::from_stream_reason(model.as_deref(), reason.as_deref(), *status_code),
            )],
            EventType::CliSubcommandExecuted { subcommand } => {
                vec![metric::feature_used_record(metric::FeatureUsed::new(subcommand))]
            },
            EventType::ChatSlashCommandExecuted { command, .. } => {
                vec![metric::slash_command_invoked_record(metric::SlashCommandInvoked::new(
                    command,
                ))]
            },
            EventType::ChatSessionStarted { mode } => {
                let mode = if self.app_type.as_deref() == Some("ACP") {
                    metric::Mode::AcpExternal
                } else {
                    *mode
                };
                vec![metric::chat_session_started_record(metric::ChatSessionStarted::new(
                    mode,
                    metric::ClientApplication::from_name(self.client_application.as_deref()),
                ))]
            },
            EventType::ProcessHealthMetric {
                agent_kind,
                rss_mb,
                cpu_user_pct,
                cpu_system_pct,
                version,
                ..
            } => metric::process_health_records(metric::ProcessHealthSnapshot::new(
                *rss_mb,
                *cpu_user_pct,
                *cpu_system_pct,
                version,
                *agent_kind,
                metric::ProcessState::Other,
            )),
            EventType::GoalCompleted { terminal_state, .. } => {
                vec![metric::session_outcome_record(
                    metric::SessionOutcomeMetric::from_goal_terminal_state(terminal_state),
                )]
            },
            _ => self
                .ty
                .legacy_event_type()
                .and_then(legacy_metric_record)
                .into_iter()
                .collect(),
        }
    }

    fn otel_log_record(&self) -> Option<TelemetryLogRecord> {
        if let EventType::MeteringEvent {
            request_id,
            model,
            usage,
            unit,
            unit_plural,
        } = &self.ty
        {
            return Some(telemetry_log::metering_event_record(
                telemetry_log::MeteringEventLog::from_names(
                    request_id.as_deref(),
                    model.as_deref(),
                    self.client_application.as_deref(),
                    *usage,
                    unit,
                    unit_plural,
                ),
            ));
        }

        if let EventType::ToolUseSuggested {
            tool_use_id,
            tool_name,
            mcp_server_name,
            is_success,
            model,
            execution_duration,
            ..
        } = &self.ty
        {
            return Some(telemetry_log::tool_invoked_record(
                telemetry_log::ToolInvokedLog::from_names(
                    tool_use_id.as_deref(),
                    tool_name.as_deref(),
                    mcp_server_name.as_deref(),
                    *is_success,
                    model.as_deref(),
                    *execution_duration,
                ),
            ));
        }

        if let EventType::McpServerInit {
            server_name,
            init_failure_reason,
            ..
        } = &self.ty
        {
            return Some(telemetry_log::mcp_server_init_record(metric::McpServerInit::from_name(
                server_name,
                init_failure_reason.as_deref(),
            )));
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
    let reason_desc = redact_optional_telemetry_field(FieldClass::Other, args.reason_desc.clone());
    let request_id = comma_join(args.request_ids.iter().filter_map(|id| id.as_deref()));
    let message_id = comma_join(args.message_ids.iter().map(String::as_str));
    let time_to_first_chunks_ms = format_optional_f64s(&args.time_to_first_chunks_ms);
    let message_meta_tags = comma_join(args.message_meta_tags.iter().map(ToString::to_string));

    let turn = telemetry_log::UserTurnCompletedLog::new(
        conversation_id,
        metric::TurnOutcome::from(*result).result_kind(),
        args.is_subagent,
        args.user_prompt_length,
        args.assistant_response_length,
        args.user_turn_duration_seconds,
        args.follow_up_count,
    )
    .request_id(request_id.as_deref())
    .message_id(message_id.as_deref())
    .model_id(args.model.as_deref())
    .client_application(client_application.as_deref())
    .turn_failure_reason(failure_reason.as_deref())
    .reason_desc(reason_desc.as_deref())
    .status_code(args.status_code)
    .time_to_first_chunks_ms(time_to_first_chunks_ms.as_deref())
    .message_meta_tags(message_meta_tags.as_deref())
    .parent_tool_use_id(args.parent_tool_use_id.as_deref())
    .request_attempts(args.request_attempts)
    .total_tokens(args.total_tokens)
    .uncached_input_tokens(args.uncached_input_tokens)
    .output_tokens(args.output_tokens)
    .cache_read_input_tokens(args.cache_read_input_tokens)
    .cache_write_input_tokens(args.cache_write_input_tokens)
    .estimated_cost_usd(args.estimated_cost_usd);

    telemetry_log::user_turn_completed_record(turn)
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

pub(crate) fn estimated_cost_usd(model: &Option<String>, usage: TokenUsage) -> Option<f64> {
    metric::InvocationContext::from_names(model.as_deref(), None, false).estimated_cost_usd(usage)
}

fn token_usage_from_chat_added_message(data: &ChatAddedMessageParams) -> TokenUsage {
    TokenUsage::from_signed_counts(
        data.uncached_input_tokens.map(i64::from),
        data.cache_read_input_tokens.map(i64::from),
        data.cache_write_input_tokens.map(i64::from),
        data.output_tokens.map(i64::from),
    )
}

fn token_usage_from_turn_args(args: &RecordUserTurnCompletionArgs) -> TokenUsage {
    TokenUsage::from_signed_counts(
        args.uncached_input_tokens,
        args.cache_read_input_tokens,
        args.cache_write_input_tokens,
        args.output_tokens,
    )
}

fn turn_metric_context<'a>(
    model: &'a Option<String>,
    client_application: &'a Option<String>,
    app_type: &'a Option<String>,
    is_subagent: bool,
    tags: &[MessageMetaTag],
) -> metric::TurnMetricContext<'a> {
    metric::TurnMetricContext::new(model.as_deref(), client_application.as_deref())
        .app_type(app_type.as_deref())
        .subagent(is_subagent)
        .tangent(tags.contains(&MessageMetaTag::TangentMode))
        .generate_agent(tags.contains(&MessageMetaTag::GenerateAgent))
}

fn redact_telemetry_field(field_class: FieldClass, value: String) -> String {
    PiiRedactor.redact(field_class, &value).text
}

fn redact_optional_telemetry_field(field_class: FieldClass, value: Option<String>) -> Option<String> {
    value.map(|value| redact_telemetry_field(field_class, value))
}

impl From<ChatConversationType> for CodewhispererterminalChatConversationType {
    fn from(value: ChatConversationType) -> Self {
        match value {
            ChatConversationType::NotToolUse => Self::NotToolUse,
            ChatConversationType::ToolUse => Self::ToolUse,
        }
    }
}

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
            mcp_server_name: None,
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

#[cfg(test)]
mod tests {
    use kiro_telemetry::LegacyEventType;
    use kiro_telemetry::testing::{
        expect_log,
        expect_metric,
        expect_metric_attrs,
        metric_attr,
        metric_record,
    };

    use super::*;

    fn metadata_value<'a>(datum: &'a MetricDatum, key: &str) -> Option<&'a str> {
        datum
            .metadata()
            .iter()
            .find(|entry| entry.key() == Some(key))
            .and_then(|entry| entry.value())
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
            record.name == "pii_redaction_runs_total"
                && record
                    .attributes
                    .iter()
                    .any(|attr| attr.key == "channel" && attr.value == "legacy_toolkit")
        }));
        assert!(records.iter().any(|record| {
            record.name == "pii_redaction_matches_total"
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

        expect_metric(
            std::slice::from_ref(&record),
            metric::empty_response_retry(metric::ModelClass::Other, metric::Outcome::Recovered),
        );
    }

    #[test]
    fn emits_stream_timing_and_token_metrics() {
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                context_file_length: Some(2_000),
                model: Some("claude-4-sonnet".to_string()),
                time_to_first_chunk_ms: Some(125.0),
                time_between_chunks_ms: Some(vec![40.0, 250.0, 0.0, f64::NAN]),
                chat_conversation_type: Some(ChatConversationType::ToolUse),
                request_duration_seconds: Some(0.8),
                uncached_input_tokens: Some(10),
                cache_read_input_tokens: Some(2),
                cache_write_input_tokens: Some(3),
                output_tokens: Some(5),
                ..Default::default()
            },
        });
        event.app_type = Some("V2".to_string());
        event.client_application = Some("chat_cli_v2".to_string());

        let records = event.otel_metric_records();
        let metric_count = |name: &str| records.iter().filter(|record| record.name == name).count();
        assert_eq!(records.len(), 15);
        assert_eq!(metric_count("kiro_cli_user_turns"), 1);
        assert_eq!(metric_count("model_invocations_total"), 1);
        assert_eq!(metric_count("kiro_cli_time_to_first_chunk_ms"), 1);
        assert_eq!(metric_count("chat_cli.bedrock.stream.ttft"), 1);
        assert_eq!(metric_count("chat_cli.bedrock.stream.inter_token_latency"), 2);
        assert_eq!(metric_count("chat_cli.bedrock.stream.duration"), 1);
        assert_eq!(metric_count("chat_cli.bedrock.request.duration"), 1);
        assert_eq!(metric_count("kiro_cli_tokens_consumed"), 4);
        assert_eq!(metric_count("kiro_cli_estimated_cost_usd"), 1);
        assert_eq!(metric_count("kiro_cli_pricing_table_active"), 1);
        assert_eq!(metric_count("kiro_cli_cache_hit_ratio"), 1);

        let user_turns = expect_metric(
            &records,
            metric::user_turns(
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                metric::ResultKind::Success,
                false,
                metric::Mode::Interactive,
            ),
        );
        expect_metric_attrs(user_turns, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v2"),
            ("result", "success"),
            ("mode", "interactive"),
        ]);
        let invocation = expect_metric(&records, metric::model_invocation(metric::ModelClass::AnthropicSonnet));
        expect_metric_attrs(invocation, &[("model_class", "anthropic_sonnet")]);
        let ttfc = expect_metric(
            &records,
            metric::time_to_first_chunk_ms(
                125.0,
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                false,
            ),
        );
        expect_metric_attrs(ttfc, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v2"),
            ("is_subagent", "false"),
        ]);
        let alarm_ttft = expect_metric(
            &records,
            metric::bedrock_stream_ttft(
                0.125,
                metric::ModelClass::AnthropicSonnet,
                metric::PromptSizeBucket::Small,
                true,
            ),
        );
        expect_metric_attrs(alarm_ttft, &[
            ("model_class", "anthropic_sonnet"),
            ("prompt_size_bucket", "small"),
            ("tools_enabled", "true"),
        ]);
        expect_metric(
            &records,
            metric::bedrock_stream_inter_token_latency(0.04, metric::ModelClass::AnthropicSonnet),
        );
        expect_metric(
            &records,
            metric::bedrock_stream_inter_token_latency(0.25, metric::ModelClass::AnthropicSonnet),
        );
        expect_metric(
            &records,
            metric::bedrock_stream_duration(
                0.8,
                metric::ModelClass::AnthropicSonnet,
                telemetry_log::CompletionReason::ToolUse,
            ),
        );
        expect_metric(
            &records,
            metric::bedrock_request_duration(
                0.8,
                metric::ModelClass::AnthropicSonnet,
                metric::Operation::Stream,
                metric::Outcome::Success,
            ),
        );

        let token_records = records
            .iter()
            .filter(|record| record.name == "kiro_cli_tokens_consumed")
            .collect::<Vec<_>>();
        assert_eq!(token_records.len(), 4);
        for (token_type, value) in [
            (metric::TokenType::InputUncached, 10),
            (metric::TokenType::InputCacheRead, 2),
            (metric::TokenType::InputCacheWrite, 3),
            (metric::TokenType::Output, 5),
        ] {
            expect_metric(
                &records,
                metric::tokens_consumed(
                    value,
                    metric::ModelClass::AnthropicSonnet,
                    token_type,
                    metric::ClientApplication::ChatCliV2,
                    false,
                ),
            );
        }

        expect_metric(
            &records,
            metric::cache_hit_ratio(
                2.0 / 12.0,
                metric::ModelClass::AnthropicSonnet,
                metric::ChatConversationKind::Interactive,
                metric::ClientApplication::ChatCliV2,
            ),
        );
        expect_metric(
            &records,
            metric::estimated_cost_usd(
                metric::InvocationContext::new(
                    metric::ModelClass::AnthropicSonnet,
                    metric::ClientApplication::ChatCliV2,
                    false,
                )
                .estimated_cost_usd(TokenUsage {
                    uncached_input_tokens: 10,
                    cache_read_input_tokens: 2,
                    cache_write_input_tokens: 3,
                    output_tokens: 5,
                })
                .expect("sonnet pricing is known"),
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                false,
            ),
        );
        expect_metric(
            &records,
            metric::pricing_table_active(kiro_telemetry::PRICING_TABLE_VERSION),
        );
    }

    #[test]
    fn chat_added_message_metrics_use_event_subagent_flag() {
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                model: Some("claude-4-sonnet".to_string()),
                time_to_first_chunk_ms: Some(125.0),
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                ..Default::default()
            },
        });
        event.client_application = Some("chat_cli_v2".to_string());
        event.is_subagent = true;

        let records = event.otel_metric_records();

        let context = metric::InvocationContext::new(
            metric::ModelClass::AnthropicSonnet,
            metric::ClientApplication::ChatCliV2,
            true,
        );
        expect_metric(
            &records,
            metric::user_turns_for_invocation(context, metric::ResultKind::Success, metric::Mode::Interactive),
        );
        expect_metric(&records, metric::time_to_first_chunk_ms_for_invocation(125.0, context));
        expect_metric(
            &records,
            metric::tokens_consumed(
                10,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::InputUncached,
                metric::ClientApplication::ChatCliV2,
                true,
            ),
        );
        expect_metric(
            &records,
            metric::tokens_consumed(
                5,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::Output,
                metric::ClientApplication::ChatCliV2,
                true,
            ),
        );
        expect_metric(
            &records,
            metric::estimated_cost_usd(
                context
                    .estimated_cost_usd(TokenUsage {
                        uncached_input_tokens: 10,
                        cache_read_input_tokens: 0,
                        cache_write_input_tokens: 0,
                        output_tokens: 5,
                    })
                    .expect("sonnet pricing is known"),
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                true,
            ),
        );
    }

    #[test]
    fn failed_stream_duration_uses_error_completion_reason() {
        let event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Failed,
            data: ChatAddedMessageParams {
                model: Some("claude-4-sonnet".to_string()),
                request_duration_seconds: Some(0.8),
                ..Default::default()
            },
        });

        let records = event.otel_metric_records();
        expect_metric(
            &records,
            metric::bedrock_stream_duration(
                0.8,
                metric::ModelClass::AnthropicSonnet,
                telemetry_log::CompletionReason::Error,
            ),
        );
    }

    #[test]
    fn emits_tool_aggregate_metrics() {
        let event = Event::new(EventType::ToolUseSuggested {
            conversation_id: "conversation".to_string(),
            utterance_id: Some("utterance".to_string()),
            user_input_id: None,
            tool_use_id: Some("tool-1".to_string()),
            tool_name: Some("fs_read".to_string()),
            mcp_server_name: None,
            is_accepted: true,
            is_trusted: true,
            is_success: Some(true),
            reason_desc: None,
            is_valid: Some(true),
            is_custom_tool: false,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: Some("claude-4-sonnet".to_string()),
            execution_duration: Some(Duration::from_millis(25)),
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        });

        let records = event.otel_metric_records();

        expect_metric(
            &records,
            metric::tool_call_total(metric::ToolOrigin::Builtin, Some("fs_read"), metric::Outcome::Success),
        );
        expect_metric(
            &records,
            metric::tool_invocations(metric::ToolOrigin::Builtin, metric::Outcome::Success),
        );
        expect_metric(
            &records,
            metric::tool_execution_duration_ms(25.0, metric::ToolOrigin::Builtin, true),
        );

        let log_record = event.otel_log_record().expect("tool fact log");
        expect_log(
            std::slice::from_ref(&log_record),
            telemetry_log::tool_invoked_record(telemetry_log::ToolInvokedLog::from_names(
                Some("tool-1"),
                Some("fs_read"),
                None,
                Some(true),
                Some("claude-4-sonnet"),
                Some(Duration::from_millis(25)),
            )),
        );
    }

    #[test]
    fn emits_mcp_server_init_metric_and_fact_log() {
        let event = Event::new(EventType::McpServerInit {
            conversation_id: "conversation".to_string(),
            server_name: "code".to_string(),
            init_failure_reason: None,
            number_of_tools: 3,
            all_tool_names: Some("read,write,search".to_string()),
            loaded_tool_names: Some("read,write".to_string()),
            all_tools_count: 3,
        });

        let records = event.otel_metric_records();
        expect_metric(
            &records,
            metric::mcp_server_init_total_record(metric::McpServerInit::from_name("code", None)),
        );
        expect_metric(
            &records,
            metric::mcp_server_connected_total_record(metric::McpServerInit::from_name("code", None))
                .expect("successful init"),
        );

        let log_record = event.otel_log_record().expect("mcp init fact log");
        expect_log(
            std::slice::from_ref(&log_record),
            telemetry_log::mcp_server_init_record(metric::McpServerInit::from_name("code", None)),
        );
    }

    #[test]
    fn buckets_mcp_server_init_failure_metric_dimensions() {
        let event = Event::new(EventType::McpServerInit {
            conversation_id: "conversation".to_string(),
            server_name: "local-server".to_string(),
            init_failure_reason: Some("request timed out while listing tools".to_string()),
            number_of_tools: 0,
            all_tool_names: None,
            loaded_tool_names: None,
            all_tools_count: 0,
        });

        let records = event.otel_metric_records();
        expect_metric(
            &records,
            metric::mcp_server_init_total_record(metric::McpServerInit::from_name(
                "local-server",
                Some("request timed out while listing tools"),
            )),
        );
        assert!(records.iter().all(|record| record.name != "mcp_server_connected_total"));

        let log_record = event.otel_log_record().expect("mcp init fact log");
        expect_log(
            std::slice::from_ref(&log_record),
            telemetry_log::mcp_server_init_record(metric::McpServerInit::from_name(
                "local-server",
                Some("request timed out while listing tools"),
            )),
        );
    }

    #[test]
    fn denied_mcp_tool_aggregate_omits_duration_metric() {
        let event = Event::new(EventType::ToolUseSuggested {
            conversation_id: "conversation".to_string(),
            utterance_id: Some("utterance".to_string()),
            user_input_id: None,
            tool_use_id: Some("tool-1".to_string()),
            tool_name: Some("custom_tool".to_string()),
            mcp_server_name: Some("local-server".to_string()),
            is_accepted: false,
            is_trusted: false,
            is_success: None,
            reason_desc: None,
            is_valid: None,
            is_custom_tool: true,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: Some("claude-4-sonnet".to_string()),
            execution_duration: None,
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        });

        let records = event.otel_metric_records();
        expect_metric(
            &records,
            metric::tool_invocations(metric::ToolOrigin::Mcp, metric::Outcome::Denied),
        );
        let log_record = event.otel_log_record().expect("tool fact log");
        expect_log(
            std::slice::from_ref(&log_record),
            telemetry_log::tool_invoked_record(telemetry_log::ToolInvokedLog::from_names(
                Some("tool-1"),
                Some("custom_tool"),
                Some("local-server"),
                None,
                Some("claude-4-sonnet"),
                None,
            )),
        );
        assert!(
            records
                .iter()
                .all(|record| record.name != "kiro_cli_tool_execution_duration_ms")
        );
    }

    #[test]
    fn aws_tool_aggregate_uses_aws_api_origin() {
        let event = Event::new(EventType::ToolUseSuggested {
            conversation_id: "conversation".to_string(),
            utterance_id: Some("utterance".to_string()),
            user_input_id: None,
            tool_use_id: Some("tool-1".to_string()),
            tool_name: Some("use_aws".to_string()),
            mcp_server_name: None,
            is_accepted: true,
            is_trusted: true,
            is_success: Some(true),
            reason_desc: None,
            is_valid: Some(true),
            is_custom_tool: false,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: Some("claude-4-sonnet".to_string()),
            execution_duration: Some(Duration::from_millis(10)),
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        });

        let records = event.otel_metric_records();

        expect_metric(
            &records,
            metric::tool_call_total(metric::ToolOrigin::AwsApi, Some("use_aws"), metric::Outcome::Success),
        );
        expect_metric(
            &records,
            metric::tool_invocations(metric::ToolOrigin::AwsApi, metric::Outcome::Success),
        );
        expect_metric(
            &records,
            metric::tool_execution_duration_ms(10.0, metric::ToolOrigin::AwsApi, true),
        );
    }

    #[test]
    fn mcp_tool_name_collision_uses_mcp_origin() {
        let event = Event::new(EventType::ToolUseSuggested {
            conversation_id: "conversation".to_string(),
            utterance_id: Some("utterance".to_string()),
            user_input_id: None,
            tool_use_id: Some("tool-1".to_string()),
            tool_name: Some("use_aws".to_string()),
            mcp_server_name: Some("local-server".to_string()),
            is_accepted: true,
            is_trusted: true,
            is_success: Some(true),
            reason_desc: None,
            is_valid: Some(true),
            is_custom_tool: true,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: Some("claude-4-sonnet".to_string()),
            execution_duration: Some(Duration::from_millis(10)),
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        });

        let records = event.otel_metric_records();

        let legacy_tool_call = metric_record(&records, "tool_call_total").expect("legacy aggregate tool metric");
        assert_eq!(metric_attr(legacy_tool_call, "tool_origin"), Some("mcp"));
        assert_eq!(metric_attr(legacy_tool_call, "builtin_tool_name"), None);

        let invocations = metric_record(&records, "kiro_cli_tool_invocations").expect("tool invocation metric");
        assert_eq!(metric_attr(invocations, "tool_origin"), Some("mcp"));

        let duration = metric_record(&records, "kiro_cli_tool_execution_duration_ms").expect("tool duration metric");
        assert_eq!(metric_attr(duration, "tool_origin"), Some("mcp"));
    }

    #[test]
    fn skips_cache_hit_ratio_without_input_tokens() {
        let event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                model: Some("claude-4-sonnet".to_string()),
                output_tokens: Some(5),
                ..Default::default()
            },
        });

        assert!(
            event
                .otel_metric_records()
                .iter()
                .all(|record| record.name != "kiro_cli_cache_hit_ratio")
        );
    }

    #[test]
    fn emits_user_turn_duration_metric() {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            args: RecordUserTurnCompletionArgs {
                model: Some("claude-4-opus".to_string()),
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                estimated_cost_usd: Some(0.25),
                user_turn_duration_seconds: 12,
                message_meta_tags: vec![MessageMetaTag::GenerateAgent],
                ..Default::default()
            },
        });
        event.app_type = Some("V2".to_string());

        let records = event.otel_metric_records();
        let duration = expect_metric(
            &records,
            metric::user_turn_duration_seconds(
                12.0,
                metric::ModelClass::AnthropicOpus,
                metric::ChatConversationKind::Interactive,
                false,
                metric::Mode::GenerateAgent,
            ),
        );
        expect_metric_attrs(duration, &[
            ("model_class", "anthropic_opus"),
            ("chat_conversation_type", "interactive"),
            ("is_subagent", "false"),
            ("mode", "generate_agent"),
        ]);
        assert!(records.iter().all(|record| record.name != "kiro_cli_user_turns"
            && record.name != "kiro_cli_tokens_consumed"
            && record.name != "kiro_cli_estimated_cost_usd"
            && record.name != "kiro_cli_pricing_table_active"));
    }

    #[test]
    fn emits_user_turn_completion_counter_economics_and_duration_metric() {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Failed,
            args: RecordUserTurnCompletionArgs {
                model: Some("claude-4-sonnet".to_string()),
                uncached_input_tokens: Some(10),
                cache_read_input_tokens: Some(2),
                output_tokens: Some(5),
                estimated_cost_usd: Some(0.25),
                user_turn_duration_seconds: 12,
                message_meta_tags: vec![MessageMetaTag::TangentMode],
                is_subagent: true,
                emit_user_turn_counter: true,
                ..Default::default()
            },
        });
        event.app_type = Some("V2".to_string());
        event.client_application = Some("kas".to_string());

        let records = event.otel_metric_records();
        let context = metric::InvocationContext::new(
            metric::ModelClass::AnthropicSonnet,
            metric::ClientApplication::ChatCliV3,
            true,
        );

        expect_metric(
            &records,
            metric::user_turns_for_invocation(context, metric::ResultKind::Failed, metric::Mode::Tangent),
        );
        expect_metric(
            &records,
            metric::tokens_consumed(
                10,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::InputUncached,
                metric::ClientApplication::ChatCliV3,
                true,
            ),
        );
        expect_metric(
            &records,
            metric::tokens_consumed(
                2,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::InputCacheRead,
                metric::ClientApplication::ChatCliV3,
                true,
            ),
        );
        expect_metric(
            &records,
            metric::tokens_consumed(
                5,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::Output,
                metric::ClientApplication::ChatCliV3,
                true,
            ),
        );
        expect_metric(
            &records,
            metric::estimated_cost_usd(
                0.25,
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV3,
                true,
            ),
        );
        expect_metric(
            &records,
            metric::pricing_table_active(kiro_telemetry::PRICING_TABLE_VERSION),
        );
        expect_metric(
            &records,
            metric::user_turn_duration_seconds_for_invocation(
                12.0,
                context,
                metric::ChatConversationKind::Subagent,
                metric::Mode::Tangent,
            ),
        );
    }

    #[test]
    fn emits_context_usage_metric() {
        let mut event = Event::new(EventType::ContextUsagePercentage {
            model: Some("claude-4-sonnet".to_string()),
            percentage: 42.5,
        });
        event.client_application = Some("chat_cli_v2".to_string());
        event.is_subagent = true;

        let records = event.otel_metric_records();
        let context_usage = expect_metric(
            &records,
            metric::context_usage_percentage_record(metric::ContextUsageMetric::from_names(
                42.5,
                Some("claude-4-sonnet"),
                Some("chat_cli_v2"),
                true,
            ))
            .expect("valid context usage metric"),
        );
        expect_metric_attrs(context_usage, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v2"),
            ("is_subagent", "true"),
        ]);
    }

    #[test]
    fn kas_context_usage_metric_uses_v3_client_attribution() {
        let mut event = Event::new(EventType::ContextUsagePercentage {
            model: Some("claude-4-sonnet".to_string()),
            percentage: 66.0,
        });
        event.client_application = Some("kas".to_string());

        let records = event.otel_metric_records();
        let context_usage = expect_metric(
            &records,
            metric::context_usage_percentage_record(metric::ContextUsageMetric::from_names(
                66.0,
                Some("claude-4-sonnet"),
                Some("kas"),
                false,
            ))
            .expect("valid context usage metric"),
        );
        expect_metric_attrs(context_usage, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v3"),
            ("is_subagent", "false"),
        ]);
    }

    #[test]
    fn drops_invalid_context_usage_metric_values() {
        let event = Event::new(EventType::ContextUsagePercentage {
            model: Some("claude-4-sonnet".to_string()),
            percentage: -1.0,
        });

        assert!(event.otel_metric_records().is_empty());
    }

    #[test]
    fn emits_model_invocation_otel_metric() {
        let event = Event::new(EventType::ModelInvocation {
            model: Some("gpt-5-codex".to_string()),
        });

        let record = event.otel_metric_record().expect("model invocation metric");
        expect_metric(
            std::slice::from_ref(&record),
            metric::model_invocation(metric::ModelClass::OpenAiGpt5),
        );
    }

    fn process_health_event_type(agent_kind: metric::AgentKind) -> EventType {
        EventType::ProcessHealthMetric {
            agent_kind,
            rss_mb: 128.0,
            heap_used_mb: 64.0,
            peak_rss_mb: 256.0,
            cpu_user_pct: 12.5,
            cpu_system_pct: 7.5,
            last_render_ms: 4.0,
            max_render_ms: 10.0,
            renders_per_min: 30,
            full_redraws_per_min: 1,
            yoga_node_count: 100,
            event_loop_p99_ms: Some(5.0),
            input_latency_p95_ms: Some(8.0),
            session_duration_sec: 90,
            cpu_cores: 8,
            total_memory_mb: 32768,
            terminal: "iTerm.app".to_string(),
            session_id: Some("session".to_string()),
            version: "2.4.0".to_string(),
            platform: "darwin".to_string(),
        }
    }

    #[test]
    fn typed_process_health_agent_kind_round_trips_as_schema_string() {
        let value = serde_json::to_value(process_health_event_type(metric::AgentKind::Kas))
            .expect("process health event should serialize");
        assert_eq!(value.get("agent_kind"), Some(&serde_json::json!("kas")));

        let mut legacy = value;
        legacy["agent_kind"] = serde_json::Value::Null;
        let decoded = serde_json::from_value::<EventType>(legacy)
            .expect("legacy process health event with null agent kind should deserialize");
        assert!(matches!(decoded, EventType::ProcessHealthMetric {
            agent_kind: metric::AgentKind::Other,
            ..
        }));

        let mut legacy = serde_json::to_value(process_health_event_type(metric::AgentKind::Kas))
            .expect("process health event should serialize");
        legacy
            .as_object_mut()
            .expect("event should serialize to an object")
            .remove("agent_kind");
        let decoded = serde_json::from_value::<EventType>(legacy)
            .expect("legacy process health event with missing agent kind should deserialize");
        assert!(matches!(decoded, EventType::ProcessHealthMetric {
            agent_kind: metric::AgentKind::Other,
            ..
        }));
    }

    #[test]
    fn emits_process_health_otel_metrics() {
        let event = Event::new(process_health_event_type(metric::AgentKind::Kas));

        let records = event.otel_metric_records();
        let rss = expect_metric(
            &records,
            metric::process_memory_rss(
                128.0 * 1024.0 * 1024.0,
                metric::VersionMinorBucket::Current,
                metric::AgentKind::Kas,
            ),
        );
        expect_metric_attrs(rss, &[("version_minor_bucket", "current"), ("agent_kind", "kas")]);
        let cpu = expect_metric(
            &records,
            metric::process_cpu_utilization(
                0.2,
                metric::VersionMinorBucket::Current,
                metric::AgentKind::Kas,
                metric::ProcessState::Other,
            ),
        );
        expect_metric_attrs(cpu, &[
            ("version_minor_bucket", "current"),
            ("agent_kind", "kas"),
            ("state", "_other_"),
        ]);
    }

    #[test]
    fn drops_invalid_process_health_otel_values() {
        let event = Event::new(EventType::ProcessHealthMetric {
            agent_kind: metric::AgentKind::Other,
            rss_mb: f64::NAN,
            heap_used_mb: 0.0,
            peak_rss_mb: 0.0,
            cpu_user_pct: f64::INFINITY,
            cpu_system_pct: 1.0,
            last_render_ms: 0.0,
            max_render_ms: 0.0,
            renders_per_min: 0,
            full_redraws_per_min: 0,
            yoga_node_count: 0,
            event_loop_p99_ms: None,
            input_latency_p95_ms: None,
            session_duration_sec: 0,
            cpu_cores: 0,
            total_memory_mb: 0,
            terminal: "unknown".to_string(),
            session_id: None,
            version: String::new(),
            platform: "linux".to_string(),
        });

        let records = event.otel_metric_records();
        assert!(records.iter().all(|record| {
            record.name != "chat_cli.process.memory.rss" && record.name != "chat_cli.process.cpu.utilization"
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
                total_tokens: Some(17),
                uncached_input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_input_tokens: Some(2),
                cache_write_input_tokens: Some(3),
                estimated_cost_usd: Some(0.00010785),
                user_turn_duration_seconds: 3,
                follow_up_count: 1,
                message_meta_tags: vec![MessageMetaTag::Compact],
                parent_tool_use_id: Some("parent-tool".to_string()),
                request_attempts: Some(2),
                ..Default::default()
            },
        });
        event.client_application = Some("chat_cli_v2".to_string());

        let record = event.otel_log_record().expect("log-backed legacy event");
        let expected =
            telemetry_log::UserTurnCompletedLog::new("conversation", metric::ResultKind::Failed, false, 7, 11, 3, 1)
                .request_id(Some("request-1"))
                .message_id(Some("message-1"))
                .model_id(Some("claude-4-sonnet"))
                .client_application(Some("chat_cli_v2"))
                .turn_failure_reason(Some("ServiceFailure"))
                .reason_desc(Some("failed for [REDACTED:email]"))
                .status_code(Some(500))
                .time_to_first_chunks_ms(Some("25.000,null"))
                .message_meta_tags(Some("Compact"))
                .parent_tool_use_id(Some("parent-tool"))
                .request_attempts(Some(2))
                .total_tokens(Some(17))
                .uncached_input_tokens(Some(10))
                .output_tokens(Some(5))
                .cache_read_input_tokens(Some(2))
                .cache_write_input_tokens(Some(3))
                .estimated_cost_usd(Some(0.00010785));

        expect_log(
            std::slice::from_ref(&record),
            telemetry_log::user_turn_completed_record(expected),
        );
    }

    #[test]
    fn kas_turn_completion_log_uses_v3_client_attribution() {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            args: RecordUserTurnCompletionArgs {
                model: Some("claude-4-sonnet".to_string()),
                user_prompt_length: 7,
                assistant_response_length: 11,
                user_turn_duration_seconds: 3,
                follow_up_count: 1,
                ..Default::default()
            },
        });
        event.client_application = Some("kas".to_string());

        let record = event.otel_log_record().expect("turn completion fact log");
        let expected =
            telemetry_log::UserTurnCompletedLog::new("conversation", metric::ResultKind::Success, false, 7, 11, 3, 1);
        let expected = expected
            .model_id(Some("claude-4-sonnet"))
            .client_application(Some("kas"));

        expect_log(
            std::slice::from_ref(&record),
            telemetry_log::user_turn_completed_record(expected),
        );
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

        expect_log(
            std::slice::from_ref(&record),
            telemetry_log::subagent_invoked("code-review").build(),
        );
    }

    #[test]
    fn emits_bounded_request_error_metric() {
        let event = Event::new(EventType::MessageResponseError {
            conversation_id: "conversation".to_string(),
            context_file_length: None,
            result: TelemetryResult::Failed,
            reason: Some("ServiceUnavailable".to_string()),
            reason_desc: None,
            status_code: Some(503),
            request_id: Some("request".to_string()),
            message_id: Some("message".to_string()),
            model: Some("claude-4-opus".to_string()),
        });

        let records = event.otel_metric_records();

        expect_metric(
            &records,
            metric::bedrock_request_error(
                metric::ModelClass::AnthropicOpus,
                metric::Operation::Stream,
                metric::ErrorKind::ServerError,
                metric::StatusClass::Class5xx,
            ),
        );
    }

    #[test]
    fn emits_cli_subcommand_feature_metric() {
        let event = Event::new(EventType::CliSubcommandExecuted {
            subcommand: "Version".to_string(),
        });

        let record = event.otel_metric_record().expect("feature usage metric");
        expect_metric(std::slice::from_ref(&record), metric::feature_used("Version"));
    }

    #[test]
    fn emits_chat_slash_command_metric() {
        let event = Event::new(EventType::ChatSlashCommandExecuted {
            conversation_id: "conversation".to_string(),
            command: "/Model".to_string(),
            subcommand: Some("list".to_string()),
            result: TelemetryResult::Succeeded,
            reason: None,
        });

        let record = event.otel_metric_record().expect("slash command metric");
        expect_metric(std::slice::from_ref(&record), metric::slash_command_invoked("/Model"));
    }

    #[test]
    fn emits_chat_session_started_metric() {
        let mut event = Event::new(EventType::ChatSessionStarted {
            mode: metric::Mode::Plan,
        });
        event.client_application = Some("chat_cli_v2".to_string());
        event.app_type = Some("V2".to_string());

        let record = event.otel_metric_record().expect("chat session start metric");

        expect_metric(
            std::slice::from_ref(&record),
            metric::chat_session_started(metric::Mode::Plan, metric::ClientApplication::ChatCliV2),
        );
    }

    #[test]
    fn typed_lifecycle_events_round_trip_as_schema_strings() {
        let value = serde_json::to_value(EventType::CliSessionStarted {
            os_type: metric::OsType::Macos,
            install_source: metric::InstallSource::Internal,
        })
        .expect("session start event should serialize");
        assert_eq!(
            value,
            serde_json::json!({
                "type": "cliSessionStarted",
                "os_type": "macos",
                "install_source": "internal",
            })
        );

        let decoded = serde_json::from_value::<EventType>(serde_json::json!({
            "type": "chatSessionStarted",
            "mode": "kiro_planner",
        }))
        .expect("chat session event should deserialize");
        assert!(matches!(decoded, EventType::ChatSessionStarted {
            mode: metric::Mode::Plan
        }));

        let decoded = serde_json::from_value::<EventType>(serde_json::json!({
            "type": "chatSessionStarted",
            "mode": null,
        }))
        .expect("legacy chat session event with null mode should deserialize");
        assert!(matches!(decoded, EventType::ChatSessionStarted {
            mode: metric::Mode::Interactive
        }));

        let decoded = serde_json::from_value::<EventType>(serde_json::json!({
            "type": "cliSessionCompleted",
            "exit_reason": "unexpected",
            "agent_kind": "v3",
        }))
        .expect("CLI completion event should deserialize");
        assert!(matches!(decoded, EventType::CliSessionCompleted {
            exit_reason: metric::ExitReason::Other,
            agent_kind: metric::AgentKind::Kas,
        }));
    }

    #[test]
    fn emits_cli_session_started_metric_with_bounded_dimensions() {
        let mut event = Event::new(EventType::CliSessionStarted {
            os_type: metric::OsType::Macos,
            install_source: metric::InstallSource::Internal,
        });
        event.client_application = Some("chat_cli_v2".to_string());

        let record = event.otel_metric_record().expect("CLI session-start metric");

        expect_metric(
            std::slice::from_ref(&record),
            metric::cli_session_started(
                metric::OsType::Macos,
                metric::InstallSource::Internal,
                metric::ClientApplication::ChatCliV2,
            ),
        );
    }

    #[test]
    fn does_not_count_user_login_as_cli_session_start() {
        let event = Event::new(EventType::UserLoggedIn {});

        assert!(event.otel_metric_records().is_empty());
    }

    #[test]
    fn emits_cli_session_completed_metric_with_bounded_dimensions() {
        let event = Event::new(EventType::CliSessionCompleted {
            exit_reason: metric::ExitReason::Clean,
            agent_kind: metric::AgentKind::Kas,
        });

        let record = event.otel_metric_record().expect("CLI session completion metric");

        expect_metric(
            std::slice::from_ref(&record),
            metric::cli_session_completed(metric::ExitReason::Clean, metric::AgentKind::Kas),
        );
    }

    #[test]
    fn emits_daily_heartbeat_metric_with_v3_client_and_install_method() {
        let mut event = Event::new(EventType::DailyHeartbeat {
            install_method: Some("toolbox (2.0.0)".to_string()),
        });
        event.client_application = Some("kas".to_string());

        let record = event.otel_metric_record().expect("daily heartbeat metric");

        expect_metric(
            std::slice::from_ref(&record),
            metric::daily_heartbeat(metric::ClientApplication::ChatCliV3, metric::InstallSource::Internal),
        );
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

        expect_log(
            std::slice::from_ref(&record),
            telemetry_log::conversation_completed(
                "conversation",
                "conversation",
                telemetry_log::CompletionReason::Stop,
            ),
        );
    }

    #[test]
    fn exposes_canonical_legacy_event_type() {
        let event = EventType::GoalCompleted {
            conversation_id: None,
            terminal_state: "completed".to_string(),
            iterations: 2,
            max_iterations: 4,
            duration_sec: 12,
        };

        assert_eq!(event.legacy_event_type(), Some(LegacyEventType::GoalCompleted));
    }

    #[test]
    fn emits_goal_completed_session_outcome_metric() {
        let event = Event::new(EventType::GoalCompleted {
            conversation_id: Some("conversation".to_string()),
            terminal_state: "completed".to_string(),
            iterations: 2,
            max_iterations: 4,
            duration_sec: 12,
        });

        let record = event.otel_metric_record().expect("goal outcome metric");
        expect_metric(
            std::slice::from_ref(&record),
            metric::session_outcome(metric::SessionOutcome::TaskCompleted),
        );
    }

    #[test]
    fn buckets_goal_terminal_states_for_session_outcome_metric() {
        for (terminal_state, expected_outcome) in [
            ("cancelled", metric::SessionOutcome::UserQuit),
            ("exhausted", metric::SessionOutcome::Timeout),
            ("reinjection_failed", metric::SessionOutcome::Error),
            ("panic", metric::SessionOutcome::Crash),
            ("surprising_state", metric::SessionOutcome::Other),
        ] {
            let event = Event::new(EventType::GoalCompleted {
                conversation_id: Some("conversation".to_string()),
                terminal_state: terminal_state.to_string(),
                iterations: 1,
                max_iterations: 4,
                duration_sec: 5,
            });

            let record = event.otel_metric_record().expect("goal outcome metric");
            expect_metric(std::slice::from_ref(&record), metric::session_outcome(expected_outcome));
        }
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
        event.client_application = Some("chat_cli_v2".to_string());

        assert!(event.into_metric_datum().is_none());

        let mut event = Event::new(EventType::MeteringEvent {
            request_id: Some("request".to_string()),
            model: Some("claude-4-sonnet".to_string()),
            usage: 1.25,
            unit: "credit".to_string(),
            unit_plural: "credits".to_string(),
        });
        event.client_application = Some("chat_cli_v2".to_string());

        let record = event.otel_log_record().expect("metering event log record");

        expect_log(
            std::slice::from_ref(&record),
            telemetry_log::metering_event_record(telemetry_log::MeteringEventLog::from_names(
                Some("request"),
                Some("claude-4-sonnet"),
                Some("chat_cli_v2"),
                1.25,
                "credit",
                "credits",
            )),
        );
    }

    #[test]
    fn kas_metering_log_uses_v3_client_attribution() {
        let mut event = Event::new(EventType::MeteringEvent {
            request_id: Some("request".to_string()),
            model: Some("claude-4-sonnet".to_string()),
            usage: 1.25,
            unit: "credit".to_string(),
            unit_plural: "credits".to_string(),
        });
        event.client_application = Some("kas".to_string());

        let record = event.otel_log_record().expect("metering event log record");

        expect_log(
            std::slice::from_ref(&record),
            telemetry_log::metering_event_record(telemetry_log::MeteringEventLog::from_names(
                Some("request"),
                Some("claude-4-sonnet"),
                Some("kas"),
                1.25,
                "credit",
                "credits",
            )),
        );
    }

    #[test]
    fn emits_empty_response_retry_as_otel_metric_only() {
        for (retry_outcome, metric_outcome) in [
            (EmptyResponseRetryOutcome::Recovered, metric::Outcome::Recovered),
            (EmptyResponseRetryOutcome::StillEmpty, metric::Outcome::StillEmpty),
        ] {
            let event = Event::new(EventType::EmptyResponseRetry {
                model: Some("claude-4-sonnet".to_string()),
                outcome: retry_outcome,
            });

            assert!(event.clone().into_metric_datum().is_none());

            let record = event.otel_metric_record().expect("empty-response retry metric");

            expect_metric(
                std::slice::from_ref(&record),
                metric::empty_response_retry(metric::ModelClass::AnthropicSonnet, metric_outcome),
            );
            assert!(event.otel_log_record().is_none());
        }
    }

    #[test]
    fn emits_retry_attempt_as_otel_metric_only() {
        let event = Event::new(EventType::RetryAttempt {
            upstream: metric::Upstream::Rts,
            retry_reason: metric::RetryReason::Other,
            attempt: 3,
        });

        assert!(event.clone().into_metric_datum().is_none());

        let record = event.otel_metric_record().expect("retry attempt metric");

        expect_metric(
            std::slice::from_ref(&record),
            metric::retry_attempt(
                metric::Upstream::Rts,
                metric::RetryReason::Other,
                metric::AttemptNumberBucket::ThreePlus,
            ),
        );
        assert!(event.otel_log_record().is_none());
    }

    #[test]
    fn emits_retry_exhausted_as_otel_metric_only() {
        let event = Event::new(EventType::RetryExhausted {
            upstream: metric::Upstream::Rts,
            final_error_kind: metric::ErrorKind::Throttling,
        });

        assert!(event.clone().into_metric_datum().is_none());

        let record = event.otel_metric_record().expect("retry exhausted metric");

        expect_metric(
            std::slice::from_ref(&record),
            metric::retry_exhausted(metric::Upstream::Rts, metric::ErrorKind::Throttling),
        );
        assert!(event.otel_log_record().is_none());
    }
}
