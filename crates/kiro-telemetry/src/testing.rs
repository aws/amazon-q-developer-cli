use std::collections::HashMap;
use std::io::{
    BufRead,
    BufReader,
    Read,
    Write,
};
use std::net::{
    TcpListener,
    TcpStream,
};
use std::sync::{
    Arc,
    mpsc,
};
use std::time::{
    Duration,
    Instant,
};

use prost::Message as _;

use crate::{
    Attribute,
    InMemorySink,
    MetricRecord,
    MetricValue,
    OtelProviders,
    TelemetryClient,
    TelemetryConfig,
    TelemetryLogRecord,
    init_noop_otel,
};

pub struct InMemoryTelemetry {
    pub providers: OtelProviders,
    pub client: Arc<TelemetryClient>,
    pub sink: Arc<InMemorySink>,
}

pub fn in_memory_telemetry(config: TelemetryConfig) -> InMemoryTelemetry {
    let sink = Arc::new(InMemorySink::default());
    InMemoryTelemetry {
        providers: init_noop_otel(&config),
        client: Arc::new(TelemetryClient::new(config).with_sink(sink.clone())),
        sink,
    }
}

/// One representative record for every catalog metric that has a typed
/// constructor. This is the single source of truth shared by the `catalog_smoke`
/// example (live OTLP emit) and the catalog-coverage test (asserts the §5 catalog
/// is fully emittable). Derived recording-rule metrics are intentionally absent —
/// they are computed downstream, never emitted from the binary.
#[allow(clippy::vec_init_then_push)] // sectioned per §5 catalog for readability + interspersed extend()
pub fn catalog_metric_records() -> Vec<MetricRecord> {
    use crate::TokenUsage;
    use crate::metric::*;

    let invocation = InvocationContext::new(Some("claude-sonnet-4"), ClientApplication::ChatCliV3, false);
    let mut records = Vec::new();

    // §5.1 Usage & adoption
    records.push(cli_session_started(
        OsType::Macos,
        InstallSource::Internal,
        ClientApplication::ChatCliV3,
    ));
    records.push(user_logged_in(ClientApplication::ChatCliV3, CredentialKind::BuilderId));
    records.push(chat_session_started(Mode::Interactive, ClientApplication::ChatCliV3));
    records.push(ui_mode_session_started(
        UiMode::Tui,
        UiModeSource::Default,
        UiMode::Unset,
    ));
    records.push(ui_mode_changed(
        UiMode::Lite,
        UiMode::Tui,
        UiModeChangeSource::SlashCommand,
    ));
    records.push(ui_mode_default_changed(UiMode::Lite, UiMode::Tui));
    records.push(daily_heartbeat(ClientApplication::ChatCliV3, InstallSource::Internal));
    records.push(active_users_daily(
        1234.0,
        InstallSource::Brew,
        ClientApplication::ChatCliV3,
        false,
    ));
    records.push(active_users_weekly(
        5678.0,
        InstallSource::Internal,
        ClientApplication::ChatCliV3,
        true,
    ));
    records.push(active_users_monthly(42_000.0));
    records.push(dau_mau_ratio(0.21));
    records.push(new_users_daily(99.0, InstallSource::Download));
    records.push(client_version_seen(7.0, "2.6.1", ReleaseChannel::Stable, OsType::Macos));
    records.push(version_adoption_pct(
        63.5,
        VersionMinorBucket::Current,
        ReleaseChannel::Stable,
    ));
    records.push(stale_version_users(12.0, StalenessBucket::from_age_days(75)));
    records.push(mode_active_users_weekly(15.0, Mode::Plan));
    records.push(upgrade_completed(
        VersionMinorBucket::Older,
        VersionMinorBucket::Current,
        UpgradeTrigger::Auto,
    ));

    // §5.2 Feature usage
    records.push(slash_command_invoked("help"));
    records.push(feature_used("catalog_smoke"));
    records.push(feature_unique_users_weekly(8.0, "tangent"));
    records.push(tool_call_total(ToolOrigin::Builtin, Some("fs_read"), Outcome::Success));
    records.push(tool_using_sessions_pct(72.0));
    records.push(mcp_server_connected_total(McpServerClass::BuiltinFs));
    records.push(model_invocation(Some("claude-sonnet-4")));

    // §5.3 Performance
    records.push(bedrock_stream_ttft(
        0.25,
        Some("claude-sonnet-4"),
        PromptSizeBucket::Small,
        true,
    ));
    records.push(bedrock_stream_duration(
        2.0,
        Some("claude-sonnet-4"),
        crate::log::CompletionReason::Stop,
    ));
    records.push(bedrock_request_duration(
        1.5,
        Some("claude-sonnet-4"),
        Operation::Stream,
        Outcome::Success,
    ));
    records.push(bedrock_stream_inter_token_latency(0.05, Some("claude-sonnet-4")));
    records.push(startup_duration(0.5, VersionMinorBucket::Current, true, OsType::Macos));
    records.push(agent_loop_iteration_duration(2.0, LoopPhase::ModelCall));
    records.push(user_turn_duration_seconds(
        5.0,
        Some("claude-sonnet-4"),
        ChatConversationKind::Interactive,
        false,
        Mode::Interactive,
    ));
    records.push(time_to_first_chunk_ms(
        250.0,
        Some("claude-sonnet-4"),
        ClientApplication::ChatCliV3,
        false,
    ));

    // §5.4 Reliability
    records.push(cli_session_completed(ExitReason::Clean, AgentKind::Kas));
    records.push(crash_total(CrashKind::Panic, OsType::Macos, HostArch::Aarch64));
    records.push(startup_failure(FailureStage::Config, OsType::Macos));
    records.push(bedrock_request_error(
        Some("claude-sonnet-4"),
        Operation::Stream,
        ErrorKind::Throttling,
        StatusClass::Class5xx,
    ));
    records.push(empty_response_retry(Some("claude-sonnet-4"), Outcome::Recovered));
    records.push(retry_attempt(
        Upstream::Bedrock,
        RetryReason::Throttled,
        AttemptNumberBucket::One,
    ));
    records.push(retry_exhausted(Upstream::Bedrock, ErrorKind::Throttling));
    records.push(agent_loop_stuck(StuckPhase::ModelCall, StuckDetection::Watchdog));
    records.push(upstream_dependency_up(true, Dependency::Bedrock, Partition::Aws));

    // §5.5 Health (process)
    records.push(process_memory_rss(
        128.0 * 1024.0 * 1024.0,
        VersionMinorBucket::Current,
        AgentKind::Kas,
    ));
    records.push(process_cpu_utilization(
        0.25,
        VersionMinorBucket::Current,
        AgentKind::Kas,
        ProcessState::Streaming,
    ));
    records.push(process_memory_growth_rate(
        1024.0,
        VersionMinorBucket::Current,
        AgentKind::Kas,
    ));
    records.push(process_fds_open(64.0, VersionMinorBucket::Current, AgentKind::Kas));
    records.push(process_threads(12.0, VersionMinorBucket::Current, AgentKind::Kas));

    // §5.6 LLM-specific
    records.extend(token_records(invocation, TokenUsage {
        uncached_input_tokens: 1200,
        cache_read_input_tokens: 300,
        cache_write_input_tokens: 0,
        output_tokens: 128,
    }));
    records.push(cache_hit_ratio(
        0.5,
        Some("claude-sonnet-4"),
        ChatConversationKind::Interactive,
        ClientApplication::ChatCliV3,
    ));
    records.push(context_usage_percentage(
        50.0,
        Some("claude-sonnet-4"),
        ClientApplication::ChatCliV3,
        false,
    ));

    // §5.7 Tool-use & MCP
    records.push(tool_invocations(ToolOrigin::Builtin, Outcome::Success));
    records.push(tool_execution_duration_ms(42.0, ToolOrigin::Builtin, true));
    records.push(mcp_server_init_total(McpServerClass::BuiltinFs, Outcome::Success));

    // §5.8 Quality / outcomes
    records.push(user_turns(
        Some("claude-sonnet-4"),
        ClientApplication::ChatCliV3,
        ResultKind::Success,
        false,
        Mode::Interactive,
    ));
    records.push(session_outcome(SessionOutcome::TaskCompleted));
    records.push(user_feedback(Sentiment::Positive, FeedbackSurface::Chat));
    records.push(message_regenerated(Some("claude-sonnet-4")));
    records.push(turn_outcome_total(
        TurnOutcomeReason::Interrupted,
        Some("claude-sonnet-4"),
        Mode::Interactive,
        Engine::V3,
    ));
    records.push(subagent_delegations_total(
        SubagentNameClass::CodeReview,
        Some("claude-sonnet-4"),
        Engine::V3,
    ));
    records.push(mode_active_total(Mode::Interactive, Engine::V3));

    // §5.5b Health (process/perf — TUI-promoted)
    records.push(process_memory_peak_rss(
        256.0 * 1024.0 * 1024.0,
        VersionMinorBucket::Current,
        Engine::V3,
        ProcessRole::Tui,
    ));
    records.push(process_memory_heap_used(
        96.0 * 1024.0 * 1024.0,
        VersionMinorBucket::Current,
        Engine::V3,
        ProcessRole::Tui,
    ));
    records.push(tui_event_loop_delay(0.012, Engine::V3, ProcessRole::Tui));
    records.push(tui_input_latency(0.008, Engine::V3, ProcessRole::Tui));
    records.push(tui_render_duration(
        0.004,
        RenderKind::Full,
        Engine::V3,
        ProcessRole::Tui,
    ));

    // §5.9 Security & privacy (paired posture)
    records.push(telemetry_opt_out_respected(TelemetryChannel::Otel, EventClass::Metric));
    records.push(telemetry_opt_out_violation(TelemetryChannel::Otel));
    records.push(pii_redaction_run(
        Redactor::Default,
        EventClass::Log,
        TelemetryChannel::Otel,
        RedactionResult::Scrubbed,
    ));
    records.push(pii_redaction_match(1, PiiType::Email, FieldClass::Prompt));
    records.push(pii_redaction_error(
        Redactor::Default,
        ErrorKind::Other,
        RedactionFailAction::Dropped,
    ));
    records.push(consent_record_integrity(
        ConsentCheckKind::Hash,
        ConsentIntegrityResult::Ok,
    ));
    records.push(govcloud_channel_disabled(
        TelemetryChannel::LegacyToolkit,
        Partition::AwsUsGov,
        PostureReason::GovcloudDisabled,
    ));
    records.push(govcloud_channel_leak(TelemetryChannel::Otel));
    records.push(kuts_export_oversize(TelemetrySignal::Metrics));
    records.push(auth_credential_failure(
        AuthProvider::BuilderId,
        "ExpiredToken",
        Operation::Refresh,
        Partition::Aws,
    ));
    records.push(auth_unexpected_identity(
        Partition::Aws,
        Partition::AwsUsGov,
        Operation::Login,
    ));
    records.push(tls_validation_failure(
        DestinationClass::PublicInternet,
        TlsFailureReason::CertExpired,
    ));
    records.push(tool_egress_destinations(
        DestinationClass::AwsEndpoint,
        UrlScheme::Https,
        true,
    ));

    // §5.10 Telemetry-on-telemetry
    records.push(telemetry_export_send_attempt(
        TelemetryExporter::Otel,
        TelemetrySignal::Metrics,
        ExportOutcome::Success,
    ));
    records.push(telemetry_export_send_duration(
        0.1,
        TelemetryExporter::Otel,
        TelemetrySignal::Metrics,
    ));
    records.push(telemetry_exporter_dropped(
        TelemetryExporter::Otel,
        TelemetrySignal::Metrics,
        DropReason::QueueFull,
    ));
    records.push(telemetry_queue_depth(
        3.0,
        TelemetryExporter::Otel,
        TelemetrySignal::Metrics,
    ));
    records.push(telemetry_batch_size(
        10.0,
        TelemetryExporter::Otel,
        TelemetrySignal::Metrics,
    ));
    records.push(telemetry_emit_failure(
        TelemetrySubsystem::Exporter,
        EmitFailureKind::Io,
    ));
    records.push(telemetry_sdk_up(Partition::Aws, OsType::Macos, ReleaseChannel::Stable));
    records.push(telemetry_flush_on_exit_dropped(1, ShutdownPath::Clean));
    records.push(meta_meter_up(Partition::Aws, OsType::Macos));

    records
}

/// One representative record for every catalog `log_event`.
pub fn catalog_log_records() -> Vec<TelemetryLogRecord> {
    use crate::log;
    use crate::metric::{
        McpServerClass,
        Outcome,
    };

    vec![
        log::client_identity("catalog-client", "brew", 20_000, false),
        log::feature_first_use("catalog-client", "tangent_mode", "catalog-session", "slash_command"),
        log::metering_event(Some("req-1"), Some("claude-sonnet-4"), None, 42.0, "token", "tokens"),
        log::user_turn_completed(
            "catalog-conversation",
            crate::metric::ResultKind::Success,
            false,
            12,
            48,
            5,
            1,
        )
        .build(),
        log::tool_invoked(
            Some("tool-1"),
            Some("fs_read"),
            None,
            Some(true),
            Some("claude-sonnet-4"),
            Some(42.0),
        ),
        log::mcp_server_init("fs", McpServerClass::BuiltinFs, Outcome::Success),
        log::subagent_invoked("code-review")
            .model(Some("claude-sonnet-4"))
            .build(),
        log::conversation_completed("catalog-session", "catalog-conversation", log::CompletionReason::Stop),
    ]
}

#[derive(Debug)]
pub struct CapturedOtlpRequest {
    pub path: String,
    pub headers: HashMap<String, String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl CapturedOtlpRequest {
    pub fn is_metrics(&self) -> bool {
        self.path == "/v1/metrics"
    }

    pub fn is_logs(&self) -> bool {
        self.path == "/v1/logs"
    }
}

pub struct OtlpTestCollector {
    endpoint: String,
    expected_requests: usize,
    rx: mpsc::Receiver<CapturedOtlpRequest>,
}

impl OtlpTestCollector {
    pub fn start(expected_requests: usize) -> Self {
        Self::start_with_statuses(vec![200; expected_requests])
    }

    pub fn start_with_statuses(statuses: Vec<u16>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock OTLP collector");
        listener.set_nonblocking(true).expect("set mock collector nonblocking");
        let endpoint = format!("http://{}", listener.local_addr().expect("collector local addr"));
        let (tx, rx) = mpsc::channel();
        let expected_requests = statuses.len();

        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut statuses = statuses.into_iter();
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let status = statuses.next().unwrap_or(200);
                        if let Ok(request) = read_http_request(stream, status) {
                            let _ = tx.send(request);
                        }
                    },
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    },
                    Err(_) => break,
                }
            }
        });

        Self {
            endpoint,
            expected_requests,
            rx,
        }
    }

    pub fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    pub fn collect(self) -> Vec<CapturedOtlpRequest> {
        self.collect_timeout(Duration::from_secs(5))
    }

    pub fn collect_timeout(self, timeout: Duration) -> Vec<CapturedOtlpRequest> {
        (0..self.expected_requests)
            .map(|_| self.rx.recv_timeout(timeout).expect("mock collector request"))
            .collect()
    }
}

#[track_caller]
pub fn expect_otlp_request<'a>(requests: &'a [CapturedOtlpRequest], path: &str) -> &'a CapturedOtlpRequest {
    requests
        .iter()
        .find(|request| request.path == path)
        .unwrap_or_else(|| panic!("expected OTLP request to {path}; got {:?}", request_paths(requests)))
}

#[track_caller]
pub fn expect_otlp_metric(requests: &[CapturedOtlpRequest], expected: &MetricRecord) {
    for request in requests.iter().filter(|request| request.is_metrics()) {
        let metrics = decode_metrics_request(request);
        if metrics
            .resource_metrics
            .iter()
            .flat_map(|resource_metrics| &resource_metrics.scope_metrics)
            .flat_map(|scope_metrics| &scope_metrics.metrics)
            .any(|metric| metric_matches(metric, expected))
        {
            return;
        }
    }

    panic!(
        "expected OTLP metric `{}` value {:?} with attributes {:?}; exported metrics: {:?}",
        expected.name,
        expected.value,
        expected.attributes,
        otlp_metric_names(requests)
    );
}

#[track_caller]
pub fn expect_otlp_log(requests: &[CapturedOtlpRequest], expected: &TelemetryLogRecord) {
    for request in requests.iter().filter(|request| request.is_logs()) {
        let logs = decode_logs_request(request);
        if logs
            .resource_logs
            .iter()
            .flat_map(|resource_logs| &resource_logs.scope_logs)
            .flat_map(|scope_logs| &scope_logs.log_records)
            .any(|record| log_matches(record, expected))
        {
            return;
        }
    }

    panic!(
        "expected OTLP log `{}` with attributes {:?}; exported logs: {:?}",
        expected.name,
        expected.attributes,
        otlp_log_event_names(requests)
    );
}

pub fn otlp_metric_names(requests: &[CapturedOtlpRequest]) -> Vec<String> {
    requests
        .iter()
        .filter(|request| request.is_metrics())
        .flat_map(|request| decode_metrics_request(request).resource_metrics)
        .flat_map(|resource_metrics| resource_metrics.scope_metrics)
        .flat_map(|scope_metrics| scope_metrics.metrics)
        .map(|metric| metric.name)
        .collect()
}

pub fn otlp_log_event_names(requests: &[CapturedOtlpRequest]) -> Vec<String> {
    requests
        .iter()
        .filter(|request| request.is_logs())
        .flat_map(|request| decode_logs_request(request).resource_logs)
        .flat_map(|resource_logs| resource_logs.scope_logs)
        .flat_map(|scope_logs| scope_logs.log_records)
        .map(|record| record.event_name)
        .collect()
}

#[track_caller]
pub fn expect_otlp_metric_resource_attribute(requests: &[CapturedOtlpRequest], key: &str, expected: &str) {
    assert!(
        requests.iter().filter(|request| request.is_metrics()).any(|request| {
            decode_metrics_request(request)
                .resource_metrics
                .iter()
                .any(|resource_metrics| resource_has_attribute(resource_metrics.resource.as_ref(), key, expected))
        }),
        "missing metric resource attribute {key}={expected}"
    );
}

#[track_caller]
pub fn expect_otlp_log_resource_attribute(requests: &[CapturedOtlpRequest], key: &str, expected: &str) {
    assert!(
        requests.iter().filter(|request| request.is_logs()).any(|request| {
            decode_logs_request(request)
                .resource_logs
                .iter()
                .any(|resource_logs| resource_has_attribute(resource_logs.resource.as_ref(), key, expected))
        }),
        "missing log resource attribute {key}={expected}"
    );
}

pub fn attribute_value<'a>(attributes: &'a [Attribute], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .map(|attribute| attribute.value.as_str())
}

pub fn has_attributes(attributes: &[Attribute], expected: &[(&str, &str)]) -> bool {
    expected
        .iter()
        .all(|(key, value)| attribute_value(attributes, key) == Some(*value))
}

#[track_caller]
pub fn expect_attributes(attributes: &[Attribute], expected: &[(&str, &str)]) {
    for (key, value) in expected {
        assert_eq!(
            attribute_value(attributes, key),
            Some(*value),
            "expected attribute `{key}` to be `{value}` in {attributes:?}",
        );
    }
}

pub fn has_attribute_records(attributes: &[Attribute], expected: &[Attribute]) -> bool {
    expected
        .iter()
        .all(|expected| attribute_value(attributes, &expected.key) == Some(expected.value.as_str()))
}

pub fn attribute_records_eq(actual: &[Attribute], expected: &[Attribute]) -> bool {
    actual.len() == expected.len() && has_attribute_records(actual, expected)
}

pub fn metric_attr<'a>(record: &'a MetricRecord, key: &str) -> Option<&'a str> {
    attribute_value(&record.attributes, key)
}

#[track_caller]
pub fn expect_metric_attrs(record: &MetricRecord, expected: &[(&str, &str)]) {
    expect_attributes(&record.attributes, expected);
}

pub fn log_attr<'a>(record: &'a TelemetryLogRecord, key: &str) -> Option<&'a str> {
    attribute_value(&record.attributes, key)
}

#[track_caller]
pub fn expect_log_attrs(record: &TelemetryLogRecord, expected: &[(&str, &str)]) {
    expect_attributes(&record.attributes, expected);
}

pub fn count_metric_records(records: &[MetricRecord], name: &str, attributes: &[(&str, &str)]) -> usize {
    records
        .iter()
        .filter(|record| record.name == name && has_attributes(&record.attributes, attributes))
        .count()
}

pub fn has_metric_record(records: &[MetricRecord], name: &str, attributes: &[(&str, &str)]) -> bool {
    count_metric_records(records, name, attributes) > 0
}

pub fn metric_record<'a>(records: &'a [MetricRecord], name: &str) -> Option<&'a MetricRecord> {
    records.iter().find(|record| record.name == name)
}

#[track_caller]
pub fn expect_metric_value(record: &MetricRecord, expected: MetricValue) {
    assert_eq!(
        record.value, expected,
        "expected metric `{}` value {:?}, got {:?}",
        record.name, expected, record.value
    );
}

#[track_caller]
pub fn expect_metric_record<'a>(records: &'a [MetricRecord], name: &str) -> &'a MetricRecord {
    metric_record(records, name).unwrap_or_else(|| panic!("expected metric record `{name}`"))
}

pub fn metric_record_with_attrs<'a>(
    records: &'a [MetricRecord],
    name: &str,
    attributes: &[(&str, &str)],
) -> Option<&'a MetricRecord> {
    records
        .iter()
        .find(|record| record.name == name && has_attributes(&record.attributes, attributes))
}

pub fn metric_record_like<'a>(records: &'a [MetricRecord], expected: &MetricRecord) -> Option<&'a MetricRecord> {
    records.iter().find(|record| {
        record.name == expected.name
            && record.value == expected.value
            && attribute_records_eq(&record.attributes, &expected.attributes)
            && attribute_records_eq(&record.resource_attributes, &expected.resource_attributes)
    })
}

#[track_caller]
pub fn expect_metric_record_with_attrs<'a>(
    records: &'a [MetricRecord],
    name: &str,
    attributes: &[(&str, &str)],
) -> &'a MetricRecord {
    metric_record_with_attrs(records, name, attributes)
        .unwrap_or_else(|| panic!("expected metric record `{name}` with attributes {attributes:?}"))
}

#[track_caller]
pub fn expect_metric(records: &[MetricRecord], expected: MetricRecord) -> &MetricRecord {
    metric_record_like(records, &expected).unwrap_or_else(|| {
        panic!(
            "expected metric record `{}` value {:?} with attributes {:?}",
            expected.name, expected.value, expected.attributes
        )
    })
}

#[track_caller]
pub fn expect_counter_metric<'a>(
    records: &'a [MetricRecord],
    name: &str,
    value: u64,
    attributes: &[(&str, &str)],
) -> &'a MetricRecord {
    let record = expect_metric_record_with_attrs(records, name, attributes);
    expect_metric_value(record, MetricValue::Counter(value));
    record
}

#[track_caller]
pub fn expect_float_counter_metric<'a>(
    records: &'a [MetricRecord],
    name: &str,
    value: f64,
    attributes: &[(&str, &str)],
) -> &'a MetricRecord {
    let record = expect_metric_record_with_attrs(records, name, attributes);
    expect_metric_value(record, MetricValue::FloatCounter(value));
    record
}

#[track_caller]
pub fn expect_histogram_metric<'a>(
    records: &'a [MetricRecord],
    name: &str,
    value: f64,
    attributes: &[(&str, &str)],
) -> &'a MetricRecord {
    let record = expect_metric_record_with_attrs(records, name, attributes);
    expect_metric_value(record, MetricValue::Histogram(value));
    record
}

#[track_caller]
pub fn expect_gauge_metric<'a>(
    records: &'a [MetricRecord],
    name: &str,
    value: f64,
    attributes: &[(&str, &str)],
) -> &'a MetricRecord {
    let record = expect_metric_record_with_attrs(records, name, attributes);
    expect_metric_value(record, MetricValue::Gauge(value));
    record
}

pub fn count_log_records(records: &[TelemetryLogRecord], name: &str, attributes: &[(&str, &str)]) -> usize {
    records
        .iter()
        .filter(|record| record.name == name && has_attributes(&record.attributes, attributes))
        .count()
}

pub fn has_log_record(records: &[TelemetryLogRecord], name: &str, attributes: &[(&str, &str)]) -> bool {
    count_log_records(records, name, attributes) > 0
}

pub fn log_record<'a>(records: &'a [TelemetryLogRecord], name: &str) -> Option<&'a TelemetryLogRecord> {
    records.iter().find(|record| record.name == name)
}

#[track_caller]
pub fn expect_log_record<'a>(records: &'a [TelemetryLogRecord], name: &str) -> &'a TelemetryLogRecord {
    log_record(records, name).unwrap_or_else(|| panic!("expected log record `{name}`"))
}

pub fn log_record_with_attrs<'a>(
    records: &'a [TelemetryLogRecord],
    name: &str,
    attributes: &[(&str, &str)],
) -> Option<&'a TelemetryLogRecord> {
    records
        .iter()
        .find(|record| record.name == name && has_attributes(&record.attributes, attributes))
}

pub fn log_record_like<'a>(
    records: &'a [TelemetryLogRecord],
    expected: &TelemetryLogRecord,
) -> Option<&'a TelemetryLogRecord> {
    records.iter().find(|record| {
        record.name == expected.name
            && attribute_records_eq(&record.attributes, &expected.attributes)
            && attribute_records_eq(&record.resource_attributes, &expected.resource_attributes)
    })
}

#[track_caller]
pub fn expect_log_record_with_attrs<'a>(
    records: &'a [TelemetryLogRecord],
    name: &str,
    attributes: &[(&str, &str)],
) -> &'a TelemetryLogRecord {
    log_record_with_attrs(records, name, attributes)
        .unwrap_or_else(|| panic!("expected log record `{name}` with attributes {attributes:?}"))
}

#[track_caller]
pub fn expect_log(records: &[TelemetryLogRecord], expected: TelemetryLogRecord) -> &TelemetryLogRecord {
    log_record_like(records, &expected).unwrap_or_else(|| {
        panic!(
            "expected log record `{}` with attributes {:?}",
            expected.name, expected.attributes
        )
    })
}

fn read_http_request(mut stream: TcpStream, status: u16) -> std::io::Result<CapturedOtlpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let path = request_line.split_whitespace().nth(1).unwrap_or("").to_string();

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        read_chunked_body(&mut reader)?
    } else {
        let content_length = headers
            .get("content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body)?;
        body
    };

    let reason = match status {
        200 => "OK",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
    )?;
    stream.flush()?;

    Ok(CapturedOtlpRequest {
        path,
        headers: headers.clone(),
        content_type: headers.get("content-type").cloned(),
        body,
    })
}

fn read_chunked_body(reader: &mut BufReader<TcpStream>) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line)?;
        let size = usize::from_str_radix(size_line.trim().split(';').next().unwrap_or("0"), 16).unwrap_or(0);
        if size == 0 {
            loop {
                let mut trailer = String::new();
                reader.read_line(&mut trailer)?;
                if trailer.trim_end_matches(['\r', '\n']).is_empty() {
                    break;
                }
            }
            break;
        }

        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        let mut crlf = [0; 2];
        reader.read_exact(&mut crlf)?;
    }
    Ok(body)
}

fn request_paths(requests: &[CapturedOtlpRequest]) -> Vec<&str> {
    requests.iter().map(|request| request.path.as_str()).collect()
}

fn decode_metrics_request(
    request: &CapturedOtlpRequest,
) -> opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest {
    opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest::decode(request.body.as_slice())
        .expect("decode OTLP metrics request")
}

fn decode_logs_request(
    request: &CapturedOtlpRequest,
) -> opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest {
    opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest::decode(request.body.as_slice())
        .expect("decode OTLP logs request")
}

fn metric_matches(metric: &opentelemetry_proto::tonic::metrics::v1::Metric, expected: &MetricRecord) -> bool {
    metric.name == expected.name && metric_data_matches(metric.data.as_ref(), expected)
}

fn metric_data_matches(
    data: Option<&opentelemetry_proto::tonic::metrics::v1::metric::Data>,
    expected: &MetricRecord,
) -> bool {
    use opentelemetry_proto::tonic::metrics::v1::metric::Data;

    match (data, expected.value.clone()) {
        (Some(Data::Sum(sum)), MetricValue::Counter(value)) => sum
            .data_points
            .iter()
            .any(|point| number_data_point_matches(point, &expected.attributes, NumberValue::Integer(value as i64))),
        (Some(Data::Sum(sum)), MetricValue::FloatCounter(value)) => sum
            .data_points
            .iter()
            .any(|point| number_data_point_matches(point, &expected.attributes, NumberValue::Float(value))),
        (Some(Data::Gauge(gauge)), MetricValue::Gauge(value)) => gauge
            .data_points
            .iter()
            .any(|point| number_data_point_matches(point, &expected.attributes, NumberValue::Float(value))),
        (Some(Data::Histogram(histogram)), MetricValue::Histogram(value)) => histogram
            .data_points
            .iter()
            .any(|point| proto_attributes_include(&point.attributes, &expected.attributes) && point.sum == Some(value)),
        _ => false,
    }
}

enum NumberValue {
    Integer(i64),
    Float(f64),
}

fn number_data_point_matches(
    point: &opentelemetry_proto::tonic::metrics::v1::NumberDataPoint,
    expected_attributes: &[Attribute],
    expected_value: NumberValue,
) -> bool {
    use opentelemetry_proto::tonic::metrics::v1::number_data_point::Value;

    if !proto_attributes_include(&point.attributes, expected_attributes) {
        return false;
    }

    match (&point.value, expected_value) {
        (Some(Value::AsInt(actual)), NumberValue::Integer(expected)) => *actual == expected,
        (Some(Value::AsDouble(actual)), NumberValue::Float(expected)) => *actual == expected,
        (Some(Value::AsInt(actual)), NumberValue::Float(expected)) => *actual as f64 == expected,
        (Some(Value::AsDouble(actual)), NumberValue::Integer(expected)) => *actual == expected as f64,
        _ => false,
    }
}

fn log_matches(record: &opentelemetry_proto::tonic::logs::v1::LogRecord, expected: &TelemetryLogRecord) -> bool {
    record.event_name == expected.name && proto_attributes_include(&record.attributes, &expected.attributes)
}

fn proto_attributes_include(
    actual_attributes: &[opentelemetry_proto::tonic::common::v1::KeyValue],
    expected_attributes: &[Attribute],
) -> bool {
    expected_attributes.iter().all(|expected| {
        proto_attribute_value(actual_attributes, &expected.key).is_some_and(|actual| actual == expected.value)
    })
}

fn resource_has_attribute(
    resource: Option<&opentelemetry_proto::tonic::resource::v1::Resource>,
    key: &str,
    expected: &str,
) -> bool {
    resource.is_some_and(|resource| {
        proto_attribute_value(&resource.attributes, key).is_some_and(|actual| actual == expected)
    })
}

fn proto_attribute_value<'a>(
    attributes: &'a [opentelemetry_proto::tonic::common::v1::KeyValue],
    key: &str,
) -> Option<&'a str> {
    use opentelemetry_proto::tonic::common::v1::any_value::Value;

    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| attribute.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            Value::StringValue(value) => Some(value.as_str()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MetricRecord,
        MetricValue,
        TelemetryLogRecord,
        log,
        metric,
    };

    #[test]
    fn typed_metric_expectations_match_value_and_attributes() {
        let records = vec![
            MetricRecord::counter("example_total", 2).with_attribute("kind", "other"),
            MetricRecord::counter("example_total", 1).with_attribute("kind", "target"),
        ];

        let record = expect_counter_metric(&records, "example_total", 1, &[("kind", "target")]);

        assert_eq!(metric_attr(record, "kind"), Some("target"));
    }

    #[test]
    fn in_memory_telemetry_captures_emitted_records() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let harness = in_memory_telemetry(
            TelemetryConfig::new(true, crate::OtelMode::DualWrite, None, tempdir.path().to_path_buf())
                .with_otlp_logs_enabled(true),
        );

        harness
            .client
            .emit(metric::model_invocation(Some("claude-sonnet-4")))
            .expect("metric should emit");
        harness
            .client
            .emit_log(log::conversation_completed(
                "session-1",
                "conversation-1",
                log::CompletionReason::Stop,
            ))
            .expect("log should emit");

        expect_metric(
            &harness.sink.records(),
            metric::model_invocation(Some("claude-sonnet-4")),
        );
        expect_log(
            &harness.sink.log_records(),
            log::conversation_completed("session-1", "conversation-1", log::CompletionReason::Stop),
        );
    }

    #[test]
    fn typed_metric_expectations_match_constructed_records() {
        let expected = metric::chat_session_started(metric::Mode::Plan, metric::ClientApplication::ChatCliV2);
        let records = vec![
            metric::chat_session_started(metric::Mode::Interactive, metric::ClientApplication::ChatCliV2),
            expected.clone(),
        ];

        let record = expect_metric(&records, expected);

        assert_eq!(metric_attr(record, "mode"), Some("plan"));
    }

    #[test]
    fn metric_value_expectation_accepts_matching_kind() {
        let record = MetricRecord::histogram("duration_seconds", 0.25);

        expect_metric_value(&record, MetricValue::Histogram(0.25));
    }

    #[test]
    fn log_expectations_match_attributes() {
        let records = vec![TelemetryLogRecord::new("fact").with_attribute("kind", "target")];

        let record = expect_log_record_with_attrs(&records, "fact", &[("kind", "target")]);

        assert_eq!(log_attr(record, "kind"), Some("target"));
    }

    #[test]
    fn typed_log_expectations_match_constructed_records() {
        let expected = log::conversation_completed("session-1", "conversation-1", log::CompletionReason::Stop);
        let records = vec![expected.clone()];

        let record = expect_log(&records, expected);

        assert_eq!(log_attr(record, "completion_reason"), Some("stop"));
    }

    #[test]
    fn otlp_proto_contract_types_are_available_for_test_collectors() {
        use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
        use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;

        let metrics = ExportMetricsServiceRequest::default();
        let logs = ExportLogsServiceRequest::default();

        assert!(metrics.resource_metrics.is_empty());
        assert!(logs.resource_logs.is_empty());
    }
}
