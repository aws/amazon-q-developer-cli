pub mod cognito;
pub mod core;
pub mod endpoint;
pub mod legacy_sink;
pub mod observer;

use core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    ToolUseEventBuilder,
};

use kiro_telemetry::metric;
// Re-exported for parity with the V1 API.
#[allow(unused_imports)]
pub use kiro_telemetry_host::get_error_reason;
use kiro_telemetry_host::{
    Event,
    OtelEventTranslator,
};
pub use kiro_telemetry_host::{
    HostConfig,
    HostRole,
    InstallMethod,
    ReasonCode,
    TelemetryError,
    TelemetryThread,
    get_install_method,
    govcloud_partition,
};
use kiro_telemetry_legacy::{
    event_to_otel_log_record,
    event_to_otel_metric_records,
};

use crate::auth::builder_id::get_start_url_and_region;
use crate::cli::RootSubcommand;
use crate::constants::{
    BREW_CASK_NAME,
    CLI_NAME,
};
use crate::database::Database;
use crate::os::{
    Env,
    Fs,
};
pub use crate::telemetry::core::{
    EmptyResponseRetryOutcome,
    EventType,
    QProfileSwitchIntent,
    TelemetryResult,
};
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
    let otel_mode = env
        .get(KIRO_TELEMETRY_OTEL)
        .map_or(kiro_telemetry::OtelMode::Off, |value| {
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

/// Async/database-aware send helpers that wrap [`TelemetryThread::send_event`].
///
/// This trait stays in V2 (rather than the host crate) because the per-event
/// metadata enrichment reads from V2's [`Database`]; PR E moves it onto a
/// host-level `EventMetadataProvider` trait.
#[async_trait::async_trait]
pub trait TelemetryThreadV2Ext {
    async fn send_cli_subcommand_executed(
        &self,
        database: &Database,
        subcommand: &RootSubcommand,
    ) -> Result<(), TelemetryError>;

    async fn send_chat_slash_command_executed(
        &self,
        database: &Database,
        conversation_id: String,
        command: String,
        subcommand: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
    ) -> Result<(), TelemetryError>;

    #[allow(clippy::too_many_arguments)]
    async fn send_agent_contribution_metric(
        &self,
        database: &Database,
        conversation_id: String,
        utterance_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        lines_by_agent: Option<isize>,
        lines_by_user: Option<isize>,
    ) -> Result<(), TelemetryError>;

    async fn send_chat_added_message(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    ) -> Result<(), TelemetryError>;

    async fn send_record_user_turn_completion(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError>;

    #[allow(clippy::too_many_arguments)]
    async fn send_metering_event(
        &self,
        database: &Database,
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
    ) -> Result<(), TelemetryError>;

    async fn send_empty_response_retry(
        &self,
        database: &Database,
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    ) -> Result<(), TelemetryError>;

    async fn send_tangent_mode_session(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: TangentModeSessionArgs,
    ) -> Result<(), TelemetryError>;

    async fn send_tool_use_suggested(
        &self,
        database: &Database,
        event: ToolUseEventBuilder,
    ) -> Result<(), TelemetryError>;

    #[allow(clippy::too_many_arguments)]
    async fn send_mcp_server_init(
        &self,
        database: &Database,
        conversation_id: String,
        server_name: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
    ) -> Result<(), TelemetryError>;

    async fn send_agent_config_init(
        &self,
        database: &Database,
        conversation_id: String,
        args: AgentConfigInitArgs,
    ) -> Result<(), TelemetryError>;

    #[allow(clippy::too_many_arguments)]
    async fn send_response_error(
        &self,
        database: &Database,
        conversation_id: String,
        context_file_length: Option<usize>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        request_id: Option<String>,
        message_id: Option<String>,
    ) -> Result<(), TelemetryError>;

    fn send_daily_heartbeat(&self) -> Result<(), TelemetryError>;

    #[cfg(feature = "voice")]
    #[allow(clippy::too_many_arguments)]
    fn send_voice_input(
        &self,
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
    ) -> Result<(), TelemetryError>;
}

#[async_trait::async_trait]
impl TelemetryThreadV2Ext for TelemetryThread {
    async fn send_cli_subcommand_executed(
        &self,
        database: &Database,
        subcommand: &RootSubcommand,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::CliSubcommandExecuted {
            subcommand: subcommand.telemetry_name(),
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_chat_slash_command_executed(
        &self,
        database: &Database,
        conversation_id: String,
        command: String,
        subcommand: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::ChatSlashCommandExecuted {
            conversation_id,
            command,
            subcommand,
            result,
            reason,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_agent_contribution_metric(
        &self,
        database: &Database,
        conversation_id: String,
        utterance_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        lines_by_agent: Option<isize>,
        lines_by_user: Option<isize>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::AgentContribution {
            conversation_id,
            utterance_id,
            tool_use_id,
            tool_name,
            lines_by_agent,
            lines_by_user,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_chat_added_message(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id,
            result,
            data,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_record_user_turn_completion(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_metering_event(
        &self,
        database: &Database,
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::MeteringEvent {
            request_id,
            model,
            usage,
            unit,
            unit_plural,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_empty_response_retry(
        &self,
        database: &Database,
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::EmptyResponseRetry { model, outcome });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_tangent_mode_session(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: TangentModeSessionArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::TangentModeSession {
            conversation_id,
            result,
            args,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_tool_use_suggested(
        &self,
        database: &Database,
        event: ToolUseEventBuilder,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ToolUseSuggested {
            conversation_id: event.conversation_id,
            utterance_id: event.utterance_id,
            user_input_id: event.user_input_id,
            tool_use_id: event.tool_use_id,
            tool_name: event.tool_name,
            mcp_server_name: event.mcp_server_name,
            is_accepted: event.is_accepted,
            is_trusted: event.is_trusted,
            is_success: event.is_success,
            reason_desc: event.reason_desc,
            is_valid: event.is_valid,
            is_custom_tool: event.is_custom_tool,
            input_token_size: event.input_token_size,
            output_token_size: event.output_token_size,
            custom_tool_call_latency: event.custom_tool_call_latency,
            model: event.model,
            execution_duration: event.execution_duration,
            turn_duration: event.turn_duration,
            aws_service_name: event.aws_service_name,
            aws_operation_name: event.aws_operation_name,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send_event(telemetry_event)
    }

    async fn send_mcp_server_init(
        &self,
        database: &Database,
        conversation_id: String,
        server_name: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::McpServerInit {
            conversation_id,
            server_name,
            init_failure_reason,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            all_tools_count,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_agent_config_init(
        &self,
        database: &Database,
        conversation_id: String,
        args: AgentConfigInitArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::AgentConfigInit { conversation_id, args });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    async fn send_response_error(
        &self,
        database: &Database,
        conversation_id: String,
        context_file_length: Option<usize>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        request_id: Option<String>,
        message_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::MessageResponseError {
            result,
            reason,
            reason_desc,
            status_code,
            conversation_id,
            context_file_length,
            request_id,
            message_id,
            model: None,
        });
        set_event_metadata(database, &mut event).await;
        self.send_event(event)
    }

    fn send_daily_heartbeat(&self) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::DailyHeartbeat {
            install_method: Some(get_install_method(BREW_CASK_NAME, CLI_NAME).to_string()),
        });
        if let Some(client_app) = get_cli_client_application() {
            event.set_client_application(client_app);
        } else {
            event.set_client_application_kind(metric::ClientApplication::ChatCliV2);
        }
        self.send_event(event)
    }

    #[cfg(feature = "voice")]
    fn send_voice_input(
        &self,
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
        self.send_event(Event::new(EventType::VoiceInput {
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
}

pub(crate) async fn set_event_metadata(database: &Database, event: &mut Event) {
    let (start_url, region) = get_start_url_and_region(database).await;
    if let Some(start_url) = start_url {
        event.set_start_url(start_url);
    }
    if let Some(region) = region {
        event.set_sso_region(region);
    }

    // Set the client application from environment variable
    if let Some(client_app) = get_cli_client_application() {
        event.set_client_application(client_app);
    }
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

        // Drive translation through the V2 OTel translator and feed the records
        // directly to the OTel client (mirrors what the moved spawn loop does).
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
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                metric::ResultKind::Success,
                false,
                metric::Mode::Interactive,
            ),
        );
        expect_metric_attrs(turn_record, &[
            ("model_class", "anthropic_sonnet"),
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
        thread.send_daily_heartbeat().expect("send_daily_heartbeat");
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
            install_method: Some(get_install_method(BREW_CASK_NAME, CLI_NAME).to_string()),
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
