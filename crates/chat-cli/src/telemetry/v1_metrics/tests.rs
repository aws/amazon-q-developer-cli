use std::time::Duration;

use super::*;
use crate::telemetry::core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    ChatConversationType,
    MessageMetaTag,
    QProfileSwitchIntent,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    TelemetryResult,
};

fn attribute<'a>(record: &'a MetricRecord, key: &str) -> Option<&'a str> {
    record
        .attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .map(|attribute| attribute.value.as_str())
}

#[test]
fn shared_launcher_lifecycle_preserves_the_selected_engine() {
    for (client_application, expected_engine) in [
        (metric::ClientApplication::ChatCli, "v1"),
        (metric::ClientApplication::ChatCliV2, "v2"),
        (metric::ClientApplication::ChatCliV3, "v3"),
    ] {
        let mut event = Event::new(EventType::CliSessionStarted {
            os_type: metric::OsType::Linux,
            install_source: metric::InstallSource::Unknown,
        });
        event.set_client_application_kind(client_application);
        let records = records(&event);
        assert_eq!(attribute(&records[0], "engine"), Some(expected_engine));
        assert_eq!(
            attribute(&records[0], "client_application"),
            Some(client_application.as_str())
        );
    }

    for (agent_kind, expected_engine) in [
        (metric::AgentKind::V1, "v1"),
        (metric::AgentKind::V2, "v2"),
        (metric::AgentKind::Kas, "v3"),
    ] {
        let event = Event::new(EventType::CliSessionCompleted {
            exit_reason: metric::ExitReason::Clean,
            agent_kind,
        });
        assert_eq!(attribute(&records(&event)[0], "engine"), Some(expected_engine));
    }
}

#[test]
fn shared_invocation_events_preserve_the_effective_engine() {
    for (engine, client_application) in [
        (metric::Engine::V1, metric::ClientApplication::ChatCli),
        (metric::Engine::V2, metric::ClientApplication::ChatCliV2),
        (metric::Engine::V3, metric::ClientApplication::ChatCliV3),
    ] {
        let mut feature = Event::new(EventType::CliSubcommandExecuted {
            subcommand: "chat".to_string(),
        });
        feature.set_engine(engine);
        assert_eq!(attribute(&records(&feature)[0], "engine"), Some(engine.as_str()));

        let mut heartbeat = Event::new(EventType::DailyHeartbeat { install_method: None });
        heartbeat.set_engine(engine);
        heartbeat.set_client_application_kind(client_application);
        assert_eq!(attribute(&records(&heartbeat)[0], "engine"), Some(engine.as_str()));
        assert_eq!(
            attribute(&records(&heartbeat)[0], "client_application"),
            Some(client_application.as_str())
        );
    }
}

#[test]
fn v1_login_defaults_to_chat_cli_application() {
    let record = records(&Event::new(EventType::UserLoggedIn {})).remove(0);
    assert_eq!(attribute(&record, "client_application"), Some("chat_cli"));
    assert_eq!(attribute(&record, "engine"), Some("v1"));
}

#[test]
fn source_values_map_to_bounded_auth_dimensions() {
    let event = Event::new(EventType::AuthFailed {
        auth_method: "BuilderId".to_string(),
        oauth_flow: "DeviceCode".to_string(),
        error_type: "TokenRefresh".to_string(),
        error_code: Some("InvalidGrantException".to_string()),
    });

    let records = records(&event);
    let record = &records[0];
    assert_eq!(attribute(record, "auth_provider"), Some("builder_id"));
    assert_eq!(attribute(record, "auth_flow"), Some("device"));
    assert_eq!(attribute(record, "operation"), Some("refresh"));
    assert_eq!(attribute(record, "result"), Some("failed"));
}

#[test]
fn zero_request_timings_are_recorded() {
    let event = Event::new(EventType::ChatAddedMessage {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Succeeded,
        data: ChatAddedMessageParams {
            time_to_first_chunk_ms: Some(0.0),
            request_duration_seconds: Some(0.0),
            time_between_chunks_ms: Some(vec![0.0]),
            ..Default::default()
        },
    });

    let records = records(&event);
    for name in [
        "kiro_cli_time_to_first_chunk_ms",
        "kiro_cli.bedrock.stream.ttft",
        "kiro_cli.bedrock.stream.inter_token_latency",
        "kiro_cli.bedrock.request.duration",
    ] {
        assert!(
            records
                .iter()
                .any(|record| record.name == name && record.value == kiro_telemetry::MetricValue::Histogram(0.0)),
            "missing zero-valued timing for {name}",
        );
    }
}

#[test]
fn signed_user_contributions_preserve_removals() {
    let event = Event::new(EventType::AgentContribution {
        conversation_id: "conversation".to_string(),
        utterance_id: None,
        tool_use_id: None,
        tool_name: None,
        lines_by_agent: Some(5),
        lines_by_user: Some(-3),
    });

    let records = records(&event);
    let removed = records
        .iter()
        .find(|record| {
            record.name == "kiro_cli_agent_contribution_lines_total"
                && attribute(record, "contribution_source") == Some("user")
        })
        .unwrap();
    assert_eq!(removed.value, kiro_telemetry::MetricValue::Counter(3));
    assert_eq!(attribute(removed, "contribution_change"), Some("removed"));
}

#[test]
fn source_values_map_to_bounded_voice_dimensions() {
    let event = Event::new(EventType::VoiceInput {
        conversation_id: None,
        result: TelemetryResult::Succeeded,
        reason: None,
        reason_desc: None,
        backend: "LocalWhisper".to_string(),
        input_method: "SlashCommand".to_string(),
        recording_duration_ms: None,
        transcription_duration_ms: None,
        text_length: None,
        model_size: Some("base".to_string()),
        auto_submit: Some(true),
    });

    let records = records(&event);
    let record = &records[0];
    assert_eq!(attribute(record, "voice_backend"), Some("local_whisper"));
    assert_eq!(attribute(record, "voice_input_method"), Some("slash_command"));
}

#[test]
fn missing_voice_auto_submit_stays_absent() {
    let event = Event::new(EventType::VoiceInput {
        conversation_id: None,
        result: TelemetryResult::Succeeded,
        reason: None,
        reason_desc: None,
        backend: "LocalWhisper".to_string(),
        input_method: "SlashCommand".to_string(),
        recording_duration_ms: None,
        transcription_duration_ms: None,
        text_length: None,
        model_size: None,
        auto_submit: None,
    });

    let records = records(&event);
    assert_eq!(attribute(&records[0], "auto_submit"), None);
}

#[test]
fn chat_start_preserves_non_interactive_mode() {
    let mut event = Event::new(EventType::ChatStart {
        conversation_id: "conversation".to_string(),
        model: None,
    });
    event.metric_context.mode = Some(metric::Mode::Oneshot);
    event.metric_context.session_start_kind = Some(metric::SessionStartKind::New);

    let records = records(&event);
    assert_eq!(attribute(&records[0], "mode"), Some("oneshot"));
}

#[test]
fn tangent_forget_omits_missing_entries_removed_value() {
    let event = Event::new(EventType::TangentModeSession {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Succeeded,
        args: TangentModeSessionArgs {
            duration_seconds: 0,
            is_forget: true,
            entries_removed: None,
        },
    });

    assert!(
        records(&event)
            .iter()
            .all(|record| record.name != "kiro_cli_tangent_entries_removed")
    );
}

#[test]
fn cli_subcommand_preserves_command_and_subcommand() {
    let records = records(&Event::new(EventType::CliSubcommandExecuted {
        subcommand: "chat:list".to_string(),
    }));
    let record = &records[0];
    assert_eq!(attribute(record, "feature"), Some("chat"));
    assert_eq!(attribute(record, "subcommand"), Some("list"));
}

#[test]
fn forbidden_source_values_never_reappear_under_allowed_attribute_keys() {
    const SECRET: &str = "customer-private-v1-cardinality-value";
    let event_types = vec![
        EventType::ChatSlashCommandExecuted {
            conversation_id: SECRET.to_string(),
            command: "help".to_string(),
            subcommand: None,
            result: TelemetryResult::Failed,
            reason: Some(SECRET.to_string()),
        },
        EventType::ToolUseSuggested {
            conversation_id: SECRET.to_string(),
            utterance_id: Some(SECRET.to_string()),
            user_input_id: Some(SECRET.to_string()),
            tool_use_id: Some(SECRET.to_string()),
            tool_name: Some(SECRET.to_string()),
            mcp_server_name: Some("local-server".to_string()),
            is_accepted: true,
            is_trusted: false,
            is_success: Some(false),
            reason_desc: Some(SECRET.to_string()),
            is_valid: Some(true),
            is_custom_tool: false,
            input_token_size: None,
            output_token_size: None,
            custom_tool_call_latency: None,
            model: None,
            execution_duration: None,
            turn_duration: None,
            aws_service_name: None,
            aws_operation_name: None,
        },
        EventType::AgentContribution {
            conversation_id: SECRET.to_string(),
            utterance_id: Some(SECRET.to_string()),
            tool_use_id: Some(SECRET.to_string()),
            tool_name: Some(SECRET.to_string()),
            lines_by_agent: Some(1),
            lines_by_user: Some(1),
        },
        EventType::McpServerInit {
            conversation_id: SECRET.to_string(),
            server_name: SECRET.to_string(),
            init_failure_reason: Some(SECRET.to_string()),
            number_of_tools: 1,
            all_tool_names: Some(SECRET.to_string()),
            loaded_tool_names: Some(SECRET.to_string()),
            all_tools_count: 1,
        },
        EventType::MessageResponseError {
            result: TelemetryResult::Failed,
            reason: Some(SECRET.to_string()),
            reason_desc: Some(SECRET.to_string()),
            status_code: Some(500),
            conversation_id: SECRET.to_string(),
            request_id: Some(SECRET.to_string()),
            message_id: Some(SECRET.to_string()),
            context_file_length: None,
            model: None,
        },
        EventType::VoiceInput {
            conversation_id: Some(SECRET.to_string()),
            result: TelemetryResult::Failed,
            reason: Some(SECRET.to_string()),
            reason_desc: Some(SECRET.to_string()),
            backend: "LocalWhisper".to_string(),
            input_method: "SlashCommand".to_string(),
            recording_duration_ms: None,
            transcription_duration_ms: None,
            text_length: None,
            model_size: None,
            auto_submit: None,
        },
    ];

    for event_type in event_types {
        let mut event = Event::new(event_type);
        event.credential_start_url = Some(SECRET.to_string());
        for record in records(&event) {
            assert!(
                record.attributes.iter().all(|attribute| attribute.value != SECRET),
                "{} leaked a forbidden source value",
                record.name,
            );
        }
    }
}

#[test]
fn request_attempts_count_requests_without_server_ids() {
    let event = Event::new(EventType::RecordUserTurnCompletion {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Succeeded,
        args: RecordUserTurnCompletionArgs {
            request_ids: vec![Some("request-1".to_string()), None],
            emit_user_turn_counter: true,
            ..Default::default()
        },
    });

    let record = records(&event)
        .into_iter()
        .find(|record| record.name == "kiro_cli_user_turn_request_attempts")
        .unwrap();
    assert_eq!(record.value, kiro_telemetry::MetricValue::Histogram(2.0));
}

fn populated_metric_contract(
    name: &str,
) -> (
    kiro_telemetry::MetricValue,
    usize,
    &'static [(&'static str, &'static str)],
) {
    use kiro_telemetry::MetricValue::{
        Counter,
        Gauge,
        Histogram,
    };

    match name {
        "kiro_cli_time_to_first_chunk_ms" => (Histogram(125.0), 1, &[("model", "claude-sonnet-4")]),
        "kiro_cli.bedrock.stream.ttft" => (Histogram(0.125), 1, &[("model", "claude-sonnet-4")]),
        "kiro_cli.bedrock.stream.inter_token_latency" => (Histogram(0.01), 1, &[("model", "claude-sonnet-4")]),
        "kiro_cli.bedrock.request.duration" => (Histogram(1.5), 1, &[("operation", "stream")]),
        "kiro_cli_tokens_consumed" => (Counter(100), 4, &[("token_type", "input_uncached")]),
        "kiro_cli_cache_hit_ratio" => (Histogram(0.2), 1, &[("client_application", "chat_cli")]),
        "kiro_cli_chat_content_length" => (Histogram(4_096.0), 2, &[("content_role", "context")]),
        "kiro_cli_chat_message_tags_total" => (Counter(1), 1, &[("message_tag", "compact")]),
        "kiro_cli_user_turn_prompt_length" => (Histogram(200.0), 1, &[]),
        "kiro_cli_user_turn_response_length" => (Histogram(400.0), 1, &[]),
        "kiro_cli_user_turn_follow_up_count" => (Histogram(1.0), 1, &[]),
        "kiro_cli_user_turn_request_attempts" => (Histogram(2.0), 1, &[]),
        "kiro_cli_user_turn_duration_seconds" => (Histogram(2.0), 1, &[("mode", "interactive")]),
        "kiro_cli_user_turn_time_to_first_chunk_ms" => (Histogram(100.0), 1, &[]),
        "kiro_cli_turn_outcome_total" => (Counter(1), 1, &[("turn_outcome_reason", "timeout")]),
        "kiro_cli_tool_execution_duration_ms" => (Histogram(60.0), 1, &[("tool_origin", "builtin")]),
        "kiro_cli_tool_token_size" => (Histogram(30.0), 2, &[("content_role", "input")]),
        "kiro_cli_tool_duration" => (Histogram(50.0), 2, &[("duration_stage", "tool_call")]),
        "kiro_cli_request_error_context_length" => (Histogram(4_096.0), 1, &[("error_kind", "server_error")]),
        "kiro_cli_mcp_tool_count" => (Histogram(3.0), 2, &[("count_kind", "loaded")]),
        "kiro_cli_voice_duration" => (Histogram(1_000.0), 2, &[("duration_stage", "recording")]),
        "kiro_cli_voice_text_length" => (Histogram(80.0), 1, &[("voice_backend", "remote_server")]),
        "kiro_cli_tangent_duration_seconds" => (Histogram(2.0), 1, &[("result", "success")]),
        "kiro_cli_tangent_entries_removed" => (Histogram(4.0), 1, &[("result", "success")]),
        "kiro_cli_agent_contribution_lines_total" => (Counter(12), 2, &[
            ("contribution_source", "agent"),
            ("contribution_change", "added"),
        ]),
        "kiro_cli_agent_config_count" => (Histogram(2.0), 3, &[("count_kind", "agents_loaded")]),
        "kiro_cli_profile_count" => (Histogram(3.0), 1, &[("profile_source", "user")]),
        "kiro_cli_subagent_tool_uses_total" => (Counter(3), 2, &[("count_kind", "builtin")]),
        "kiro_cli.process.memory.rss" => (Gauge(1_024.0), 1, &[("agent_kind", "v1"), ("process_role", "host")]),
        "kiro_cli.process.memory.peak_rss" => (Gauge(2_048.0), 1, &[("agent_kind", "v1"), ("process_role", "host")]),
        "kiro_cli.process.cpu.utilization" => (Histogram(0.5), 1, &[("agent_kind", "v1"), ("process_role", "host")]),
        _ => (Counter(1), 1, &[("engine", "v1")]),
    }
}

#[test]
fn populated_events_emit_all_numeric_companions() {
    let cases = [
        (
            EventType::ChatAddedMessage {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                data: ChatAddedMessageParams {
                    context_file_length: Some(4_096),
                    model: Some("claude-sonnet-4".to_string()),
                    time_to_first_chunk_ms: Some(125.0),
                    request_duration_seconds: Some(1.5),
                    time_between_chunks_ms: Some(vec![10.0]),
                    chat_conversation_type: Some(ChatConversationType::ToolUse),
                    assistant_response_length: Some(512),
                    message_meta_tags: vec![MessageMetaTag::Compact],
                    uncached_input_tokens: Some(100),
                    output_tokens: Some(50),
                    cache_read_input_tokens: Some(25),
                    cache_write_input_tokens: Some(10),
                    ..Default::default()
                },
            },
            &[
                "kiro_cli_chat_messages_total",
                "kiro_cli_model_invocations_total",
                "kiro_cli_time_to_first_chunk_ms",
                "kiro_cli.bedrock.stream.ttft",
                "kiro_cli.bedrock.stream.inter_token_latency",
                "kiro_cli.bedrock.request.duration",
                "kiro_cli_tokens_consumed",
                "kiro_cli_cache_hit_ratio",
                "kiro_cli_chat_content_length",
                "kiro_cli_chat_message_tags_total",
            ][..],
        ),
        (
            EventType::RecordUserTurnCompletion {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Failed,
                args: RecordUserTurnCompletionArgs {
                    request_ids: vec![Some("request-1".to_string()), None],
                    model: Some("claude-sonnet-4".to_string()),
                    reason: Some("timeout".to_string()),
                    status_code: Some(504),
                    time_to_first_chunks_ms: vec![Some(100.0)],
                    user_prompt_length: 200,
                    assistant_response_length: 400,
                    uncached_input_tokens: Some(100),
                    output_tokens: Some(50),
                    cache_read_input_tokens: Some(25),
                    cache_write_input_tokens: Some(10),
                    user_turn_duration_seconds: 2,
                    follow_up_count: 1,
                    message_meta_tags: vec![MessageMetaTag::TangentMode],
                    emit_user_turn_counter: true,
                    ..Default::default()
                },
            },
            &[
                "kiro_cli_user_turns",
                "kiro_cli_user_turn_prompt_length",
                "kiro_cli_user_turn_response_length",
                "kiro_cli_user_turn_follow_up_count",
                "kiro_cli_user_turn_request_attempts",
                "kiro_cli_user_turn_duration_seconds",
                "kiro_cli_user_turn_time_to_first_chunk_ms",
                "kiro_cli_turn_outcome_total",
            ][..],
        ),
        (
            EventType::ToolUseSuggested {
                conversation_id: "conversation".to_string(),
                utterance_id: Some("utterance".to_string()),
                user_input_id: Some("input".to_string()),
                tool_use_id: Some("tool-use".to_string()),
                tool_name: Some("fs_read".to_string()),
                mcp_server_name: None,
                is_accepted: true,
                is_trusted: true,
                is_success: Some(true),
                reason_desc: None,
                is_valid: Some(true),
                is_custom_tool: false,
                input_token_size: Some(30),
                output_token_size: Some(40),
                custom_tool_call_latency: Some(50),
                model: Some("claude-sonnet-4".to_string()),
                execution_duration: Some(Duration::from_millis(60)),
                turn_duration: Some(Duration::from_millis(70)),
                aws_service_name: None,
                aws_operation_name: None,
            },
            &[
                "kiro_cli_tool_call_total",
                "kiro_cli_tool_invocations",
                "kiro_cli_tool_execution_duration_ms",
                "kiro_cli_tool_token_size",
                "kiro_cli_tool_duration",
            ][..],
        ),
        (
            EventType::MessageResponseError {
                result: TelemetryResult::Failed,
                reason: Some("server error".to_string()),
                reason_desc: None,
                status_code: Some(500),
                conversation_id: "conversation".to_string(),
                request_id: Some("request".to_string()),
                message_id: Some("message".to_string()),
                context_file_length: Some(4_096),
                model: Some("claude-sonnet-4".to_string()),
            },
            &[
                "kiro_cli.bedrock.request.errors",
                "kiro_cli_request_error_context_length",
            ][..],
        ),
        (
            EventType::McpServerInit {
                conversation_id: "conversation".to_string(),
                server_name: "awslabs.tools".to_string(),
                init_failure_reason: None,
                number_of_tools: 3,
                all_tool_names: None,
                loaded_tool_names: None,
                all_tools_count: 5,
            },
            &[
                "kiro_cli_mcp_server_init_total",
                "kiro_cli_mcp_tool_count",
                "kiro_cli_mcp_server_connected_total",
            ][..],
        ),
        (
            EventType::VoiceInput {
                conversation_id: Some("conversation".to_string()),
                result: TelemetryResult::Succeeded,
                reason: None,
                reason_desc: None,
                backend: "RemoteServer".to_string(),
                input_method: "ContinuousVoice".to_string(),
                recording_duration_ms: Some(1_000),
                transcription_duration_ms: Some(200),
                text_length: Some(80),
                model_size: Some("small".to_string()),
                auto_submit: Some(true),
            },
            &[
                "kiro_cli_voice_input_total",
                "kiro_cli_voice_duration",
                "kiro_cli_voice_text_length",
            ][..],
        ),
        (
            EventType::TangentModeSession {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                args: TangentModeSessionArgs {
                    duration_seconds: 2,
                    is_forget: false,
                    entries_removed: None,
                },
            },
            &[
                "kiro_cli_slash_command_invoked_total",
                "kiro_cli_tangent_duration_seconds",
            ][..],
        ),
        (
            EventType::TangentModeSession {
                conversation_id: "conversation".to_string(),
                result: TelemetryResult::Succeeded,
                args: TangentModeSessionArgs {
                    duration_seconds: 0,
                    is_forget: true,
                    entries_removed: Some(4),
                },
            },
            &[
                "kiro_cli_slash_command_invoked_total",
                "kiro_cli_tangent_entries_removed",
            ][..],
        ),
        (
            EventType::AgentContribution {
                conversation_id: "conversation".to_string(),
                utterance_id: Some("utterance".to_string()),
                tool_use_id: Some("tool-use".to_string()),
                tool_name: Some("fs_write".to_string()),
                lines_by_agent: Some(12),
                lines_by_user: Some(3),
            },
            &[
                "kiro_cli_agent_contribution_total",
                "kiro_cli_agent_contribution_lines_total",
            ][..],
        ),
        (
            EventType::AgentConfigInit {
                conversation_id: "conversation".to_string(),
                args: AgentConfigInitArgs {
                    agents_loaded_count: 2,
                    agents_loaded_failed_count: 1,
                    legacy_profile_migration_executed: true,
                    legacy_profile_migrated_count: 1,
                    launched_agent: "kiro_default".to_string(),
                },
            },
            &["kiro_cli_agent_config_init_total", "kiro_cli_agent_config_count"][..],
        ),
        (
            EventType::DidSelectProfile {
                source: QProfileSwitchIntent::User,
                amazonq_profile_region: "us-east-1".to_string(),
                result: TelemetryResult::Succeeded,
                sso_region: Some("us-west-2".to_string()),
                profile_count: Some(3),
            },
            &["kiro_cli_profile_selection_total", "kiro_cli_profile_count"][..],
        ),
        (
            EventType::SubagentInvocation {
                parent_conversation_id: "conversation".to_string(),
                subagent_name: "explore".to_string(),
                builtin_tool_uses: 3,
                mcp_tool_uses: 2,
                parent_tool_use_id: "tool-use".to_string(),
            },
            &[
                "kiro_cli_subagent_delegations_total",
                "kiro_cli_subagent_tool_uses_total",
            ][..],
        ),
        (
            EventType::ProcessHealth {
                rss_bytes: 1_024.0,
                peak_rss_bytes: 2_048.0,
                cpu_utilization: 0.5,
            },
            &[
                "kiro_cli.process.memory.rss",
                "kiro_cli.process.memory.peak_rss",
                "kiro_cli.process.cpu.utilization",
            ][..],
        ),
    ];

    for (event_type, expected_names) in cases {
        let records = records(&Event::new(event_type));
        for expected_name in expected_names {
            let (expected_value, expected_count, expected_attributes) = populated_metric_contract(expected_name);
            assert_eq!(
                records.iter().filter(|record| record.name == *expected_name).count(),
                expected_count,
                "unexpected record count for {expected_name}",
            );
            assert!(
                records.iter().any(|record| {
                    record.name == *expected_name
                        && record.value == expected_value
                        && expected_attributes
                            .iter()
                            .all(|(key, value)| attribute(record, key) == Some(*value))
                }),
                "wrong value or bounded attributes for {expected_name}: {records:?}",
            );
        }
    }
}

#[test]
fn tool_durations_preserve_units_without_raw_server_names() {
    let records = records(&Event::new(EventType::ToolUseSuggested {
        conversation_id: "conversation".to_string(),
        utterance_id: None,
        user_input_id: None,
        tool_use_id: None,
        tool_name: Some("custom_tool".to_string()),
        mcp_server_name: Some("local-server".to_string()),
        is_accepted: true,
        is_trusted: true,
        is_success: Some(true),
        reason_desc: None,
        is_valid: Some(true),
        is_custom_tool: true,
        input_token_size: None,
        output_token_size: None,
        custom_tool_call_latency: Some(2),
        model: None,
        execution_duration: Some(Duration::from_millis(250)),
        turn_duration: Some(Duration::from_millis(750)),
        aws_service_name: None,
        aws_operation_name: None,
    }));

    let execution = records
        .iter()
        .find(|record| record.name == "kiro_cli_tool_execution_duration_ms")
        .unwrap();
    assert_eq!(execution.value, kiro_telemetry::MetricValue::Histogram(250.0));
    for name in [
        "kiro_cli_tool_call_total",
        "kiro_cli_tool_invocations",
        "kiro_cli_tool_execution_duration_ms",
    ] {
        let record = records.iter().find(|record| record.name == name).unwrap();
        assert_eq!(attribute(record, "mcp_server_name"), None);
        assert!(
            record
                .attributes
                .iter()
                .all(|attribute| attribute.value != "local-server")
        );
    }

    let durations = records
        .iter()
        .filter(|record| record.name == "kiro_cli_tool_duration")
        .collect::<Vec<_>>();
    assert_eq!(durations.len(), 2);
    assert!(durations.iter().any(|record| {
        record.value == kiro_telemetry::MetricValue::Histogram(2.0)
            && attribute(record, "duration_stage") == Some("tool_call")
    }));
    assert!(durations.iter().any(|record| {
        record.value == kiro_telemetry::MetricValue::Histogram(0.75)
            && attribute(record, "duration_stage") == Some("tool_turn")
    }));
}

#[test]
fn tool_call_preserves_bounded_toolkit_dimensions() {
    let records = records(&Event::new(EventType::ToolUseSuggested {
        conversation_id: "conversation".to_string(),
        utterance_id: None,
        user_input_id: None,
        tool_use_id: None,
        tool_name: Some("fs_read".to_string()),
        mcp_server_name: None,
        is_accepted: true,
        is_trusted: false,
        is_success: Some(false),
        reason_desc: None,
        is_valid: Some(true),
        is_custom_tool: false,
        input_token_size: None,
        output_token_size: None,
        custom_tool_call_latency: None,
        model: Some("claude-sonnet-4".to_string()),
        execution_duration: None,
        turn_duration: None,
        aws_service_name: None,
        aws_operation_name: None,
    }));
    let record = records
        .iter()
        .find(|record| record.name == "kiro_cli_tool_call_total")
        .unwrap();

    for (key, value) in [
        ("model", "claude-sonnet-4"),
        ("is_accepted", "true"),
        ("is_trusted", "false"),
        ("is_valid", "true"),
        ("is_success", "false"),
    ] {
        assert_eq!(attribute(record, key), Some(value));
    }
}

#[test]
fn cancellation_is_a_turn_outcome_not_a_request_error() {
    let event = Event::new(EventType::RecordUserTurnCompletion {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Cancelled,
        args: RecordUserTurnCompletionArgs {
            emit_user_turn_counter: true,
            ..Default::default()
        },
    });
    let records = records(&event);

    let outcome = records
        .iter()
        .find(|record| record.name == "kiro_cli_turn_outcome_total")
        .unwrap();
    assert_eq!(attribute(outcome, "turn_outcome_reason"), Some("interrupted"));
    assert!(
        records
            .iter()
            .all(|record| record.name != "kiro_cli.bedrock.request.errors")
    );
}

#[test]
fn normal_turn_completion_does_not_reemit_request_tokens() {
    let event = Event::new(EventType::RecordUserTurnCompletion {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Failed,
        args: RecordUserTurnCompletionArgs {
            model: Some("claude-sonnet-4".to_string()),
            reason: Some("server error".to_string()),
            status_code: Some(500),
            total_tokens: Some(175),
            uncached_input_tokens: Some(100),
            output_tokens: Some(50),
            cache_read_input_tokens: Some(25),
            cache_write_input_tokens: Some(10),
            emit_user_turn_counter: true,
            ..Default::default()
        },
    });

    let records = records(&event);
    assert!(records.iter().all(|record| record.name != "kiro_cli_tokens_consumed"));
    for request_metric in [
        "kiro_cli_cache_hit_ratio",
        "kiro_cli.bedrock.request.errors",
        "kiro_cli.bedrock.request.duration",
    ] {
        assert!(
            records.iter().all(|record| record.name != request_metric),
            "{request_metric} must be emitted by its request-level event"
        );
    }
}

#[test]
fn metadata_free_subagent_failure_omits_unknown_numeric_details() {
    let event = Event::new(EventType::RecordUserTurnCompletion {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Failed,
        args: RecordUserTurnCompletionArgs {
            reason: Some("service failure".to_string()),
            is_subagent: true,
            emit_user_turn_counter: true,
            emit_turn_numeric_metrics: Some(false),
            ..Default::default()
        },
    });

    let records = records(&event);
    assert!(records.iter().any(|record| record.name == "kiro_cli_user_turns"));
    assert!(
        records
            .iter()
            .any(|record| record.name == "kiro_cli_turn_outcome_total")
    );
    for name in [
        "kiro_cli_user_turn_duration_seconds",
        "kiro_cli_user_turn_prompt_length",
        "kiro_cli_user_turn_response_length",
        "kiro_cli_user_turn_follow_up_count",
        "kiro_cli_user_turn_request_attempts",
        "kiro_cli_user_turn_time_to_first_chunk_ms",
    ] {
        assert!(
            records.iter().all(|record| record.name != name),
            "metadata-free failure emitted {name}",
        );
    }
}

#[test]
fn subagent_completion_owns_tokens_without_request_events() {
    let event = Event::new(EventType::RecordUserTurnCompletion {
        conversation_id: "conversation".to_string(),
        result: TelemetryResult::Succeeded,
        args: RecordUserTurnCompletionArgs {
            model: Some("claude-sonnet-4".to_string()),
            uncached_input_tokens: Some(100),
            output_tokens: Some(50),
            cache_read_input_tokens: Some(25),
            cache_write_input_tokens: Some(10),
            is_subagent: true,
            emit_user_turn_counter: true,
            ..Default::default()
        },
    });

    assert_eq!(
        records(&event)
            .iter()
            .filter(|record| record.name == "kiro_cli_tokens_consumed")
            .count(),
        4
    );
}

#[test]
fn slash_command_preserves_subcommand_and_result() {
    let event = Event::new(EventType::ChatSlashCommandExecuted {
        conversation_id: "conversation".to_string(),
        command: "model".to_string(),
        subcommand: Some("list".to_string()),
        result: TelemetryResult::Failed,
        reason: Some("failure".to_string()),
    });

    let record = records(&event).remove(0);
    assert_eq!(attribute(&record, "command"), Some("model"));
    assert_eq!(attribute(&record, "subcommand"), Some("list"));
    assert_eq!(attribute(&record, "result"), Some("failed"));
}
