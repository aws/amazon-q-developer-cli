use std::sync::Arc;
use std::time::Duration;

use agent::agent_loop::protocol::{
    AgentLoopEvent,
    AgentLoopEventKind,
    LoopEndReason,
    LoopError,
    StreamMetadata,
    StreamResult,
    UserTurnMetadata,
};
use agent::agent_loop::types::{
    ContentBlock,
    Message,
    MetadataEvent,
    MetadataMetrics,
    MetadataService,
    MetadataUsage,
    MeteringUsageInfo,
    RefusalInfo,
    RetryWarningEvent,
    Role,
    StreamError,
    StreamErrorKind,
    StreamEvent,
};
use agent::protocol::{
    AgentEvent,
    InitializeUpdateEvent,
    InternalEvent,
    ToolCallFailureReason,
    ToolCallResult,
    UpdateEvent,
};
use agent::tools::{
    BuiltInTool,
    ToolCallIdentity,
    ToolKind,
};
use agent::types::AgentId;
use kiro_telemetry::metric;
use kiro_telemetry_host::{
    Event,
    EventType,
    MessageMetaTag,
    TelemetryResult,
};
use kiro_telemetry_legacy::event_to_otel_metric_records;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::context::{
    AcpClientInfo,
    AppType,
    ClientName,
    KIRO_ACP_CLIENT_NAME,
    ModelProvider,
    TelemetryContext,
};
use crate::observer::*;

fn test_loop_id() -> agent::agent_loop::AgentLoopId {
    agent::agent_loop::AgentLoopId::new(AgentId::default())
}

fn model_provider(model: &'static str) -> ModelProvider {
    Arc::new(move || Some(model.to_string()))
}

fn make_observer_with_subagent(is_subagent: bool) -> (TelemetryObserver, mpsc::UnboundedReceiver<Event>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let client_info = Some(AcpClientInfo::new(KIRO_ACP_CLIENT_NAME.into(), "1.0.0".into()));
    let ctx = TelemetryContext::new(model_provider("claude-4-sonnet"), client_info, is_subagent);
    (TelemetryObserver::new_for_test(tx, ctx), rx)
}

fn make_observer() -> (TelemetryObserver, mpsc::UnboundedReceiver<Event>) {
    make_observer_with_subagent(false)
}

fn make_external_observer() -> (TelemetryObserver, mpsc::UnboundedReceiver<Event>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let client_info = Some(AcpClientInfo::new("external-test-client".into(), "1.0.0".into()));
    let ctx = TelemetryContext::new(model_provider("claude-4-sonnet"), client_info, false);
    (TelemetryObserver::new_for_test(tx, ctx), rx)
}

fn make_loop_event(kind: AgentLoopEventKind) -> AgentEvent {
    AgentEvent::Internal(InternalEvent::AgentLoop(Box::new(AgentLoopEvent {
        id: test_loop_id(),
        kind,
    })))
}

fn success_stream_end() -> AgentLoopEventKind {
    let request_start_time = chrono::Utc::now();
    AgentLoopEventKind::ResponseStreamEnd {
        result: Ok(Message::new(
            Uuid::new_v4().to_string(),
            Role::Assistant,
            vec![ContentBlock::Text("hello".into())],
            None,
        )),
        metadata: StreamMetadata {
            tool_uses: vec![],
            stream: Some(MetadataEvent {
                metrics: Some(MetadataMetrics {
                    request_start_time,
                    request_end_time: request_start_time + chrono::Duration::milliseconds(250),
                    time_to_first_chunk: Some(Duration::from_millis(100)),
                    time_between_chunks: Some(vec![Duration::from_millis(10)]),
                    response_stream_len: 42,
                }),
                usage: None,
                service: Some(MetadataService {
                    request_id: Some("req-1".into()),
                    status_code: Some(200),
                }),
                metering_usage: Vec::new(),
                stop_reason: None,
                refusal: None,
            }),
            request_attempts: None,
        },
    }
}

fn error_stream_end(kind: StreamErrorKind) -> AgentLoopEventKind {
    AgentLoopEventKind::ResponseStreamEnd {
        result: Err(LoopError::Stream(StreamError::new(kind))),
        metadata: StreamMetadata {
            tool_uses: vec![],
            stream: None,
            request_attempts: None,
        },
    }
}

fn error_stream_end_with_attempts(kind: StreamErrorKind, attempts: u32) -> AgentLoopEventKind {
    AgentLoopEventKind::ResponseStreamEnd {
        result: Err(LoopError::Stream(StreamError::new(kind))),
        metadata: StreamMetadata {
            tool_uses: vec![],
            stream: None,
            request_attempts: Some(attempts),
        },
    }
}

#[test]
fn test_successful_request_emits_add_chat_message() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ChatAddedMessage { result, data, .. } => {
            assert_eq!(*result, TelemetryResult::Succeeded);
            assert_eq!(data.request_id.as_deref(), Some("req-1"));
            assert_eq!(data.request_duration_seconds, Some(0.25));
            assert!(data.reason.is_none());
            assert_eq!(event.app_type.as_deref(), Some("V2"));
            assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
        },
        other => panic!("expected ChatAddedMessage, got {other:?}"),
    }
    assert!(rx.try_recv().is_err());
}

#[test]
fn typed_refusal_emits_refusal_model_request_failure() {
    let (mut obs, mut rx) = make_external_observer();
    let mut response = success_stream_end();
    let AgentLoopEventKind::ResponseStreamEnd { metadata, .. } = &mut response else {
        unreachable!();
    };
    let stream = metadata.stream.as_mut().expect("stream metadata");
    stream.stop_reason = Some("CONTENT_FILTERED".to_string());
    stream.refusal = Some(Box::new(RefusalInfo {
        category: Some("policy".to_string()),
        explanation: None,
        recommended_model: None,
    }));

    obs.handle_event("test-session", &make_loop_event(response));

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage {
        result: TelemetryResult::Failed,
        ..
    }));
    let failure = rx.try_recv().unwrap();
    assert!(matches!(failure.ty, EventType::MessageResponseError { .. }));
    let records = event_to_otel_metric_records(&failure);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "kiro_cli_model_request_failure_total");
    assert!(
        records[0]
            .attributes
            .iter()
            .any(|attribute| attribute.key == "error_kind" && attribute.value == "refusal")
    );

    obs.handle_event(
        "test-session",
        &AgentEvent::EndTurn(UserTurnMetadata {
            loop_id: test_loop_id(),
            result: None,
            message_ids: vec![],
            total_request_count: 1,
            number_of_cycles: 0,
            builtin_tool_uses: 0,
            turn_duration: Some(Duration::from_secs(1)),
            end_reason: LoopEndReason::UserTurnEnd,
            end_timestamp: chrono::Utc::now(),
            input_token_count: 0,
            output_token_count: 0,
            cache_read_input_token_count: 0,
            cache_write_input_token_count: 0,
            model: None,
            assistant_response_length: 0,
            request_attempts: None,
            context_usage_percentage: None,
            final_context_usage_percentage: None,
            metering_usage: Vec::new(),
            user_prompt_length: 0,
        }),
    );

    let turn = rx.try_recv().unwrap();
    assert!(matches!(
        &turn.ty,
        EventType::RecordUserTurnCompletion {
            result: TelemetryResult::Failed,
            args,
            ..
        } if args.reason.as_deref() == Some("ModelRefusal")
    ));
    let records = event_to_otel_metric_records(&turn);
    assert!(records.iter().any(|record| {
        record.name == "kiro_cli_turn_failure_total"
            && record
                .attributes
                .iter()
                .any(|attribute| attribute.key == "turn_failure_reason" && attribute.value == "model_error")
    }));
}

#[test]
fn mcp_initialize_update_emits_mcp_server_init_event() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &AgentEvent::InitializeUpdate(InitializeUpdateEvent::Mcp(agent::mcp::McpServerEvent::Initialized {
            server_name: "code".to_string(),
            source: agent::agent_config::McpServerConfigSource::Registry,
            serve_duration: Duration::from_millis(25),
            list_tools_duration: Some(Duration::from_millis(10)),
            list_prompts_duration: None,
            tool_token_count_estimate: 128,
        })),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::McpServerInit {
            conversation_id,
            server_name,
            mcp_server_source,
            init_failure_reason,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            all_tools_count,
            mcp_tools_token_count_estimate,
        } => {
            assert_eq!(conversation_id, "test-session");
            assert_eq!(server_name, "code");
            assert_eq!(*mcp_server_source, metric::McpServerSource::Registry);
            assert!(init_failure_reason.is_none());
            assert_eq!(*number_of_tools, 0);
            assert!(all_tool_names.is_none());
            assert!(loaded_tool_names.is_none());
            assert_eq!(*all_tools_count, 0);
            assert_eq!(*mcp_tools_token_count_estimate, Some(128));
        },
        other => panic!("expected McpServerInit, got {other:?}"),
    }

    let records = event_to_otel_metric_records(&event);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].name, "kiro_cli_mcp_server_init_total");
    assert_eq!(records[1].name, "kiro_cli_mcp_tools_token_count_estimate");
    assert_eq!(records[1].value, kiro_telemetry::MetricValue::Histogram(128.0));
}

#[test]
fn mcp_runtime_error_emits_failed_mcp_server_init_event() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &AgentEvent::Mcp(agent::mcp::McpServerEvent::InitializeError {
            server_name: "local-server".to_string(),
            source: agent::agent_config::McpServerConfigSource::AcpInjected,
            error: "request timed out while listing tools".to_string(),
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::McpServerInit {
            conversation_id,
            server_name,
            mcp_server_source,
            init_failure_reason,
            ..
        } => {
            assert_eq!(conversation_id, "test-session");
            assert_eq!(server_name, "local-server");
            assert_eq!(*mcp_server_source, metric::McpServerSource::AcpInjected);
            assert_eq!(
                init_failure_reason.as_deref(),
                Some("request timed out while listing tools")
            );
        },
        other => panic!("expected McpServerInit, got {other:?}"),
    }

    let records = event_to_otel_metric_records(&event);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "kiro_cli_mcp_server_init_total");
    assert!(
        records[0]
            .attributes
            .iter()
            .all(|attribute| attribute.key != "mcp_error_kind" && attribute.key != "mcp_failure_stage")
    );
}

#[test]
fn mcp_status_refresh_is_not_an_initialization_attempt() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &AgentEvent::Mcp(agent::mcp::McpServerEvent::StatusRefresh {
            server_name: "code".to_string(),
            source: agent::agent_config::McpServerConfigSource::Registry,
        }),
    );

    assert!(rx.try_recv().is_err());
}

#[test]
fn retry_warning_does_not_emit_per_attempt_metric() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::Stream(StreamResult::Ok(StreamEvent::RetryWarning(
            RetryWarningEvent {
                attempt: 3,
                max_attempts: 4,
                delay_secs: 1.0,
                message: "Retrying in 1s (attempt 3/4)".to_string(),
            },
        )))),
    );

    assert!(rx.try_recv().is_err());
}

#[test]
fn successful_retried_request_emits_recovered_operation() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
            result: Ok(Message::new(
                Uuid::new_v4().to_string(),
                Role::Assistant,
                vec![ContentBlock::Text("ok".into())],
                None,
            )),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: None,
                request_attempts: Some(3),
            },
        }),
    );

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
    let event = rx.try_recv().unwrap();
    assert!(matches!(&event.ty, EventType::AutomaticRetryCompleted {
        retry_reason: metric::RetryReason::Other,
        additional_attempts: 2,
        outcome: metric::RetryOutcome::Recovered,
    }));
    assert!(
        event_to_otel_metric_records(&event).is_empty(),
        "retry lifecycle remains available to legacy/local diagnostics but is not a KUTS metric"
    );
}

#[test]
fn test_compaction_turn_carries_compact_message_meta_tag() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event(
        "test-session",
        &AgentEvent::Compaction(agent::protocol::CompactionEvent::Started),
    );
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ChatAddedMessage { data, .. } => {
            assert_eq!(data.message_meta_tags, vec![MessageMetaTag::Compact]);
        },
        other => panic!("expected ChatAddedMessage, got {other:?}"),
    }

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: Some(Duration::from_secs(2)),
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { args, .. } => {
            assert_eq!(args.message_meta_tags, vec![MessageMetaTag::Compact]);
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_metering_stream_emits_metering_event() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
            result: Ok(Message::new(
                Uuid::new_v4().to_string(),
                Role::Assistant,
                vec![ContentBlock::Text("hello".into())],
                None,
            )),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: Some(MetadataEvent {
                    metrics: None,
                    usage: None,
                    service: Some(MetadataService {
                        request_id: Some("req-1".into()),
                        status_code: Some(200),
                    }),
                    metering_usage: vec![MeteringUsageInfo {
                        value: 2.0,
                        unit: "credit".into(),
                        unit_plural: "credits".into(),
                    }],
                    stop_reason: None,
                    refusal: None,
                }),
                request_attempts: None,
            },
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::MeteringEvent {
            request_id,
            usage,
            unit,
            unit_plural,
            ..
        } => {
            assert_eq!(request_id.as_deref(), Some("req-1"));
            assert_eq!(*usage, 2.0);
            assert_eq!(unit, "credit");
            assert_eq!(unit_plural, "credits");
        },
        other => panic!("expected MeteringEvent, got {other:?}"),
    }

    let event = rx.try_recv().unwrap();
    assert!(matches!(event.ty, EventType::ChatAddedMessage { .. }));
}

#[test]
fn test_failed_request_emits_error_events() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::Throttling)),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ChatAddedMessage { result, data, .. } => {
            assert_eq!(*result, TelemetryResult::Failed);
            assert_eq!(data.reason.as_deref(), Some(REASON_QUOTA_BREACH));
        },
        other => panic!("expected ChatAddedMessage, got {other:?}"),
    }

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::MessageResponseError { reason, .. } => {
            assert_eq!(reason.as_deref(), Some(REASON_QUOTA_BREACH));
        },
        other => panic!("expected MessageResponseError, got {other:?}"),
    }
}

#[test]
fn failed_retried_request_emits_exhausted_operation() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::Throttling, 3)),
    );

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::MessageResponseError { .. }
    ));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::AutomaticRetryCompleted {
            retry_reason,
            additional_attempts,
            outcome,
        } => {
            assert_eq!(*retry_reason, metric::RetryReason::Throttled);
            assert_eq!(*additional_attempts, 2);
            assert_eq!(*outcome, metric::RetryOutcome::Exhausted);
        },
        other => panic!("expected AutomaticRetryCompleted, got {other:?}"),
    }
}

#[test]
fn cancelled_retried_request_emits_cancelled_operation() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::Interrupted, 3)),
    );

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::MessageResponseError { .. }
    ));
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::AutomaticRetryCompleted {
            retry_reason: metric::RetryReason::Other,
            additional_attempts: 2,
            outcome: metric::RetryOutcome::Cancelled,
            ..
        }
    ));
}

#[test]
fn ambiguous_validation_and_model_errors_use_unknown_retry_reason() {
    for kind in [
        StreamErrorKind::Validation {
            message: Some("invalid request".to_string()),
        },
        StreamErrorKind::ModelOverloaded {
            message: "overloaded".to_string(),
        },
    ] {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event(
            "test-session",
            &make_loop_event(error_stream_end_with_attempts(kind, 2)),
        );

        let _ = rx.try_recv().unwrap();
        let _ = rx.try_recv().unwrap();
        assert!(matches!(
            rx.try_recv().unwrap().ty,
            EventType::AutomaticRetryCompleted {
                retry_reason: metric::RetryReason::Other,
                ..
            }
        ));
    }
}

#[test]
fn context_recovery_uses_typed_attempt_lifecycle() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &AgentEvent::Compaction(agent::protocol::CompactionEvent::ContextRecoveryAttempt { final_attempt: false }),
    );
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::ContextWindowOverflow)),
    );

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::MessageResponseError { .. }
    ));
    assert!(rx.try_recv().is_err());

    obs.handle_event(
        "test-session",
        &AgentEvent::Compaction(agent::protocol::CompactionEvent::ContextRecoveryAttempt { final_attempt: true }),
    );
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::AutomaticRetryCompleted {
            retry_reason: metric::RetryReason::ContextRecovery,
            additional_attempts: 2,
            outcome: metric::RetryOutcome::Recovered,
        }
    ));
}

#[test]
fn first_attempt_failure_does_not_emit_retry_operation() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::ServiceFailure, 1)),
    );

    assert!(matches!(rx.try_recv().unwrap().ty, EventType::ChatAddedMessage { .. }));
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::MessageResponseError { .. }
    ));
    assert!(rx.try_recv().is_err());
}

#[test]
fn test_context_overflow_reason_mapping() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::ContextWindowOverflow)),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ChatAddedMessage { data, .. } => {
            assert_eq!(data.reason.as_deref(), Some(REASON_CONTEXT_WINDOW_OVERFLOW));
        },
        other => panic!("expected ChatAddedMessage, got {other:?}"),
    }
}

#[test]
fn test_end_turn_emits_record_user_turn_completion() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event("test-session", &make_loop_event(success_stream_end()));
    let _ = rx.try_recv();

    obs.handle_event("test-session", &make_loop_event(success_stream_end()));
    let _ = rx.try_recv();

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 2,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: Some(Duration::from_secs(5)),
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 11,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { result, args, .. } => {
            assert_eq!(*result, TelemetryResult::Succeeded);
            assert_eq!(args.request_ids.len(), 2);
            assert_eq!(args.user_turn_duration_seconds, 5);
            assert_eq!(args.user_prompt_length, 11);
            assert!(args.reason.is_none());
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_turn_completion_keeps_subsecond_duration() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event("test-session", &make_loop_event(success_stream_end()));
    let _ = rx.try_recv();

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: Some(Duration::from_millis(250)),
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { args, .. } => {
            assert_eq!(args.user_turn_duration_seconds, 1);
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_end_turn_emits_context_usage_percentage() {
    let (mut obs, mut rx) = make_observer_with_subagent(true);

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: Some(Duration::from_secs(1)),
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: Some(42.5),
        final_context_usage_percentage: Some(42.5),
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    assert!(matches!(event.ty, EventType::RecordUserTurnCompletion { .. }));

    let event = rx.try_recv().unwrap();
    assert!(event.is_subagent);
    assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
    match &event.ty {
        EventType::ContextUsagePercentage { model, percentage } => {
            assert_eq!(model.as_deref(), Some("claude-4-sonnet"));
            assert_eq!(*percentage, 42.5);
        },
        other => panic!("expected ContextUsagePercentage, got {other:?}"),
    }
}

#[test]
fn test_subagent_context_marks_chat_and_turn_telemetry() {
    let (mut obs, mut rx) = make_observer_with_subagent(true);

    obs.handle_event("subagent-session", &make_loop_event(success_stream_end()));

    let event = rx.try_recv().unwrap();
    assert!(event.is_subagent);
    assert!(matches!(event.ty, EventType::ChatAddedMessage { .. }));

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: Some(Duration::from_secs(1)),
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("subagent-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    assert!(event.is_subagent);
    match &event.ty {
        EventType::RecordUserTurnCompletion { args, .. } => {
            assert!(args.is_subagent);
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_turn_completion_accumulates_token_counts() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
            result: Ok(Message::new(
                Uuid::new_v4().to_string(),
                Role::Assistant,
                vec![ContentBlock::Text("hello".into())],
                None,
            )),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: Some(MetadataEvent {
                    metrics: None,
                    usage: Some(MetadataUsage {
                        input_tokens: Some(10),
                        output_tokens: Some(5),
                        cache_read_input_tokens: Some(2),
                        cache_write_input_tokens: Some(3),
                        context_usage_percentage: None,
                    }),
                    service: None,
                    metering_usage: Vec::new(),
                    stop_reason: None,
                    refusal: None,
                }),
                request_attempts: None,
            },
        }),
    );
    let _ = rx.try_recv();

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: None,
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { args, .. } => {
            assert_eq!(args.total_tokens, Some(17));
            assert_eq!(args.uncached_input_tokens, Some(10));
            assert_eq!(args.output_tokens, Some(5));
            assert_eq!(args.cache_read_input_tokens, Some(2));
            assert_eq!(args.cache_write_input_tokens, Some(3));
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_error_turn_propagates_reason() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::Throttling)),
    );
    let _ = rx.try_recv();
    let _ = rx.try_recv();

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: None,
        end_reason: LoopEndReason::Error,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { result, args, .. } => {
            assert_eq!(*result, TelemetryResult::Failed);
            assert_eq!(args.reason.as_deref(), Some(REASON_QUOTA_BREACH));
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_turn_completion_carries_last_request_attempts() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::ResponseStreamEnd {
            result: Ok(Message::new(
                Uuid::new_v4().to_string(),
                Role::Assistant,
                vec![ContentBlock::Text("ok".into())],
                None,
            )),
            metadata: StreamMetadata {
                tool_uses: vec![],
                stream: None,
                request_attempts: Some(1),
            },
        }),
    );
    let _ = rx.try_recv();

    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end_with_attempts(StreamErrorKind::Throttling, 3)),
    );
    let _ = rx.try_recv();
    let _ = rx.try_recv();
    assert!(matches!(
        rx.try_recv().unwrap().ty,
        EventType::AutomaticRetryCompleted {
            retry_reason: metric::RetryReason::Throttled,
            additional_attempts: 2,
            outcome: metric::RetryOutcome::Exhausted,
            ..
        }
    ));

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 2,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: None,
        end_reason: LoopEndReason::Error,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { result, args, .. } => {
            assert_eq!(*result, TelemetryResult::Failed);
            assert_eq!(args.request_attempts, Some(3));
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_turn_completion_attempts_is_none_when_transport_does_not_report() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event("test-session", &make_loop_event(success_stream_end()));
    let _ = rx.try_recv();

    let metadata = UserTurnMetadata {
        loop_id: test_loop_id(),
        result: None,
        message_ids: vec![],
        total_request_count: 1,
        number_of_cycles: 0,
        builtin_tool_uses: 0,
        turn_duration: None,
        end_reason: LoopEndReason::UserTurnEnd,
        end_timestamp: chrono::Utc::now(),
        input_token_count: 0,
        output_token_count: 0,
        cache_read_input_token_count: 0,
        cache_write_input_token_count: 0,
        model: None,
        assistant_response_length: 0,
        request_attempts: None,
        context_usage_percentage: None,
        final_context_usage_percentage: None,
        metering_usage: Vec::new(),
        user_prompt_length: 0,
    };
    obs.handle_event("test-session", &AgentEvent::EndTurn(metadata));

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::RecordUserTurnCompletion { args, .. } => {
            assert!(args.request_attempts.is_none());
        },
        other => panic!("expected RecordUserTurnCompletion, got {other:?}"),
    }
}

#[test]
fn test_acp_client_app_type() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client_info = Some(AcpClientInfo::new("Sugarmaker".into(), "2.0".into()));
    let ctx = TelemetryContext::new(model_provider("claude-4-sonnet"), client_info, false);
    let mut obs = TelemetryObserver::new_for_test(tx, ctx);
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));

    let event = rx.try_recv().unwrap();
    assert_eq!(event.app_type.as_deref(), Some("ACP"));
    assert_eq!(event.engine, Some(metric::Engine::V2));
    assert_eq!(event.acp_client_name.as_deref(), Some("Sugarmaker"));
    assert_eq!(event.client_application.as_deref(), Some("acp_external"));
    assert_eq!(event.session_interface, Some(metric::SessionInterface::ExternalAcp));
}

#[test]
fn test_first_party_one_shot_context() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client_info = Some(AcpClientInfo::new(
        crate::context::KIRO_CLI_NON_INTERACTIVE_CLIENT_NAME.into(),
        "1.0.0".into(),
    ));
    let ctx = TelemetryContext::new(model_provider("claude-4-sonnet"), client_info, false);
    let mut obs = TelemetryObserver::new_for_test(tx, ctx);
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));

    let event = rx.try_recv().unwrap();
    assert_eq!(event.app_type.as_deref(), Some("V2"));
    assert_eq!(event.engine, Some(metric::Engine::V2));
    assert_eq!(event.client_application.as_deref(), Some("chat_cli_v2"));
    assert_eq!(
        event.session_interface,
        Some(metric::SessionInterface::NoninteractiveCli)
    );
}

#[test]
fn test_use_aws_tool_call_uses_builtin_metric_origin() {
    let (mut obs, mut rx) = make_observer();
    let tool_call = agent::protocol::ToolCall {
        id: "tool-aws".to_string(),
        tool: agent::tools::Tool {
            tool_use_purpose: None,
            kind: ToolKind::BuiltIn(BuiltInTool::UseAws(agent::tools::use_aws::UseAws {
                service_name: "s3".to_string(),
                operation_name: "ListBuckets".to_string(),
                positional_args: None,
                parameters: None,
                region: "us-east-1".to_string(),
                profile_name: None,
                label: None,
            })),
        },
        tool_use_block: agent::agent_loop::types::ToolUseBlock {
            tool_use_id: "tool-aws".to_string(),
            name: "use_aws".to_string(),
            input: serde_json::json!({}),
        },
    };

    obs.handle_event(
        "test-session",
        &AgentEvent::Update(UpdateEvent::ToolCall(tool_call.clone())),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Update(UpdateEvent::ToolCallFinished {
            tool_call,
            result: ToolCallResult::Cancelled,
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ToolUseSuggested {
            aws_service_name,
            aws_operation_name,
            ..
        } => {
            assert_eq!(aws_service_name.as_deref(), Some("s3"));
            assert_eq!(aws_operation_name.as_deref(), Some("ListBuckets"));
        },
        other => panic!("expected ToolUseSuggested, got {other:?}"),
    }

    let records = event_to_otel_metric_records(&event);
    let tool_call = records
        .iter()
        .find(|record| record.name == "kiro_cli_tool_call_total")
        .expect("kiro_cli_tool_call_total");
    assert!(
        tool_call
            .attributes
            .iter()
            .any(|attr| { attr.key == "tool_origin" && attr.value == "builtin" })
    );
    assert!(
        tool_call
            .attributes
            .iter()
            .any(|attr| { attr.key == "builtin_tool_name" && attr.value == "use_aws" })
    );
}

#[test]
fn test_failed_tool_call_without_tracker_emits_denied_mcp_telemetry() {
    let (mut obs, mut rx) = make_observer();

    obs.handle_event(
        "test-session",
        &AgentEvent::Update(UpdateEvent::ToolCallFailed {
            tool_use_id: "tool-mcp".to_string(),
            tool_name: "sanitized_custom_tool".to_string(),
            tool_identity: Some(ToolCallIdentity {
                tool_name: "custom_tool".to_string(),
                mcp_server_name: Some("local-server".to_string()),
            }),
            raw_input: serde_json::json!({}),
            reason: ToolCallFailureReason::PermissionDenied,
            error: "denied".to_string(),
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ToolUseSuggested {
            tool_name,
            mcp_server_name,
            is_accepted,
            is_valid,
            is_custom_tool,
            ..
        } => {
            assert_eq!(tool_name.as_deref(), Some("custom_tool"));
            assert_eq!(mcp_server_name.as_deref(), Some("local-server"));
            assert!(!is_accepted);
            assert_eq!(*is_valid, Some(true));
            assert!(*is_custom_tool);
        },
        other => panic!("expected ToolUseSuggested, got {other:?}"),
    }

    let records = event_to_otel_metric_records(&event);
    let invocations = records
        .iter()
        .find(|record| record.name == "kiro_cli_tool_call_total")
        .expect("kiro_cli_tool_call_total");
    assert!(
        invocations
            .attributes
            .iter()
            .any(|attr| { attr.key == "tool_origin" && attr.value == "mcp" })
    );
    assert!(
        invocations
            .attributes
            .iter()
            .any(|attr| { attr.key == "tool_outcome" && attr.value == "denied" })
    );
}

#[test]
fn test_extract_reason_from_stream_error_kind() {
    let err = StreamError::new(StreamErrorKind::Throttling);
    assert_eq!(extract_reason_from_kind(&err).0, REASON_QUOTA_BREACH);

    let err = StreamError::new(StreamErrorKind::ModelOverloaded {
        message: "overloaded".to_string(),
    });
    assert_eq!(extract_reason_from_kind(&err).0, REASON_MODEL_OVERLOADED);

    let err = StreamError::new(StreamErrorKind::MonthlyLimitReached {
        message: "limit reached".to_string(),
    });
    assert_eq!(extract_reason_from_kind(&err).0, REASON_MONTHLY_LIMIT_REACHED);

    let err = StreamError::new(StreamErrorKind::ContextWindowOverflow);
    assert_eq!(extract_reason_from_kind(&err).0, REASON_CONTEXT_WINDOW_OVERFLOW);

    let err = StreamError::new(StreamErrorKind::Interrupted);
    assert_eq!(extract_reason_from_kind(&err).0, REASON_INTERRUPTED);
}

#[test]
fn test_kiro_client_is_v2() {
    let info = AcpClientInfo::new(KIRO_ACP_CLIENT_NAME.into(), "1.0.0".into());
    assert_eq!(info.app_type(), AppType::V2);
    assert_eq!(info.name, ClientName::Kiro);
    assert_eq!(info.session_interface(), metric::SessionInterface::InteractiveCli);
}

#[test]
fn test_first_party_one_shot_client_is_v2() {
    let info = AcpClientInfo::new(
        crate::context::KIRO_CLI_NON_INTERACTIVE_CLIENT_NAME.into(),
        "1.0.0".into(),
    );
    assert_eq!(info.app_type(), AppType::V2);
    assert_eq!(info.name, ClientName::KiroCliNonInteractive);
    assert_eq!(info.session_interface(), metric::SessionInterface::NoninteractiveCli);
}

#[test]
fn test_external_client_is_acp() {
    let info = AcpClientInfo::new("external-editor".into(), "2.0".into());
    assert_eq!(info.app_type(), AppType::Acp);
    assert_eq!(info.name, ClientName::Other("external-editor".into()));
    assert_eq!(info.session_interface(), metric::SessionInterface::ExternalAcp);
}

#[test]
fn test_external_client_name_is_sanitized_for_telemetry() {
    let info = AcpClientInfo::new("  Sugar\u{2028}maker\u{200b}  ".into(), "2.0".into());
    assert_eq!(info.name, ClientName::Other("Sugarmaker".into()));

    let long = format!("{} ", "a".repeat(64));
    let info = AcpClientInfo::new(long, "2.0".into());
    assert_eq!(info.name, ClientName::Other("a".repeat(64)));

    let info = AcpClientInfo::new("\u{0}\u{2028}".into(), "2.0".into());
    assert_eq!(info.name, ClientName::Unknown);
}
