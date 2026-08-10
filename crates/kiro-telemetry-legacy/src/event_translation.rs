use amzn_toolkit_telemetry_client::types::MetricDatum;
use kiro_telemetry::{
    FieldClass,
    MetricRecord,
    PiiRedactor,
    TokenUsage,
    metric,
};
use kiro_telemetry_host::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    ChatConversationType,
    Event,
    EventType,
    MessageMetaTag,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    TelemetryResult,
};

use crate::definitions::IntoMetricDatum;
use crate::definitions::metrics::{
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
    CodewhispererterminalRecordUserTurnCompletion,
    CodewhispererterminalRefreshCredentials,
    CodewhispererterminalToolUseSuggested,
    CodewhispererterminalUiModeChanged,
    CodewhispererterminalUiModeDefaultChanged,
    CodewhispererterminalUiModeSessionStart,
    CodewhispererterminalUserLoggedIn,
    KirocliGoalCompleted,
    KirocliSubagentInvocation,
    KirocliVoiceInput,
};
use crate::definitions::types::{
    CodewhispererterminalChatConversationType,
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

pub fn event_to_metric_datum(event: Event) -> Option<MetricDatum> {
    let app_type_enum = event.app_type.as_deref().and_then(|s| match s {
        "V1" => Some(KirocliAppType::V1),
        "V2" => Some(KirocliAppType::V2),
        "ACP" => Some(KirocliAppType::Acp),
        _ => None,
    });
    match event.ty {
        EventType::UserLoggedIn {} => Some(
            CodewhispererterminalUserLoggedIn {
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
                codewhispererterminal_subcommand: Some(subcommand.into()),
                codewhispererterminal_in_cloudshell: None,
                codewhispererterminal_client_application: event.client_application.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
                sso_region: event.sso_region.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
                amazonq_conversation_id: Some(conversation_id.into()),
                codewhispererterminal_in_cloudshell: None,
                codewhispererterminal_model: model.map(Into::into),
            }
            .into_metric_datum(),
        ),
        EventType::ChatEnd { conversation_id, model } => Some(
            AmazonqEndChat {
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                amazonq_conversation_id: Some(conversation_id.into()),
                request_id: request_id.map(Into::into),
                codewhispererterminal_utterance_id: message_id.map(Into::into),
                credential_start_url: event.credential_start_url.map(Into::into),
                sso_region: event.sso_region.map(Into::into),
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
                codewhispererterminal_client_application: event.client_application.map(Into::into),
                kirocli_app_type: app_type_enum.clone(),
                kirocli_acp_client_name: event.acp_client_name.map(Into::into),
                kirocli_acp_client_version: event.acp_client_version.map(Into::into),
                codewhispererterminal_total_tokens: total_tokens.map(|v| v as i64).map(Into::into),
                codewhispererterminal_uncached_input_tokens: uncached_input_tokens.map(|v| v as i64).map(Into::into),
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
                    model_invocation_count: _,
                    user_turn_duration_seconds,
                    follow_up_count,
                    user_prompt_length,
                    message_meta_tags,
                    is_subagent,
                    emit_user_turn_counter: _,
                    parent_tool_use_id,
                    request_attempts,
                    emit_turn_numeric_metrics: _,
                    model: _,
                },
        } => Some(
            CodewhispererterminalRecordUserTurnCompletion {
                create_time: event.created_time,
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

                credential_start_url: event.credential_start_url.map(Into::into),
                sso_region: event.sso_region.map(Into::into),
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
                kirocli_acp_client_name: event.acp_client_name.map(Into::into),
                kirocli_acp_client_version: event.acp_client_version.map(Into::into),
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
                create_time: event.created_time,
                value: Some(if is_forget {
                    entries_removed.unwrap_or(0) as f64
                } else {
                    duration_seconds as f64
                }),
                credential_start_url: event.credential_start_url.map(Into::into),
                sso_region: event.sso_region.map(Into::into),
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
                create_time: event.created_time,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                codewhispererterminal_client_application: event.client_application.map(Into::into),
                codewhispererterminal_aws_service_name: aws_service_name.map(Into::into),
                codewhispererterminal_aws_operation_name: aws_operation_name.map(Into::into),
                kirocli_app_type: app_type_enum.clone(),
                kirocli_acp_client_name: event.acp_client_name.map(Into::into),
                kirocli_acp_client_version: event.acp_client_version.map(Into::into),
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
                create_time: event.created_time,
                credential_start_url: event.credential_start_url.map(Into::into),
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
            mcp_server_source: _,
            init_failure_reason,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            all_tools_count,
        } => Some(
            CodewhispererterminalMcpServerInit {
                create_time: event.created_time,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                codewhispererterminal_client_application: event.client_application.map(Into::into),
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
                codewhispererterminal_mcp_server_all_tools_count: Some(CodewhispererterminalMcpServerAllToolsCount(
                    all_tools_count as i64,
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
                create_time: event.created_time,
                credential_start_url: event.credential_start_url.map(Into::into),
                value: None,
                amazonq_conversation_id: Some(conversation_id.into()),
                codewhispererterminal_agents_loaded_count: Some(agents_loaded_count.into()),
                codewhispererterminal_agents_failed_to_load_count: Some(agents_loaded_failed_count.into()),
                codewhispererterminal_legacy_profile_migration_executed: Some(legacy_profile_migration_executed.into()),
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
                create_time: event.created_time,
                value: None,
                source: Some(source.to_string().into()),
                amazon_q_profile_region: Some(amazonq_profile_region.into()),
                result: Some(result.to_string().into()),
                sso_region: sso_region.map(Into::into),
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                source: Some(source.to_string().into()),
                amazon_q_profile_region: Some(amazonq_profile_region.into()),
                result: Some(result.to_string().into()),
                sso_region: sso_region.map(Into::into),
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                amazonq_conversation_id: Some(conversation_id.into()),
                codewhispererterminal_context_file_length: context_file_length.map(|l| l as i64).map(Into::into),
                credential_start_url: event.credential_start_url.map(Into::into),
                sso_region: event.sso_region.map(Into::into),
                result: Some(result.to_string().into()),
                reason: reason.map(Into::into),
                reason_desc: redact_optional_telemetry_field(FieldClass::Other, reason_desc).map(Into::into),
                status_code: status_code.map(|v| v as i64).map(Into::into),
                request_id: request_id.map(Into::into),
                codewhispererterminal_utterance_id: message_id.map(Into::into),
                codewhispererterminal_client_application: event.client_application.map(Into::into),
                kirocli_app_type: app_type_enum,
                kirocli_acp_client_name: event.acp_client_name.map(Into::into),
                kirocli_acp_client_version: event.acp_client_version.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
                codewhispererterminal_in_cloudshell: None,
                codewhispererterminal_auth_method: Some(auth_method.into()),
                oauth_flow: Some(oauth_flow.into()),
                codewhispererterminal_error_type: Some(redact_telemetry_field(FieldClass::Other, error_type).into()),
                codewhispererterminal_error_code: error_code.map(Into::into),
            }
            .into_metric_datum(),
        ),
        EventType::DailyHeartbeat { .. } => Some(
            AmazonqcliDailyHeartbeat {
                create_time: event.created_time,
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                    create_time: event.created_time,
                    value: None,
                    amazonq_conversation_id: conversation_id.map(Into::into),
                    credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
                codewhispererterminal_ui_mode_from: Some(from.into()),
                codewhispererterminal_ui_mode_to: Some(to.into()),
                codewhispererterminal_ui_mode_change_source: Some(source.to_string().into()),
                amazonq_conversation_id: session_id.map(Into::into),
            }
            .into_metric_datum(),
        ),
        EventType::UiModeDefaultChanged { from, to, session_id } => Some(
            CodewhispererterminalUiModeDefaultChanged {
                create_time: event.created_time,
                value: None,
                credential_start_url: event.credential_start_url.map(Into::into),
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
                    create_time: event.created_time,
                    value: None,
                    amazonq_conversation_id: conversation_id.map(Into::into),
                    credential_start_url: event.credential_start_url.map(Into::into),
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
        | EventType::AutomaticRetryCompleted { .. }
        | EventType::ModelInvocation { .. }
        | EventType::CliSessionStarted { .. }
        | EventType::CliSessionCompleted { .. }
        | EventType::StartupDuration { .. }
        | EventType::StartupFailure { .. }
        | EventType::ChatSessionStarted { .. }
        | EventType::ProcessHealth { .. } => None,
    }
}

pub fn event_to_otel_metric_record(event: &Event) -> Option<MetricRecord> {
    event_to_otel_metric_records(event).into_iter().next()
}

pub fn event_to_otel_metric_records(event: &Event) -> Vec<MetricRecord> {
    let engine = event_engine(event);
    match &event.ty {
        EventType::UserLoggedIn {} => vec![metric::record_login_success(
            auth_method_from_start_url(event.credential_start_url.as_deref()),
            metric::AuthFlow::Unknown,
        )],
        EventType::AuthFailed {
            auth_method,
            oauth_flow,
            error_type,
            error_code,
        } => vec![metric::record_auth_failure(
            metric::AuthMethod::from_name(auth_method),
            metric::AuthFlow::from_name(oauth_flow),
            metric::AuthOperation::Login,
            auth_failure_reason(error_code.as_deref().or(Some(error_type))),
        )],
        EventType::RefreshCredentials {
            result,
            reason,
            oauth_flow,
            ..
        } => match result {
            TelemetryResult::Succeeded => Vec::new(),
            TelemetryResult::Failed | TelemetryResult::Cancelled => vec![metric::record_auth_failure(
                auth_method_from_start_url(event.credential_start_url.as_deref()),
                metric::AuthFlow::from_name(oauth_flow),
                metric::AuthOperation::Refresh,
                auth_failure_reason(reason.as_deref()),
            )],
        },
        EventType::CliSessionStarted { os_type, .. } => vec![metric::record_run_started(
            event_session_interface(event),
            engine,
            *os_type,
        )],
        EventType::CliSessionCompleted { exit_reason, .. } => vec![metric::record_run_outcome(
            event_session_interface(event),
            engine,
            current_os_type(),
            event
                .metric_context
                .run_outcome
                .unwrap_or_else(|| run_outcome(*exit_reason)),
        )],
        EventType::StartupDuration {
            duration_seconds,
            os_type,
        } => {
            metric::record_startup_duration_seconds(*duration_seconds, event_session_interface(event), engine, *os_type)
                .into_iter()
                .collect()
        },
        EventType::StartupFailure { os_type, failure_stage } => vec![metric::record_startup_failure(
            event_session_interface(event),
            engine,
            *os_type,
            *failure_stage,
        )],
        EventType::DailyHeartbeat { install_method } => vec![metric::record_daily_heartbeat(
            metric::ReleaseChannel::from_version(env!("CARGO_PKG_VERSION")),
            current_os_type(),
            metric::InstallSource::from_name(install_method.as_deref().unwrap_or("unknown")),
        )],
        EventType::ChatAddedMessage { result, data, .. } => {
            let mut records = Vec::new();
            if engine != metric::Engine::V3 {
                records.push(metric::record_model_invocation(engine, data.model.as_deref()));
            }
            records.extend(data.time_to_first_chunk_ms.and_then(|milliseconds| {
                metric::record_model_time_to_first_content_ms(milliseconds, engine, data.model.as_deref())
            }));
            records.extend(data.request_duration_seconds.and_then(|seconds| {
                metric::record_model_request_duration_seconds(
                    seconds,
                    engine,
                    data.model.as_deref(),
                    model_request_outcome(*result),
                )
            }));
            records.extend(canonical_token_metric_records(
                engine,
                data.model.as_deref(),
                token_usage_from_chat_added_message(data),
            ));
            records
        },
        EventType::RecordUserTurnCompletion { result, args, .. } => {
            let mut records = if engine == metric::Engine::V3 || (engine == metric::Engine::V1 && args.is_subagent) {
                canonical_token_metric_records(engine, args.model.as_deref(), token_usage_from_turn_completion(args))
            } else {
                Vec::new()
            };
            let session_interface = event_session_interface(event);
            if engine == metric::Engine::V3
                && session_interface == metric::SessionInterface::NoninteractiveCli
                && let Some(record) =
                    metric::record_model_invocations(engine, args.model.as_deref(), args.model_invocation_count)
            {
                records.push(record);
            }
            if args.is_subagent || !host_owns_turn_metrics(event) {
                return records;
            }
            let agent_mode = event
                .metric_context
                .agent_mode
                .unwrap_or_else(|| turn_agent_mode(&args.message_meta_tags));
            records.push(metric::record_user_turn_for_acp_client(
                session_interface,
                agent_mode,
                engine,
                event.acp_client_name.as_deref(),
            ));
            match result {
                TelemetryResult::Succeeded => {
                    records.extend(metric::record_user_turn_duration_seconds(
                        args.user_turn_duration_seconds as f64,
                        session_interface,
                        agent_mode,
                        engine,
                    ));
                },
                TelemetryResult::Failed => records.push(metric::record_turn_failure(
                    session_interface,
                    agent_mode,
                    engine,
                    turn_failure_reason(args.reason.as_deref(), args.status_code),
                )),
                TelemetryResult::Cancelled => {
                    records.push(metric::record_turn_cancelled(session_interface, agent_mode, engine));
                },
            }
            records
        },
        EventType::ContextUsagePercentage { .. } => Vec::new(),
        EventType::ModelInvocation { model } => {
            if engine == metric::Engine::V3 {
                return Vec::new();
            }
            vec![metric::record_model_invocation(engine, model.as_deref())]
        },
        EventType::ToolUseSuggested {
            tool_name,
            mcp_server_name,
            is_accepted,
            is_success,
            is_valid,
            is_custom_tool,
            execution_duration,
            aws_service_name,
            ..
        } => {
            if engine == metric::Engine::V3 {
                return Vec::new();
            }
            let tool = canonical_tool_metric(
                engine,
                tool_name.as_deref(),
                aws_service_name.as_deref(),
                *is_custom_tool,
                mcp_server_name.as_deref(),
                *is_accepted,
                *is_valid,
                *is_success,
                event.is_subagent,
            );
            let mut records = vec![metric::record_tool_call(tool)];
            records.extend(
                execution_duration.and_then(|duration| {
                    metric::record_tool_execution_duration_ms(duration.as_secs_f64() * 1000.0, tool)
                }),
            );
            records
        },
        EventType::McpServerInit {
            server_name,
            mcp_server_source,
            init_failure_reason,
            ..
        } => vec![metric::record_mcp_server_init(
            engine,
            *mcp_server_source,
            if init_failure_reason.is_some() {
                metric::McpInitOutcome::Failure
            } else {
                metric::McpInitOutcome::Success
            },
            Some(server_name),
            init_failure_reason.as_deref().map(mcp_error_kind),
            init_failure_reason.as_deref().map(mcp_failure_stage),
        )],
        EventType::EmptyResponseRetry { outcome, .. } => {
            metric::record_automatic_retries(1, engine, metric::RetryReason::EmptyResponse, match outcome {
                kiro_telemetry_host::EmptyResponseRetryOutcome::Recovered => metric::RetryOutcome::Recovered,
                kiro_telemetry_host::EmptyResponseRetryOutcome::StillEmpty => metric::RetryOutcome::Exhausted,
            })
            .into_iter()
            .collect()
        },
        EventType::AutomaticRetryCompleted {
            retry_reason,
            additional_attempts,
            outcome,
            ..
        } => metric::record_automatic_retries(*additional_attempts, engine, *retry_reason, *outcome)
            .into_iter()
            .collect(),
        EventType::MessageResponseError {
            model,
            reason,
            status_code,
            ..
        } => vec![metric::record_model_request_failure(
            engine,
            model.as_deref(),
            metric::ErrorKind::from_reason(reason.as_deref(), *status_code),
        )],
        EventType::CliSubcommandExecuted { subcommand } => {
            vec![metric::record_top_level_command(subcommand)]
        },
        EventType::ChatSlashCommandExecuted { command, .. } => {
            vec![metric::record_slash_command(command, engine)]
        },
        EventType::ChatSessionStarted { mode } => {
            if !host_owns_turn_metrics(event) {
                return Vec::new();
            }
            vec![metric::record_chat_session_started(
                event_session_interface(event),
                event
                    .metric_context
                    .agent_mode
                    .unwrap_or_else(|| metric::AgentMode::from_id(Some(mode.as_str()))),
                engine,
            )]
        },
        EventType::ProcessHealthMetric { .. } => Vec::new(),
        EventType::GoalCompleted { terminal_state, .. } => vec![metric::record_goal_outcome(
            engine,
            metric::GoalOutcome::from_terminal_state(terminal_state),
        )],
        EventType::UiModeSessionStart { ui_mode, .. } => {
            vec![metric::record_ui_mode_session_started(metric::UiMode::from_name(
                ui_mode,
            ))]
        },
        EventType::MeteringEvent {
            model,
            usage,
            unit,
            unit_plural,
            ..
        } if is_credit_unit(unit, unit_plural) => metric::record_credits_consumed(*usage, model.as_deref())
            .into_iter()
            .collect(),
        EventType::AgentContribution { .. }
        | EventType::ChatStart { .. }
        | EventType::ChatEnd { .. }
        | EventType::TangentModeSession { .. }
        | EventType::AgentConfigInit { .. }
        | EventType::DidSelectProfile { .. }
        | EventType::ProfileState { .. }
        | EventType::SubagentInvocation { .. }
        | EventType::VoiceInput { .. }
        | EventType::ModeChanged { .. }
        | EventType::UiModeChanged { .. }
        | EventType::UiModeDefaultChanged { .. }
        | EventType::ProcessHealth { .. }
        | EventType::MeteringEvent { .. } => Vec::new(),
    }
}

fn event_engine(event: &Event) -> metric::Engine {
    if let Some(engine) = event.engine {
        return engine;
    }
    match event.app_type.as_deref() {
        Some("V1") => metric::Engine::V1,
        Some("V2" | "ACP") => metric::Engine::V2,
        Some("KAS") => metric::Engine::V3,
        _ => event
            .client_application
            .as_deref()
            .map_or(metric::Engine::Unknown, metric::Engine::from_name),
    }
}

fn event_session_interface(event: &Event) -> metric::SessionInterface {
    if let Some(session_interface) = event.session_interface {
        session_interface
    } else if event.app_type.as_deref() == Some("ACP") || event.client_application.as_deref() == Some("acp_external") {
        metric::SessionInterface::ExternalAcp
    } else {
        metric::SessionInterface::InteractiveCli
    }
}

fn host_owns_turn_metrics(event: &Event) -> bool {
    event_session_interface(event) != metric::SessionInterface::InteractiveCli
        || event_engine(event) == metric::Engine::V1
}

fn current_os_type() -> metric::OsType {
    os_type_from_name(std::env::consts::OS)
}

fn os_type_from_name(value: &str) -> metric::OsType {
    match value.trim().to_ascii_lowercase().as_str() {
        "darwin" | "macos" => metric::OsType::Macos,
        "windows" | "win32" => metric::OsType::Windows,
        "linux" => metric::OsType::Linux,
        _ => metric::OsType::Other,
    }
}

fn auth_method_from_start_url(start_url: Option<&str>) -> metric::AuthMethod {
    match start_url {
        Some("https://view.awsapps.com/start") => metric::AuthMethod::BuilderId,
        Some(url) if !url.trim().is_empty() => metric::AuthMethod::IdentityCenter,
        _ => metric::AuthMethod::Unknown,
    }
}

fn auth_failure_reason(value: Option<&str>) -> metric::AuthFailureReason {
    let value = value.unwrap_or_default().to_ascii_lowercase();
    if value.contains("denied") || value.contains("forbidden") {
        metric::AuthFailureReason::AuthorizationDenied
    } else if value.contains("expired") || value.contains("invalid_token") || value.contains("credential") {
        metric::AuthFailureReason::InvalidOrExpiredCredential
    } else if value.contains("timeout") || value.contains("timed out") {
        metric::AuthFailureReason::Timeout
    } else if value.contains("network") || value.contains("connection") || value.contains("dns") {
        metric::AuthFailureReason::Network
    } else if value.contains("config") {
        metric::AuthFailureReason::Configuration
    } else if value.contains("storage") || value.contains("keychain") {
        metric::AuthFailureReason::Storage
    } else if value.contains("service") || value.contains("server") {
        metric::AuthFailureReason::ServiceError
    } else {
        metric::AuthFailureReason::Unknown
    }
}

fn run_outcome(exit_reason: metric::ExitReason) -> metric::RunOutcome {
    match exit_reason {
        metric::ExitReason::Clean => metric::RunOutcome::Success,
        metric::ExitReason::UserInterrupt => metric::RunOutcome::UserInterrupt,
        metric::ExitReason::Crash
        | metric::ExitReason::Oom
        | metric::ExitReason::HangTimeout
        | metric::ExitReason::AuthFailure
        | metric::ExitReason::UpstreamOutage => metric::RunOutcome::Failure,
        metric::ExitReason::Other => metric::RunOutcome::Unknown,
    }
}

fn model_request_outcome(result: TelemetryResult) -> metric::ModelRequestOutcome {
    match result {
        TelemetryResult::Succeeded => metric::ModelRequestOutcome::Success,
        TelemetryResult::Failed => metric::ModelRequestOutcome::Failure,
        TelemetryResult::Cancelled => metric::ModelRequestOutcome::Cancelled,
    }
}

fn turn_agent_mode(message_meta_tags: &[MessageMetaTag]) -> metric::AgentMode {
    if message_meta_tags.contains(&MessageMetaTag::GenerateAgent)
        || message_meta_tags.contains(&MessageMetaTag::TangentMode)
    {
        metric::AgentMode::Custom
    } else {
        metric::AgentMode::Default
    }
}

fn turn_failure_reason(reason: Option<&str>, status_code: Option<u16>) -> metric::TurnFailureReason {
    match metric::ErrorKind::from_reason(reason, status_code) {
        metric::ErrorKind::ContextLimit => metric::TurnFailureReason::ContextLimit,
        metric::ErrorKind::Timeout => metric::TurnFailureReason::Timeout,
        metric::ErrorKind::ModelError
        | metric::ErrorKind::Throttling
        | metric::ErrorKind::Validation
        | metric::ErrorKind::ServerError
        | metric::ErrorKind::Connection
        | metric::ErrorKind::AccessDenied => metric::TurnFailureReason::ModelError,
        metric::ErrorKind::Other => metric::TurnFailureReason::Unknown,
    }
}

#[allow(clippy::too_many_arguments)]
fn canonical_tool_metric<'a>(
    engine: metric::Engine,
    tool_name: Option<&'a str>,
    aws_service_name: Option<&str>,
    is_custom_tool: bool,
    mcp_server_name: Option<&str>,
    is_accepted: bool,
    is_valid: Option<bool>,
    is_success: Option<bool>,
    is_subagent: bool,
) -> metric::ToolMetric<'a> {
    let origin = if is_custom_tool || mcp_server_name.is_some() {
        metric::ToolMetricOrigin::Mcp
    } else if tool_name.is_some() || aws_service_name.is_some() {
        metric::ToolMetricOrigin::Builtin
    } else {
        metric::ToolMetricOrigin::Unknown
    };
    let outcome = if !is_accepted {
        metric::ToolMetricOutcome::Denied
    } else if is_valid == Some(false) || is_success == Some(false) {
        metric::ToolMetricOutcome::Error
    } else if is_success == Some(true) {
        metric::ToolMetricOutcome::Success
    } else {
        metric::ToolMetricOutcome::Cancelled
    };
    metric::ToolMetric::new(
        engine,
        origin,
        outcome,
        if is_subagent {
            metric::ExecutionContext::Subagent
        } else {
            metric::ExecutionContext::Main
        },
    )
    .builtin_tool_name(tool_name)
}

fn canonical_token_metric_records(engine: metric::Engine, model: Option<&str>, usage: TokenUsage) -> Vec<MetricRecord> {
    [
        (metric::TokenType::InputUncached, usage.uncached_input_tokens),
        (metric::TokenType::InputCacheRead, usage.cache_read_input_tokens),
        (metric::TokenType::Output, usage.output_tokens),
    ]
    .into_iter()
    .filter_map(|(token_type, value)| metric::record_tokens_consumed(value, engine, model, token_type))
    .collect()
}

fn mcp_error_kind(reason: &str) -> metric::McpErrorKind {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("capabil") || reason.contains("tool") {
        metric::McpErrorKind::Protocol
    } else if reason.contains("timeout") || reason.contains("timed out") {
        metric::McpErrorKind::Timeout
    } else if reason.contains("auth") || reason.contains("unauthorized") || reason.contains("forbidden") {
        metric::McpErrorKind::Authentication
    } else if reason.contains("config") || reason.contains("environment") {
        metric::McpErrorKind::Configuration
    } else if reason.contains("spawn") || reason.contains("launch") || reason.contains("process") {
        metric::McpErrorKind::ProcessLaunch
    } else if reason.contains("connect") || reason.contains("network") || reason.contains("dns") {
        metric::McpErrorKind::Connection
    } else if reason.contains("protocol") || reason.contains("jsonrpc") || reason.contains("handshake") {
        metric::McpErrorKind::Protocol
    } else {
        metric::McpErrorKind::Unknown
    }
}

fn mcp_failure_stage(reason: &str) -> metric::McpFailureStage {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("config") || reason.contains("environment") {
        metric::McpFailureStage::Configuration
    } else if reason.contains("spawn") || reason.contains("launch") || reason.contains("process") {
        metric::McpFailureStage::ProcessLaunch
    } else if reason.contains("connect") || reason.contains("network") || reason.contains("dns") {
        metric::McpFailureStage::Connection
    } else if reason.contains("protocol") || reason.contains("jsonrpc") || reason.contains("handshake") {
        metric::McpFailureStage::Handshake
    } else if reason.contains("capabil") || reason.contains("tool") {
        metric::McpFailureStage::CapabilityDiscovery
    } else {
        metric::McpFailureStage::Unknown
    }
}

fn is_credit_unit(unit: &str, unit_plural: &str) -> bool {
    [unit, unit_plural]
        .into_iter()
        .any(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "credit" | "credits"))
}

fn token_usage_from_chat_added_message(data: &ChatAddedMessageParams) -> TokenUsage {
    TokenUsage::from_signed_counts(
        data.uncached_input_tokens.map(i64::from),
        data.cache_read_input_tokens.map(i64::from),
        data.cache_write_input_tokens.map(i64::from),
        data.output_tokens.map(i64::from),
    )
}

fn token_usage_from_turn_completion(args: &RecordUserTurnCompletionArgs) -> TokenUsage {
    TokenUsage::from_signed_counts(
        args.uncached_input_tokens,
        args.cache_read_input_tokens,
        args.cache_write_input_tokens,
        args.output_tokens,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn attribute<'a>(record: &'a MetricRecord, key: &str) -> Option<&'a str> {
        record
            .attributes
            .iter()
            .find(|attribute| attribute.key == key)
            .map(|attribute| attribute.value.as_str())
    }

    fn attributed_event(ty: EventType, engine: metric::Engine, session_interface: metric::SessionInterface) -> Event {
        let mut event = Event::new(ty);
        event.set_engine(engine);
        event.set_session_interface(session_interface);
        event
    }

    fn turn_event(
        engine: metric::Engine,
        session_interface: metric::SessionInterface,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Event {
        attributed_event(
            EventType::RecordUserTurnCompletion {
                conversation_id: "conversation".to_string(),
                result,
                args,
            },
            engine,
            session_interface,
        )
    }

    fn has_metric(records: &[MetricRecord], name: &str) -> bool {
        records.iter().any(|record| record.name == name)
    }

    #[test]
    fn v1_run_start_preserves_noninteractive_cli_attribution() {
        let mut event = Event::new(EventType::CliSessionStarted {
            os_type: metric::OsType::Linux,
            install_source: metric::InstallSource::Unknown,
        });
        event.app_type = Some("V1".to_string());
        event.set_engine(metric::Engine::V1);
        event.set_session_interface(metric::SessionInterface::NoninteractiveCli);

        let records = event_to_otel_metric_records(&event);
        let record = records
            .iter()
            .find(|record| record.name == "kiro_cli_run_started_total")
            .unwrap();

        assert_eq!(attribute(record, "session_interface"), Some("noninteractive_cli"));
        assert_eq!(attribute(record, "agent_engine"), Some("v1"));
    }

    #[test]
    fn v1_turn_uses_explicit_bounded_agent_mode() {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            args: RecordUserTurnCompletionArgs::default(),
        });
        event.app_type = Some("V1".to_string());
        event.set_engine(metric::Engine::V1);
        event.set_session_interface(metric::SessionInterface::InteractiveCli);
        event.metric_context.agent_mode = Some(metric::AgentMode::Spec);

        let records = event_to_otel_metric_records(&event);
        let record = records
            .iter()
            .find(|record| record.name == "kiro_cli_user_turns")
            .unwrap();

        assert_eq!(attribute(record, "session_interface"), Some("interactive_cli"));
        assert_eq!(attribute(record, "agent_mode"), Some("spec"));
        assert_eq!(attribute(record, "agent_engine"), Some("v1"));
    }

    #[test]
    fn external_acp_turn_carries_reported_client_name() {
        let mut event = turn_event(
            metric::Engine::V2,
            metric::SessionInterface::ExternalAcp,
            TelemetryResult::Succeeded,
            RecordUserTurnCompletionArgs::default(),
        );
        event.acp_client_name = Some("Sugarmaker".to_string());

        let records = event_to_otel_metric_records(&event);
        let record = records
            .iter()
            .find(|record| record.name == "kiro_cli_user_turns")
            .unwrap();

        assert_eq!(attribute(record, "session_interface"), Some("external_acp"));
        assert_eq!(attribute(record, "acp_client_name"), Some("Sugarmaker"));
        assert_eq!(attribute(record, "agent_engine"), Some("v2"));
    }

    #[test]
    fn explicit_engine_takes_precedence_over_legacy_app_type() {
        let mut event = Event::new(EventType::ModelInvocation { model: None });
        event.app_type = Some("V1".to_string());
        event.set_engine(metric::Engine::V2);

        let record = event_to_otel_metric_record(&event).unwrap();

        assert_eq!(attribute(&record, "agent_engine"), Some("v2"));
    }

    #[test]
    fn explicit_run_outcome_preserves_launcher_classification() {
        let mut event = Event::new(EventType::CliSessionCompleted {
            exit_reason: metric::ExitReason::Crash,
            agent_kind: metric::AgentKind::V2,
        });
        event.set_engine(metric::Engine::V2);
        event.metric_context.run_outcome = Some(metric::RunOutcome::Failure);

        let record = event_to_otel_metric_record(&event).unwrap();

        assert_eq!(attribute(&record, "run_outcome"), Some("failure"));
    }

    #[test]
    fn v2_interactive_frontend_owns_session_and_turn_metrics() {
        let session = attributed_event(
            EventType::ChatSessionStarted {
                mode: metric::Mode::Interactive,
            },
            metric::Engine::V2,
            metric::SessionInterface::InteractiveCli,
        );
        let turn = turn_event(
            metric::Engine::V2,
            metric::SessionInterface::InteractiveCli,
            TelemetryResult::Succeeded,
            RecordUserTurnCompletionArgs {
                user_turn_duration_seconds: 2,
                ..Default::default()
            },
        );

        assert!(event_to_otel_metric_records(&session).is_empty());
        assert!(event_to_otel_metric_records(&turn).is_empty());
    }

    #[test]
    fn v2_and_v3_oneshot_sessions_use_host_owned_session_and_turn_metrics() {
        for engine in [metric::Engine::V2, metric::Engine::V3] {
            let session = attributed_event(
                EventType::ChatSessionStarted {
                    mode: metric::Mode::Oneshot,
                },
                engine,
                metric::SessionInterface::NoninteractiveCli,
            );
            let turn = turn_event(
                engine,
                metric::SessionInterface::NoninteractiveCli,
                TelemetryResult::Succeeded,
                RecordUserTurnCompletionArgs {
                    user_turn_duration_seconds: 2,
                    uncached_input_tokens: Some(10),
                    output_tokens: Some(5),
                    model: Some("model".to_string()),
                    ..Default::default()
                },
            );

            let session_records = event_to_otel_metric_records(&session);
            assert!(has_metric(&session_records, "kiro_cli_chat_session_started_total"));

            let turn_records = event_to_otel_metric_records(&turn);
            assert!(has_metric(&turn_records, "kiro_cli_user_turns"));
            assert!(has_metric(&turn_records, "kiro_cli_user_turn_duration_seconds"));
            if engine == metric::Engine::V3 {
                assert!(has_metric(&turn_records, "kiro_cli_tokens_consumed"));
            }
        }
    }

    #[test]
    fn v3_noninteractive_turn_emits_model_invocation_count() {
        let event = turn_event(
            metric::Engine::V3,
            metric::SessionInterface::NoninteractiveCli,
            TelemetryResult::Succeeded,
            RecordUserTurnCompletionArgs {
                model: Some("model".to_string()),
                model_invocation_count: 3,
                ..Default::default()
            },
        );

        let records = event_to_otel_metric_records(&event);
        let record = records
            .iter()
            .find(|record| record.name == "kiro_cli_model_invocations_total")
            .unwrap();

        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(3));
        assert_eq!(attribute(record, "agent_engine"), Some("v3"));
        assert_eq!(attribute(record, "model"), Some("model"));
    }

    #[test]
    fn v3_interactive_turn_does_not_duplicate_model_invocations() {
        let event = turn_event(
            metric::Engine::V3,
            metric::SessionInterface::InteractiveCli,
            TelemetryResult::Succeeded,
            RecordUserTurnCompletionArgs {
                model: Some("model".to_string()),
                model_invocation_count: 3,
                ..Default::default()
            },
        );

        assert!(!has_metric(
            &event_to_otel_metric_records(&event),
            "kiro_cli_model_invocations_total"
        ));
    }

    #[test]
    fn v2_oneshot_response_uses_host_owned_model_and_token_metrics() {
        let event = attributed_event(
            EventType::ChatAddedMessage {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                data: ChatAddedMessageParams {
                    model: Some("model".to_string()),
                    uncached_input_tokens: Some(10),
                    output_tokens: Some(5),
                    ..Default::default()
                },
            },
            metric::Engine::V2,
            metric::SessionInterface::NoninteractiveCli,
        );

        let records = event_to_otel_metric_records(&event);
        assert!(has_metric(&records, "kiro_cli_model_invocations_total"));
        assert_eq!(
            records
                .iter()
                .filter(|record| record.name == "kiro_cli_tokens_consumed")
                .count(),
            2
        );
    }

    #[test]
    fn v3_and_v1_subagent_turns_use_host_owned_token_metrics() {
        for (engine, is_subagent) in [(metric::Engine::V3, false), (metric::Engine::V1, true)] {
            let args = RecordUserTurnCompletionArgs {
                model: Some("model".to_string()),
                uncached_input_tokens: Some(10),
                cache_read_input_tokens: Some(4),
                output_tokens: Some(5),
                is_subagent,
                ..Default::default()
            };
            let mut event = turn_event(
                engine,
                metric::SessionInterface::InteractiveCli,
                TelemetryResult::Succeeded,
                args,
            );
            event.is_subagent = is_subagent;

            let records = event_to_otel_metric_records(&event);
            assert_eq!(
                records
                    .iter()
                    .filter(|record| record.name == "kiro_cli_tokens_consumed")
                    .count(),
                3
            );
            assert!(!has_metric(&records, "kiro_cli_user_turns"));
        }
    }

    #[test]
    fn host_owned_cancelled_turn_emits_user_turn_and_cancellation() {
        let event = turn_event(
            metric::Engine::V2,
            metric::SessionInterface::NoninteractiveCli,
            TelemetryResult::Cancelled,
            RecordUserTurnCompletionArgs::default(),
        );

        let records = event_to_otel_metric_records(&event);
        assert!(has_metric(&records, "kiro_cli_user_turns"));
        assert!(has_metric(&records, "kiro_cli_turn_cancelled_total"));
        assert!(!has_metric(&records, "kiro_cli_turn_failure_total"));
    }

    #[test]
    fn v3_model_and_tool_events_are_suppressed() {
        let model = attributed_event(
            EventType::ModelInvocation {
                model: Some("model".to_string()),
            },
            metric::Engine::V3,
            metric::SessionInterface::InteractiveCli,
        );
        let tool = attributed_event(
            EventType::ToolUseSuggested {
                conversation_id: "conversation".to_string(),
                utterance_id: None,
                user_input_id: None,
                tool_use_id: Some("tool-use".to_string()),
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
                model: Some("model".to_string()),
                execution_duration: None,
                turn_duration: None,
                aws_service_name: None,
                aws_operation_name: None,
            },
            metric::Engine::V3,
            metric::SessionInterface::InteractiveCli,
        );

        assert!(event_to_otel_metric_records(&model).is_empty());
        assert!(event_to_otel_metric_records(&tool).is_empty());
    }

    #[test]
    fn legacy_chat_lifecycle_reaches_toolkit_without_creating_otel_metrics() {
        for (event, metric_name) in [
            (
                Event::new(EventType::ChatStart {
                    conversation_id: "conversation".to_string(),
                    model: Some("model".to_string()),
                }),
                "amazonq_startChat",
            ),
            (
                Event::new(EventType::ChatEnd {
                    conversation_id: "conversation".to_string(),
                    model: Some("model".to_string()),
                }),
                "amazonq_endChat",
            ),
        ] {
            assert!(event_to_otel_metric_records(&event).is_empty());
            assert_eq!(event_to_metric_datum(event).unwrap().metric_name(), metric_name);
        }
    }
}
