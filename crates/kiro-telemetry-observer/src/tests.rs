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
    StreamTimeoutSource,
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
    let client_info = Some(AcpClientInfo::new("external-client".into(), "2.0".into()));
    let ctx = TelemetryContext::new(model_provider("claude-4-sonnet"), client_info, false);
    let mut obs = TelemetryObserver::new_for_test(tx, ctx);
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));

    let event = rx.try_recv().unwrap();
    assert_eq!(event.app_type.as_deref(), Some("ACP"));
    assert_eq!(event.engine, Some(metric::Engine::V2));
    assert_eq!(event.acp_client_name.as_deref(), Some("external-client"));
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

/// Pins the V2 per-tool duration pipeline end to end: the executed-tool flow
/// (ToolCall -> ToolExecutionStart -> ToolCallFinished) must emit a
/// ToolUseSuggested with a measured execution_duration, and the shared legacy
/// translation must turn that event into `kiro_cli_tool_execution_duration_ms`
/// with the tool dimension — the same series V1 records, so V2 traffic shows up
/// in the existing production distribution queries.
#[test]
fn executed_tool_emits_execution_duration_metric() {
    let (mut obs, mut rx) = make_observer();
    let tool_call = agent::protocol::ToolCall {
        id: "tool-dur".to_string(),
        tool: agent::tools::Tool {
            tool_use_purpose: None,
            kind: ToolKind::BuiltIn(BuiltInTool::ExecuteCmd(agent::tools::execute_cmd::ExecuteCmd {
                command: "echo hi".to_string(),
                working_dir: None,
            })),
        },
        tool_use_block: agent::agent_loop::types::ToolUseBlock {
            tool_use_id: "tool-dur".to_string(),
            name: "execute_cmd".to_string(),
            input: serde_json::json!({}),
        },
    };

    obs.handle_event(
        "test-session",
        &AgentEvent::Update(UpdateEvent::ToolCall(tool_call.clone())),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::TaskExecutor(Box::new(
            agent::task_executor::TaskExecutorEvent::ToolExecutionStart(
                agent::task_executor::ToolExecutionStartEvent {
                    id: agent::task_executor::ToolExecutionId::new("tool-dur".to_string()),
                    tool: tool_call.tool.clone(),
                    start_time: chrono::Utc::now(),
                },
            ),
        ))),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Update(UpdateEvent::ToolCallFinished {
            tool_call,
            result: ToolCallResult::Success(agent::tools::ToolExecutionOutput::new(vec![])),
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::ToolUseSuggested {
            execution_duration,
            tool_name,
            ..
        } => {
            assert_eq!(tool_name.as_deref(), Some("shell"));
            assert!(
                execution_duration.is_some(),
                "an executed tool must carry a measured execution duration"
            );
        },
        other => panic!("expected ToolUseSuggested, got {other:?}"),
    }

    let records = event_to_otel_metric_records(&event);
    let duration_record = records
        .iter()
        .find(|r| r.name == "kiro_cli_tool_execution_duration_ms")
        .expect("executed tool must produce the duration histogram");
    // "shell" canonicalizes to "execute_bash" so V1 and V2 land in one series.
    assert!(
        duration_record
            .attributes
            .iter()
            .any(|a| a.key == "builtin_tool_name" && a.value == "execute_bash"),
        "duration histogram must carry the tool dimension; got {duration_record:?}"
    );
    assert!(
        duration_record
            .attributes
            .iter()
            .any(|a| a.key == "agent_engine" && a.value == "v2"),
        "duration histogram must carry the engine dimension; got {duration_record:?}"
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

fn test_turn_metadata() -> UserTurnMetadata {
    UserTurnMetadata {
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
    }
}

#[test]
fn stream_stall_warning_emits_soft_stall_event() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::StreamStallWarning {
            idle: Duration::from_secs(31),
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::StreamStall { model, tier } => {
            assert_eq!(model.as_deref(), Some("claude-4-sonnet"));
            assert_eq!(*tier, metric::StallTier::Soft);
        },
        other => panic!("expected StreamStall, got {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "soft warning must emit exactly one event");
}

/// Every episode-end value the loop can produce must survive to the metric
/// attribute: an `as_str()`/schema mismatch panics on the emission path
/// (`record_stream_stall_idle_seconds` ends in `.expect_valid()`).
#[test]
fn stream_stall_failed_and_cancelled_emit_episode_end() {
    for (event, expected) in [
        (
            AgentLoopEventKind::StreamStallFailed {
                idle: Duration::from_secs(45),
            },
            metric::StallEpisodeEnd::Failed,
        ),
        (
            AgentLoopEventKind::StreamStallCancelled {
                idle: Duration::from_secs(45),
            },
            metric::StallEpisodeEnd::Cancelled,
        ),
    ] {
        let (mut obs, mut rx) = make_observer();
        obs.handle_event("test-session", &make_loop_event(event));
        let event = rx.try_recv().unwrap();
        match &event.ty {
            EventType::StreamStallEpisodeEnd {
                idle_seconds,
                episode_end,
                ..
            } => {
                assert_eq!(*idle_seconds, 45.0);
                assert_eq!(*episode_end, expected);
            },
            other => panic!("expected StreamStallEpisodeEnd, got {other:?}"),
        }
    }
}

#[test]
fn stream_stall_resumed_emits_episode_end() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::StreamStallResumed {
            idle: Duration::from_secs(45),
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::StreamStallEpisodeEnd {
            idle_seconds,
            episode_end,
            ..
        } => {
            assert_eq!(*idle_seconds, 45.0);
            assert_eq!(*episode_end, metric::StallEpisodeEnd::Resumed);
        },
        other => panic!("expected StreamStallEpisodeEnd, got {other:?}"),
    }
}

/// A soft warning escalating to the hard cancel is ONE silent gap: it must
/// count one stall (at the warning) and close with one episode end (the hard
/// cancel), not count a second hard-tier stall for the same gap.
#[test]
fn escalated_stall_counts_once_and_closes_once() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::StreamStallWarning {
            idle: Duration::from_secs(60),
        }),
    );
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::StreamTimeout {
            duration: Duration::from_secs(300),
            source: StreamTimeoutSource::IdleWatchdog,
        })),
    );

    let mut stalls = Vec::new();
    let mut episode_ends = Vec::new();
    while let Ok(event) = rx.try_recv() {
        match &event.ty {
            EventType::StreamStall { tier, .. } => stalls.push(*tier),
            EventType::StreamStallEpisodeEnd { episode_end, .. } => episode_ends.push(*episode_end),
            _ => {},
        }
    }
    assert_eq!(
        stalls,
        vec![metric::StallTier::Soft],
        "the escalated gap must count exactly one stall, at the warning"
    );
    assert_eq!(
        episode_ends,
        vec![metric::StallEpisodeEnd::HardCancelled],
        "the escalated gap must close exactly once, as hard_cancelled"
    );
}

/// The compaction escalation mirrors the main-loop one: a forwarded compaction
/// warning followed by the dedicated CompactionStreamStalled event is one gap —
/// one stall (soft) and one episode end (hard_cancelled).
#[test]
fn escalated_compaction_stall_counts_once_and_closes_once() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::StreamStallWarning {
            idle: Duration::from_secs(60),
        }),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::CompactionStreamStalled {
            idle: Duration::from_secs(300),
        }),
    );

    let mut stalls = Vec::new();
    let mut episode_ends = Vec::new();
    while let Ok(event) = rx.try_recv() {
        match &event.ty {
            EventType::StreamStall { tier, .. } => stalls.push(*tier),
            EventType::StreamStallEpisodeEnd { episode_end, .. } => episode_ends.push(*episode_end),
            _ => {},
        }
    }
    assert_eq!(
        stalls,
        vec![metric::StallTier::Soft],
        "the escalated compaction gap must count exactly one stall"
    );
    assert_eq!(
        episode_ends,
        vec![metric::StallEpisodeEnd::HardCancelled],
        "the escalated compaction gap must close exactly once"
    );
}

#[test]
fn hard_stall_stream_end_emits_hard_stall_and_episode_end() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::StreamTimeout {
            duration: Duration::from_secs(120),
            source: StreamTimeoutSource::IdleWatchdog,
        })),
    );

    let mut saw_hard_stall = false;
    let mut saw_episode_end = false;
    while let Ok(event) = rx.try_recv() {
        match &event.ty {
            EventType::StreamStall { tier, .. } => {
                assert_eq!(*tier, metric::StallTier::Hard);
                saw_hard_stall = true;
            },
            EventType::StreamStallEpisodeEnd {
                idle_seconds,
                episode_end,
                ..
            } => {
                assert_eq!(*idle_seconds, 120.0);
                assert_eq!(*episode_end, metric::StallEpisodeEnd::HardCancelled);
                saw_episode_end = true;
            },
            _ => {},
        }
    }
    assert!(saw_hard_stall, "expected a hard-tier StreamStall event");
    assert!(saw_episode_end, "expected a hard_cancelled StreamStallEpisodeEnd event");
}

/// The whole stall family must be gated on the same producer: a series member
/// existing without its family (e.g. a recovery sample with no matching stall)
/// skews any cross-series dashboard join.
#[test]
fn sdk_recv_timeout_emits_no_member_of_the_stall_family() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::StreamTimeout {
            duration: Duration::from_secs(59),
            source: StreamTimeoutSource::SdkRecv,
        })),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRetry {
            outcome: agent::protocol::StallRetryOutcome::Recovered,
            attempt_number: 1,
            partial_output: false,
            source: StreamTimeoutSource::SdkRecv,
        }),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRecovery {
            recovery: Duration::from_secs(3),
            source: StreamTimeoutSource::SdkRecv,
        }),
    );

    while let Ok(event) = rx.try_recv() {
        assert!(
            !matches!(
                &event.ty,
                EventType::StreamStall { .. }
                    | EventType::StreamStallEpisodeEnd { .. }
                    | EventType::StreamStallRetry { .. }
                    | EventType::StreamStallRecovery { .. }
            ),
            "SDK recv timeouts must not land in ANY watchdog stall series, got {:?}",
            event.ty
        );
    }
}

/// Positive counterpart: a watchdog-sourced stall episode emits every member of
/// the family coherently.
#[test]
fn watchdog_stall_emits_the_full_stall_family() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::StreamTimeout {
            duration: Duration::from_secs(300),
            source: StreamTimeoutSource::IdleWatchdog,
        })),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRecovery {
            recovery: Duration::from_secs(4),
            source: StreamTimeoutSource::IdleWatchdog,
        }),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRetry {
            outcome: agent::protocol::StallRetryOutcome::Recovered,
            attempt_number: 1,
            partial_output: false,
            source: StreamTimeoutSource::IdleWatchdog,
        }),
    );

    let mut saw = (false, false, false, false);
    while let Ok(event) = rx.try_recv() {
        match &event.ty {
            EventType::StreamStall { .. } => saw.0 = true,
            EventType::StreamStallEpisodeEnd { .. } => saw.1 = true,
            EventType::StreamStallRetry { .. } => saw.2 = true,
            EventType::StreamStallRecovery { .. } => saw.3 = true,
            _ => {},
        }
    }
    assert_eq!(
        saw,
        (true, true, true, true),
        "watchdog stall episode must emit all four family members (stall, episode_end, retry, recovery)"
    );
}

#[test]
fn transient_retry_counted_at_execution_not_schedule() {
    let (mut obs, mut rx) = make_observer();
    // Schedule-time event drives the banner only; a cancel during backoff means
    // no request is ever sent, so it must not count.
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::TransientRetry {
            class: agent::error_recovery::TransientErrorClass::Throttle,
            attempt_number: 1,
            backoff: Duration::from_secs(2),
            partial_output: false,
        }),
    );
    while let Ok(event) = rx.try_recv() {
        assert!(
            !matches!(&event.ty, EventType::TransientRetry { .. }),
            "scheduled (not yet executed) retry must not be counted, got {:?}",
            event.ty
        );
    }

    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::TransientRetryExecuted {
            class: agent::error_recovery::TransientErrorClass::Throttle,
            attempt_number: 1,
            partial_output: false,
        }),
    );
    let event = rx.try_recv().expect("executed retry should emit the counter event");
    match &event.ty {
        EventType::TransientRetry {
            class, attempt_number, ..
        } => {
            assert_eq!(*class, metric::TransientErrorClass::Throttle);
            assert_eq!(*attempt_number, 1);
        },
        other => panic!("expected TransientRetry, got {other:?}"),
    }
}

#[test]
fn stall_retry_events_translate_to_event_types() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRetry {
            outcome: agent::protocol::StallRetryOutcome::Recovered,
            attempt_number: 2,
            partial_output: true,
            source: StreamTimeoutSource::IdleWatchdog,
        }),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRecovery {
            source: StreamTimeoutSource::IdleWatchdog,
            recovery: Duration::from_secs(3),
        }),
    );

    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::StreamStallRetry {
            outcome,
            attempt_number,
            partial_output,
            ..
        } => {
            assert_eq!(*outcome, metric::RetryOutcome::Recovered);
            assert_eq!(*attempt_number, 2);
            assert_eq!(*partial_output, Some(true));
        },
        other => panic!("expected StreamStallRetry, got {other:?}"),
    }
    let event = rx.try_recv().unwrap();
    match &event.ty {
        EventType::StreamStallRecovery { recovery_seconds, .. } => {
            assert_eq!(*recovery_seconds, 3.0);
        },
        other => panic!("expected StreamStallRecovery, got {other:?}"),
    }
}

#[test]
fn turn_completion_carries_stall_counters() {
    let (mut obs, mut rx) = make_observer();
    // One soft warning escalating to a hard cancel (a single episode, counted
    // once) + one exhausted retry sequence of 2 attempts.
    obs.handle_event(
        "test-session",
        &make_loop_event(AgentLoopEventKind::StreamStallWarning {
            idle: Duration::from_secs(31),
        }),
    );
    obs.handle_event(
        "test-session",
        &make_loop_event(error_stream_end(StreamErrorKind::StreamTimeout {
            duration: Duration::from_secs(120),
            source: StreamTimeoutSource::IdleWatchdog,
        })),
    );
    obs.handle_event(
        "test-session",
        &AgentEvent::Internal(InternalEvent::StreamStallRetry {
            outcome: agent::protocol::StallRetryOutcome::Exhausted,
            attempt_number: 2,
            partial_output: false,
            source: StreamTimeoutSource::IdleWatchdog,
        }),
    );
    obs.handle_event("test-session", &AgentEvent::EndTurn(test_turn_metadata()));

    let mut saw_completion = false;
    while let Ok(event) = rx.try_recv() {
        if let EventType::RecordUserTurnCompletion { args, .. } = &event.ty {
            assert_eq!(
                args.stream_stall_count,
                Some(1),
                "a warning escalating to a hard cancel is one episode"
            );
            assert_eq!(args.stream_stall_retries, Some(2), "2 continuation retries");
            saw_completion = true;
        }
    }
    assert!(saw_completion, "expected RecordUserTurnCompletion");
}

#[test]
fn turn_completion_without_stalls_carries_no_stall_counters() {
    let (mut obs, mut rx) = make_observer();
    obs.handle_event("test-session", &make_loop_event(success_stream_end()));
    obs.handle_event("test-session", &AgentEvent::EndTurn(test_turn_metadata()));

    let mut saw_completion = false;
    while let Ok(event) = rx.try_recv() {
        if let EventType::RecordUserTurnCompletion { args, .. } = &event.ty {
            assert_eq!(args.stream_stall_count, None);
            assert_eq!(args.stream_stall_retries, None);
            saw_completion = true;
        }
    }
    assert!(saw_completion, "expected RecordUserTurnCompletion");
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

/// The attempt-number telemetry bucket must cover every attempt either retry
/// budget can produce: raising a budget without widening the bucket range would
/// silently fold the new top attempts into `unknown`, degrading the very series
/// the budgets were instrumented for.
#[test]
fn attempt_buckets_cover_the_full_retry_budgets() {
    for budget in [
        agent::error_recovery::MAX_TRANSIENT_RETRIES,
        agent::consts::MAX_STREAM_TIMEOUT_RETRIES,
    ] {
        for attempt in 1..=budget as u32 {
            assert_ne!(
                metric::AttemptNumberBucket::from_attempt(attempt).as_str(),
                "unknown",
                "attempt {attempt} of a {budget}-retry budget must map to a real bucket"
            );
        }
    }
}

mod permission_outcome_ordering {
    use agent::protocol::{
        PermissionEvalResult,
        ToolApprovalOutcome,
        ToolTargetScope,
    };
    use agent::tools::fs_write::{
        FileCreate,
        FsWrite,
    };
    use agent::tools::{
        BuiltInTool,
        ToolKind,
    };
    use kiro_telemetry::metric;

    use super::{
        event_to_otel_metric_records,
        make_observer,
    };

    /// Drives one tool call, sending the permission decision BEFORE the tool
    /// call is announced, which is the real order the agent produces: it
    /// evaluates a whole batch first. Returns the emitted `approval_path`.
    fn reported_approval_path(
        result: PermissionEvalResult,
        approval: Option<ToolApprovalOutcome>,
        execute: bool,
    ) -> Option<String> {
        let (mut obs, mut rx) = make_observer();
        let session = "test-session";

        let tool_call = agent::protocol::ToolCall {
            id: "tool-1".to_string(),
            tool: agent::tools::Tool {
                tool_use_purpose: None,
                kind: ToolKind::BuiltIn(BuiltInTool::FileWrite(FsWrite::Create(FileCreate {
                    path: "/home/u/project/f.txt".to_string(),
                    content: String::new(),
                    start_line: None,
                }))),
            },
            tool_use_block: agent::agent_loop::types::ToolUseBlock {
                tool_use_id: "tool-1".to_string(),
                name: "fs_write".to_string(),
                input: serde_json::json!({}),
            },
        };

        // Permission first, exactly as the agent orders it.
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Internal(crate::observer::InternalEvent::ToolPermissionEvalResult {
                tool_use_id: "tool-1".to_string(),
                tool: tool_call.tool.clone(),
                result,
                target_scope: ToolTargetScope::Workspace,
            }),
        );
        if let Some(outcome) = approval {
            obs.handle_event(
                session,
                &crate::observer::AgentEvent::Internal(crate::observer::InternalEvent::ToolApprovalResult {
                    tool_use_id: "tool-1".to_string(),
                    outcome,
                }),
            );
        }
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Update(crate::observer::UpdateEvent::ToolCall(tool_call.clone())),
        );
        let outcome = if execute {
            crate::observer::ToolCallResult::Success(Default::default())
        } else {
            crate::observer::ToolCallResult::Cancelled
        };
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Update(crate::observer::UpdateEvent::ToolCallFinished {
                tool_call,
                result: outcome,
            }),
        );

        let event = rx.try_recv().expect("tool use event");
        event_to_otel_metric_records(&event)
            .iter()
            .find(|record| record.name == "kiro_cli_tool_call_total")
            .expect("kiro_cli_tool_call_total")
            .attributes
            .iter()
            .find(|attribute| attribute.key == "approval_path")
            .map(|attribute| attribute.value.clone())
    }

    #[test]
    fn a_rule_allowed_call_is_auto_allowed_even_though_permission_arrives_first() {
        // Without buffering, this decision is dropped because the tracker does
        // not exist yet, and the call is misreported as `user_approved` -- which
        // is what trust-all and every silently-allowed call produced in KUTS.
        assert_eq!(
            reported_approval_path(PermissionEvalResult::Allow, None, true),
            Some("auto_allowed".to_string())
        );
    }

    #[test]
    fn a_denied_call_is_reported_as_denied() {
        assert_eq!(
            reported_approval_path(
                PermissionEvalResult::Deny {
                    reason: "blocked".to_string()
                },
                None,
                false
            ),
            Some("denied".to_string())
        );
    }

    #[test]
    fn a_prompt_result_is_reported_without_inferring_from_execution() {
        assert_eq!(
            reported_approval_path(PermissionEvalResult::ask(), Some(ToolApprovalOutcome::Approved), true),
            Some("user_approved".to_string())
        );
        assert_eq!(
            reported_approval_path(PermissionEvalResult::ask(), Some(ToolApprovalOutcome::Denied), false),
            Some("denied".to_string())
        );
        assert_eq!(
            reported_approval_path(PermissionEvalResult::ask(), None, false),
            Some("unknown".to_string())
        );
    }

    #[test]
    fn a_deny_decision_is_recorded_rather_than_defaulted_to() {
        // `denied` is also what a tracker with no recorded decision collapses to,
        // because emission does `unwrap_or(false)` on both flags. So asserting
        // the emitted string cannot tell "the Deny arrived" from "nothing
        // arrived" -- this asserts the tracker state instead, which can.
        let (mut obs, _rx) = make_observer();
        let session = "test-session";

        let tool_call = agent::protocol::ToolCall {
            id: "tool-1".to_string(),
            tool: agent::tools::Tool {
                tool_use_purpose: None,
                kind: ToolKind::BuiltIn(BuiltInTool::FileWrite(FsWrite::Create(FileCreate {
                    path: "/etc/passwd".to_string(),
                    content: String::new(),
                    start_line: None,
                }))),
            },
            tool_use_block: agent::agent_loop::types::ToolUseBlock {
                tool_use_id: "tool-1".to_string(),
                name: "fs_write".to_string(),
                input: serde_json::json!({}),
            },
        };
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Internal(crate::observer::InternalEvent::ToolPermissionEvalResult {
                tool_use_id: "tool-1".to_string(),
                tool: tool_call.tool.clone(),
                result: PermissionEvalResult::Deny {
                    reason: "blocked".to_string(),
                },
                target_scope: ToolTargetScope::System,
            }),
        );
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Update(crate::observer::UpdateEvent::ToolCall(tool_call)),
        );

        let tracker = &obs.sessions[session].tool_trackers["tool-1"];
        assert_eq!(
            tracker.is_accepted,
            Some(false),
            "the refusal must be recorded, not absent"
        );
        assert_eq!(tracker.is_trusted, Some(false));
        assert_eq!(tracker.approval_path, Some(metric::ApprovalPath::Denied));
    }

    #[test]
    fn a_call_with_no_permission_decision_is_unknown() {
        let (mut obs, mut rx) = make_observer();
        let session = "test-session";

        let tool_call = agent::protocol::ToolCall {
            id: "tool-1".to_string(),
            tool: agent::tools::Tool {
                tool_use_purpose: None,
                kind: ToolKind::BuiltIn(BuiltInTool::FileWrite(FsWrite::Create(FileCreate {
                    path: "/home/u/project/f.txt".to_string(),
                    content: String::new(),
                    start_line: None,
                }))),
            },
            tool_use_block: agent::agent_loop::types::ToolUseBlock {
                tool_use_id: "tool-1".to_string(),
                name: "fs_write".to_string(),
                input: serde_json::json!({}),
            },
        };
        // No ToolPermissionEvalResult at all.
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Update(crate::observer::UpdateEvent::ToolCall(tool_call.clone())),
        );
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Update(crate::observer::UpdateEvent::ToolCallFinished {
                tool_call,
                result: crate::observer::ToolCallResult::Cancelled,
            }),
        );

        let event = rx.try_recv().expect("tool use event");
        let reported = event_to_otel_metric_records(&event)
            .iter()
            .find(|record| record.name == "kiro_cli_tool_call_total")
            .expect("kiro_cli_tool_call_total")
            .attributes
            .iter()
            .find(|attribute| attribute.key == "approval_path")
            .map(|attribute| attribute.value.clone());
        assert_eq!(
            reported,
            Some("unknown".to_string()),
            "a missing authorization observation must not be reported as a denial"
        );
    }

    #[test]
    fn a_parse_failure_reports_unknown_rather_than_an_approval_nobody_gave() {
        // A model-generated call that could not be parsed never reached
        // authorization. `is_accepted` has to stay true so the outcome is
        // `error` and not `denied`, and the two flags would then derive
        // `user_approved` -- inflating the bucket that counts how often we
        // interrupt people, with calls no one was ever asked about.
        let (mut obs, mut rx) = make_observer();
        let session = "test-session";

        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Update(crate::observer::UpdateEvent::ToolCallFailed {
                tool_use_id: "tool-1".to_string(),
                tool_name: "fs_read".to_string(),
                tool_identity: None,
                raw_input: serde_json::json!({}),
                reason: agent::protocol::ToolCallFailureReason::ParseError,
                error: "could not parse tool input".to_string(),
            }),
        );

        let event = rx.try_recv().expect("tool use event");
        let records = event_to_otel_metric_records(&event);
        let record = records
            .iter()
            .find(|record| record.name == "kiro_cli_tool_call_total")
            .expect("kiro_cli_tool_call_total");
        let attribute = |key: &str| {
            record
                .attributes
                .iter()
                .find(|attribute| attribute.key == key)
                .map(|attribute| attribute.value.clone())
        };
        assert_eq!(attribute("approval_path"), Some("unknown".to_string()));
        // The call itself is still reported as a failure, unchanged.
        assert_eq!(attribute("tool_outcome"), Some("error".to_string()));
    }

    #[test]
    fn a_decision_for_a_tool_that_never_runs_does_not_outlive_its_turn() {
        // A tool denied during evaluation is handed straight back to the model
        // and never announced, so nothing consumes its buffered decision. The
        // buffer has to be emptied at the turn boundary or it grows for the
        // lifetime of the session.
        let (mut obs, _rx) = make_observer();
        let session = "test-session";

        let tool = agent::tools::Tool {
            tool_use_purpose: None,
            kind: ToolKind::BuiltIn(BuiltInTool::FileWrite(FsWrite::Create(FileCreate {
                path: "/etc/passwd".to_string(),
                content: String::new(),
                start_line: None,
            }))),
        };
        obs.handle_event(
            session,
            &crate::observer::AgentEvent::Internal(crate::observer::InternalEvent::ToolPermissionEvalResult {
                tool_use_id: "tool-never-announced".to_string(),
                tool,
                result: PermissionEvalResult::Deny {
                    reason: "blocked".to_string(),
                },
                target_scope: ToolTargetScope::System,
            }),
        );
        assert_eq!(
            obs.sessions[session].pending_permissions.len(),
            1,
            "the decision should be buffered while its tool call might still arrive"
        );

        obs.handle_event(
            session,
            &crate::observer::AgentEvent::EndTurn(super::UserTurnMetadata {
                loop_id: super::test_loop_id(),
                result: None,
                message_ids: vec![],
                total_request_count: 1,
                number_of_cycles: 0,
                builtin_tool_uses: 0,
                turn_duration: Some(std::time::Duration::from_secs(1)),
                end_reason: super::LoopEndReason::UserTurnEnd,
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
        assert!(
            obs.sessions[session].pending_permissions.is_empty(),
            "a decision whose tool never ran must not survive the turn"
        );
    }
}
