//! V2-specific HostConfig builder + V2-only `send_*` helpers that pull in
//! brew/CLI install constants. Pure file split out of mod.rs to close the
//! stage-2 gate (V2 mod.rs is now a thin shim of declarations + re-exports).

use kiro_telemetry::metric;
use kiro_telemetry_host::{
    Event,
    EventType,
    HostConfig,
    HostRole,
    OtelEventTranslator,
    TelemetryError,
    TelemetryThread,
    get_install_method,
    govcloud_partition,
};
use kiro_telemetry_legacy::{
    event_to_otel_log_record,
    event_to_otel_metric_records,
};

use crate::database::Database;
use crate::os::{
    Env,
    Fs,
};
use crate::telemetry::core::TelemetryResult;
use crate::telemetry::legacy_sink;
use crate::util::env_var::get_cli_client_application;
use crate::util::paths::GlobalPaths;

/// V2-side OTel translator that bridges `kiro_telemetry_host::Event` into OTel
/// records via `kiro-telemetry-legacy`.
#[derive(Debug)]
pub struct V2OtelTranslator;

impl OtelEventTranslator for V2OtelTranslator {
    fn metric_records(&self, event: &Event) -> Vec<kiro_telemetry::MetricRecord> {
        event_to_otel_metric_records(event)
    }

    fn log_record(&self, event: &Event) -> Option<kiro_telemetry::TelemetryLogRecord> {
        event_to_otel_log_record(event)
    }
}

/// Build a [`HostConfig`] for V2 from the existing `(env, fs, database, region)`
/// inputs. Wraps the env/database lookups + legacy-sink build that used to live
/// inline in `TelemetryClient::new`.
pub async fn build_v2_host_config(
    env: &Env,
    fs: &Fs,
    database: &mut Database,
    region: Option<&str>,
) -> Result<HostConfig, BuildHostConfigError> {
    let govcloud_partition = region.and_then(govcloud_partition);
    let telemetry_enabled = legacy_sink::resolve_telemetry_enabled(database);
    let client_id = legacy_sink::resolve_client_id(env, database, telemetry_enabled)?;
    let legacy_sink =
        legacy_sink::V2LegacySink::build(env, fs, database, govcloud_partition, client_id, telemetry_enabled).await?;
    let otel_config = otel_telemetry_config(env, telemetry_enabled, client_id);
    Ok(HostConfig {
        client_id,
        telemetry_enabled,
        otel_config,
        legacy_sink: Some(legacy_sink),
        otel_translator: Some(std::sync::Arc::new(V2OtelTranslator)),
        metadata_enricher: None,
        client_application: get_cli_client_application().map(|s| metric::ClientApplication::from_name(Some(&s))),
        host_role: HostRole::UserCli,
        govcloud_partition,
        consent_settings_path: GlobalPaths::settings_path().ok(),
    })
}

#[derive(Debug, thiserror::Error)]
pub enum BuildHostConfigError {
    #[error(transparent)]
    LegacySink(#[from] legacy_sink::LegacySinkError),
}

fn state_dir() -> std::path::PathBuf {
    GlobalPaths::database_path_static()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::env::temp_dir().join("kiro-cli"))
}

/// Default OTLP collector endpoint when `KIRO_TELEMETRY_OTLP_ENDPOINT` is not overridden.
const DEFAULT_OTLP_ENDPOINT: &str = "https://prod.us-east-1.telemetry-v2.kiro.dev";

fn otel_telemetry_config(env: &Env, telemetry_enabled: bool, client_id: uuid::Uuid) -> kiro_telemetry::TelemetryConfig {
    use crate::util::consts::env_var::{
        KIRO_TELEMETRY_OTEL,
        KIRO_TELEMETRY_OTLP_ENDPOINT,
        KIRO_TELEMETRY_OTLP_LOGS_ENABLED,
    };
    // Default to DualWrite (KUTS/OTel + legacy Toolkit) so pre-existing metrics
    // dual-hit both backends without opt-in. An explicit `KIRO_TELEMETRY_OTEL=0`
    // still parses to Off (the user opt-out), and `2` selects OtelOnly.
    let otel_mode = env
        .get(KIRO_TELEMETRY_OTEL)
        .map_or(kiro_telemetry::OtelMode::DualWrite, |value| {
            kiro_telemetry::OtelMode::parse(&value)
        });
    let otlp_endpoint = env
        .get(KIRO_TELEMETRY_OTLP_ENDPOINT)
        .ok()
        .or_else(|| Some(DEFAULT_OTLP_ENDPOINT.to_string()));
    let otlp_logs_enabled = env
        .get(KIRO_TELEMETRY_OTLP_LOGS_ENABLED)
        .is_ok_and(|value| value.trim() != "0");

    kiro_telemetry::TelemetryConfig::new(telemetry_enabled, otel_mode, otlp_endpoint, state_dir())
        .with_otlp_logs_enabled(otlp_logs_enabled)
        .with_machine_id(client_id.hyphenated().to_string())
}

/// Daily heartbeat send helper. Stays in V2 (rather than the observer's
/// extension trait) because it pulls in V2-specific brew/CLI install constants
/// and has no metadata enrichment path.
pub fn send_daily_heartbeat(thread: &TelemetryThread) -> Result<(), TelemetryError> {
    let mut event = Event::new(EventType::DailyHeartbeat {
        install_method: Some(get_install_method().to_string()),
    });
    if let Some(client_app) = get_cli_client_application() {
        event.set_client_application(client_app);
    } else {
        event.set_client_application_kind(metric::ClientApplication::ChatCliV2);
    }
    thread.send_event(event)
}

/// V2-side voice input send helper. Kept here (not on the observer extension
/// trait) because it has no metadata enrichment path — the event has no
/// session/auth fields to set.
#[cfg(feature = "voice")]
#[allow(clippy::too_many_arguments)]
pub fn send_voice_input(
    thread: &TelemetryThread,
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
) -> Result<(), TelemetryError> {
    thread.send_event(Event::new(EventType::VoiceInput {
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
    }))
}

#[cfg(test)]
mod test {
    use kiro_telemetry::testing::{
        InMemoryTelemetry,
        expect_log,
        expect_log_attrs,
        expect_metric,
        expect_metric_attrs,
        in_memory_telemetry,
    };
    use kiro_telemetry::{
        OtelMode,
        TelemetryConfig as OtelTelemetryConfig,
        metric,
    };
    use kiro_telemetry_host::{
        ChatAddedMessageParams,
        ChatConversationType,
        EmptyResponseRetryOutcome,
        Event,
        EventType,
        TelemetryResult,
    };
    use kiro_telemetry_legacy::event_to_otel_metric_record;

    use super::*;

    #[tokio::test]
    async fn host_thread_default_runs_with_no_sink() {
        let thread = TelemetryThread::new(HostConfig::default()).await.unwrap();
        thread.send_user_logged_in().unwrap();
        thread.finish().await.unwrap();
    }

    #[test]
    fn govcloud_partition_detects_gov_regions() {
        assert_eq!(govcloud_partition("us-gov-east-1"), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-gov-west-1"), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-east-1"), None);
    }

    #[tokio::test]
    async fn observer_emits_otel_metrics_for_chat_added_message() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let InMemoryTelemetry {
            providers: _,
            client,
            sink,
        } = in_memory_telemetry(
            OtelTelemetryConfig::new(true, OtelMode::DualWrite, None, tempdir.path().to_path_buf())
                .with_otlp_logs_enabled(true),
        );
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                model: Some("claude-4-sonnet".to_string()),
                time_to_first_chunk_ms: Some(250.0),
                request_duration_seconds: Some(1.5),
                output_tokens: Some(7),
                ..Default::default()
            },
        });
        event.client_application = Some("chat_cli_v2".to_string());

        let translator = V2OtelTranslator;
        for record in translator.metric_records(&event) {
            client.emit(record).unwrap();
        }
        if let Some(log) = translator.log_record(&event) {
            client.emit_log(log).unwrap();
        }

        let _ = ChatConversationType::ToolUse;

        let records = sink.records();
        let turn_record = expect_metric(
            &records,
            metric::user_turns(
                Some("claude-4-sonnet"),
                metric::ClientApplication::ChatCliV2,
                metric::ResultKind::Success,
                false,
                metric::Mode::Interactive,
            ),
        );
        expect_metric_attrs(turn_record, &[
            ("model", "claude-4-sonnet"),
            ("client_application", "chat_cli_v2"),
            ("result", "success"),
            ("mode", "interactive"),
        ]);
        let _ = expect_log;
        let _ = expect_log_attrs;
    }

    /// Captures every event the host thread forwards to a legacy sink, so a
    /// test can assert what `send_*` helpers actually queued.
    #[derive(Debug, Default)]
    struct CapturingSink {
        events: std::sync::Mutex<Vec<Event>>,
    }

    impl kiro_telemetry_host::LegacySink for CapturingSink {
        fn send_event(&self, event: Event) -> futures::future::BoxFuture<'_, ()> {
            self.events.lock().unwrap().push(event);
            Box::pin(async {})
        }

        fn send_event_govcloud(&self, _event: Event, _partition: &'static str) -> futures::future::BoxFuture<'_, ()> {
            Box::pin(async {})
        }
    }

    #[tokio::test]
    async fn send_daily_heartbeat_defaults_client_application_to_v2() {
        // SAFETY: env mutation is only safe in single-threaded test context.
        // Running under `cargo test --lib` defaults to a multi-thread runtime,
        // but this test does not race against any other test reading these
        // vars. We unset both overrides to exercise the default-fallback
        // branch in `send_daily_heartbeat`.
        // SAFETY: see comment above.
        unsafe {
            std::env::remove_var("KIRO_CLI_CLIENT_APPLICATION");
            std::env::remove_var("Q_CLI_CLIENT_APPLICATION");
        }

        let sink = std::sync::Arc::new(CapturingSink::default());
        let host_config = HostConfig {
            legacy_sink: Some(sink.clone()),
            ..HostConfig::default()
        };
        let thread = TelemetryThread::new(host_config).await.unwrap();
        super::send_daily_heartbeat(&thread).expect("send_daily_heartbeat");
        thread.finish().await.unwrap();

        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1, "expected exactly one heartbeat event");
        let event = &events[0];
        assert!(
            matches!(event.ty, EventType::DailyHeartbeat { .. }),
            "expected DailyHeartbeat, got {:?}",
            event.ty
        );
        assert_eq!(
            event.client_application.as_deref(),
            Some(metric::ClientApplication::ChatCliV2.as_str()),
            "default fallback must be chat_cli_v2 when no env override is set",
        );
    }

    #[test]
    fn daily_heartbeat_otel_record_uses_client_application_label() {
        let mut event = Event::new(EventType::DailyHeartbeat {
            install_method: Some(get_install_method().to_string()),
        });
        event.set_client_application_kind(metric::ClientApplication::ChatCliV2);
        let record = event_to_otel_metric_record(&event).expect("daily heartbeat metric");
        let install_method = match &event.ty {
            EventType::DailyHeartbeat { install_method } => install_method.clone(),
            other => panic!("expected daily heartbeat, got {other:?}"),
        };
        expect_metric(
            std::slice::from_ref(&record),
            metric::daily_heartbeat_record(metric::DailyHeartbeat::from_names(
                Some(metric::ClientApplication::ChatCliV2.as_str()),
                install_method.as_deref(),
            )),
        );
        let _ = EmptyResponseRetryOutcome::Recovered;
    }
}
