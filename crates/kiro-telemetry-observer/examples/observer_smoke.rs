//! End-to-end smoke test for the observer translation path.
//!
//! Wires up:
//!
//! ```text
//! AgentEvent ──▶ TelemetryObserver ──▶ Event
//!                                       │
//!                                       ▼
//!                              TelemetryThread (HostConfig)
//!                                       │
//!                                       ▼
//!                              kiro-telemetry-legacy translator
//!                                       │
//!                                       ▼
//!                              OTLP/HTTP export → otel-collector → Prometheus
//! ```
//!
//! Unlike `kiro-telemetry`'s `local_smoke` example (which calls `client.emit(metric::*)`
//! directly), this example exercises the *observer-specific* translation:
//! a synthetic `AgentLoopEventKind::ResponseStreamEnd` is fed through
//! `TelemetryObserver::handle_event`, which produces a `ChatAddedMessage`
//! `Event`, which the observer's forwarding task hands to `TelemetryThread`,
//! which fans the event through `V2OtelTranslator` (kiro-telemetry-legacy)
//! into OTel records. The `kiro_cli_model_invocations_total` counter is the canonical
//! observable side-effect of the observer-translated `ChatAddedMessage` path.

use std::env;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use agent::agent_loop::protocol::{
    AgentLoopEvent,
    AgentLoopEventKind,
    StreamMetadata,
};
use agent::agent_loop::types::{
    ContentBlock,
    Message,
    MetadataEvent,
    MetadataMetrics,
    MetadataService,
    Role,
};
use agent::protocol::{
    AgentEvent,
    InternalEvent,
};
use agent::types::AgentId;
use kiro_telemetry::{
    MetricRecord,
    OtelMode,
    TelemetryConfig,
    TelemetryLogRecord,
};
use kiro_telemetry_host::{
    Event,
    HostConfig,
    HostRole,
    OtelEventTranslator,
    TelemetryThread,
};
use kiro_telemetry_legacy::{
    event_to_otel_log_record,
    event_to_otel_metric_records,
};
use kiro_telemetry_observer::{
    AcpClientInfo,
    TelemetryContext,
    TelemetryObserver,
};
use uuid::Uuid;

#[derive(Debug)]
struct ExampleOtelTranslator;

impl OtelEventTranslator for ExampleOtelTranslator {
    fn metric_records(&self, event: &Event) -> Vec<MetricRecord> {
        event_to_otel_metric_records(event)
    }

    fn log_record(&self, event: &Event) -> Option<TelemetryLogRecord> {
        event_to_otel_log_record(event)
    }
}

fn success_stream_end() -> AgentLoopEventKind {
    let request_start_time = chrono::Utc::now();
    AgentLoopEventKind::ResponseStreamEnd {
        result: Ok(Message::new(
            Uuid::new_v4().to_string(),
            Role::Assistant,
            vec![ContentBlock::Text("hello from observer_smoke".into())],
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
                    request_id: Some("observer-smoke-req-1".into()),
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

fn make_loop_event(kind: AgentLoopEventKind) -> AgentEvent {
    AgentEvent::Internal(InternalEvent::AgentLoop(Box::new(AgentLoopEvent {
        id: agent::agent_loop::AgentLoopId::new(AgentId::default()),
        kind,
    })))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = env::var("KIRO_TELEMETRY_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string());

    // Build the OTel TelemetryConfig — same shape V2 uses, but with a stable
    // service identity so we can filter the resulting metric in Prometheus.
    let mut config_pairs = vec![
        ("KIRO_TELEMETRY_ENABLED".to_string(), "1".to_string()),
        ("KIRO_TELEMETRY_OTEL".to_string(), "2".to_string()),
        ("KIRO_TELEMETRY_OTLP_ENDPOINT".to_string(), endpoint.clone()),
        (
            "KIRO_TELEMETRY_EXPORT_INTERVAL_MS".to_string(),
            env::var("KIRO_TELEMETRY_EXPORT_INTERVAL_MS").unwrap_or_else(|_| "1000".to_string()),
        ),
    ];
    config_pairs.extend(env::vars().filter(|(k, _)| {
        // Forward existing OTel env, but don't double-set the four we just pinned.
        !matches!(
            k.as_str(),
            "KIRO_TELEMETRY_ENABLED"
                | "KIRO_TELEMETRY_OTEL"
                | "KIRO_TELEMETRY_OTLP_ENDPOINT"
                | "KIRO_TELEMETRY_EXPORT_INTERVAL_MS"
        )
    }));
    let otel_config = TelemetryConfig::from_pairs(config_pairs).with_machine_id("kiro-cli-observer-smoke");

    if otel_config.otel_mode == OtelMode::Off {
        return Err("otel_mode is Off; check KIRO_TELEMETRY_OTEL".into());
    }
    if otel_config.otlp_endpoint.is_none() {
        return Err("KIRO_TELEMETRY_OTLP_ENDPOINT must be set".into());
    }

    let host_config = HostConfig {
        client_id: Uuid::new_v4(),
        telemetry_enabled: true,
        otel_config,
        legacy_sink: None,
        otel_translator: Some(Arc::new(ExampleOtelTranslator)),
        metadata_enricher: None,
        client_application: None,
        host_role: HostRole::UserCli,
        govcloud_partition: None,
        consent_settings_path: None,
    };

    let telemetry_thread = TelemetryThread::new(host_config).await?;
    // The observer takes a sender-only handle; the original keeps the JoinHandle
    // and is awaited via finish_with_timeout below to drive the OTel SDK flush.
    let thread_for_observer = telemetry_thread.clone();

    let model_provider = Arc::new(|| Some("claude-4-sonnet".to_string()));
    let client_info = Some(AcpClientInfo::new("kiro-tui".to_string(), "1.0.0".to_string()));
    let context = TelemetryContext::new(model_provider, client_info, false);

    let observer_handle = TelemetryObserver::spawn(context, thread_for_observer, None, None, None);

    // Drive a synthetic successful stream-end through the observer.
    // This exercises observer.handle_response_stream_end ->
    // observer.emit(EventType::ChatAddedMessage{..}) -> forwarding task ->
    // TelemetryThread -> V2OtelTranslator -> OTel metric records.
    let session_id = format!("observer-smoke-{}", Uuid::new_v4());
    observer_handle.send_event(session_id.clone(), make_loop_event(success_stream_end()));

    // Give the observer + forwarding tasks a chance to drain, and the OTel
    // metric reader a chance to export at least once.
    tokio::time::sleep(Duration::from_millis(2500)).await;

    // Cleanly drain the host thread so the OTel SDK gets a chance to flush.
    telemetry_thread.finish_with_timeout(Duration::from_secs(3)).await?;

    println!("observer_smoke: pushed ResponseStreamEnd through TelemetryObserver -> TelemetryThread -> {endpoint}");
    println!("observer_smoke: session_id={session_id}");
    Ok(())
}
