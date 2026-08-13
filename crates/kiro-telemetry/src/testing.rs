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

pub fn catalog_metric_records() -> Vec<MetricRecord> {
    use crate::metric::*;

    let os = OsType::Macos;
    let engine = Engine::V3;
    let interface = SessionInterface::InteractiveCli;
    let mode = AgentMode::Default;
    let role = ProcessRole::Host;
    let model = Some("claude-sonnet-4");
    let tool = ToolMetric::new(
        engine,
        ToolMetricOrigin::Builtin,
        ToolMetricOutcome::Success,
        ExecutionContext::Main,
    )
    .builtin_tool_name(Some("fs_read"));
    let workflow_dimensions = |builder: MetricBuilder| {
        builder
            .attribute("version_full", version_attr())
            .attribute("agent_engine", engine.as_str())
    };

    vec![
        record_run_started(interface, engine, os),
        record_login_success(AuthMethod::BuilderId, AuthFlow::Pkce),
        record_chat_session_started(interface, mode, engine),
        record_cloud_session_lifecycle(CloudSessionEvent::Started),
        record_cloud_session_ready(1.0).expect("positive duration"),
        record_autonomous_mode("enabled", engine),
        record_cloud_repo_attach("submitted", "2", engine),
        record_cloud_error("session_new", "version_skew", engine),
        record_cloud_attach("image", "under_1m", engine),
        record_config_panel("menu", engine),
        record_cloud_config_diagnostic("warning", engine),
        record_cloud_config_source("mcp", engine),
        workflow_dimensions(counter("kiro_cli_workflow_run_total", 1))
            .attribute("workflow_run_event", "started")
            .attribute("workflow_topology", "mixed")
            .attribute("workflow_step_bucket", "6_10")
            .expect_valid(),
        workflow_dimensions(histogram("kiro_cli_workflow_run_duration_seconds", 45.0))
            .attribute("workflow_outcome", "completed")
            .attribute("workflow_topology", "mixed")
            .attribute("workflow_step_bucket", "6_10")
            .expect_valid(),
        workflow_dimensions(counter("kiro_cli_workflow_node_total", 1))
            .attribute("workflow_node_type", "repeat")
            .attribute("workflow_node_outcome", "failed")
            .expect_valid(),
        workflow_dimensions(histogram("kiro_cli_workflow_node_duration_seconds", 7.0))
            .attribute("workflow_node_type", "repeat")
            .attribute("workflow_node_outcome", "failed")
            .expect_valid(),
        workflow_dimensions(counter("kiro_cli_workflow_control_total", 1))
            .attribute("workflow_control_action", "pause")
            .attribute("workflow_control_result", "success")
            .expect_valid(),
        workflow_dimensions(counter("kiro_cli_workflow_restore_total", 1))
            .attribute("workflow_restore_result", "restored")
            .expect_valid(),
        workflow_dimensions(gauge("kiro_cli_workflow_concurrent_runs", 2.0)).expect_valid(),
        record_ui_mode_session_started(UiMode::Tui),
        record_daily_heartbeat(ReleaseChannel::Stable, os, InstallSource::Internal),
        record_slash_command("/help", engine),
        record_top_level_command("chat"),
        record_tool_call(tool),
        record_model_invocation(engine, model),
        record_model_time_to_first_content_ms(250.0, engine, model).expect("positive duration"),
        record_time_to_first_visible_response_ms(300.0, interface, mode, engine).expect("positive duration"),
        record_model_request_duration_seconds(1.5, engine, model, ModelRequestOutcome::Success)
            .expect("positive duration"),
        record_user_turn_duration_seconds(2.0, interface, mode, engine).expect("positive duration"),
        record_run_outcome(interface, engine, os, RunOutcome::Success),
        record_crash(engine, os, role, CrashKind::Panic),
        record_startup_duration_seconds(0.5, interface, engine, os).expect("positive duration"),
        record_startup_failure(interface, engine, os, StartupFailureStage::AgentLaunch),
        record_model_request_failure(engine, model, ErrorKind::Throttling),
        record_automatic_retries(2, engine, RetryReason::Throttled, RetryOutcome::Recovered)
            .expect("retried operation"),
        record_process_memory_rss_bytes(128.0 * 1024.0 * 1024.0, os, engine, role).expect("non-negative value"),
        record_process_cpu_utilization_ratio(0.25, os, engine, role).expect("non-negative value"),
        record_process_open_file_descriptor_count(64.0, os, engine, role).expect("non-negative value"),
        record_process_handle_count(64.0, engine, role).expect("non-negative value"),
        record_process_thread_count(12.0, os, engine, role).expect("non-negative value"),
        record_tokens_consumed(1200, engine, model, TokenType::InputUncached).expect("positive token count"),
        record_credits_consumed(1.5, model).expect("positive credit count"),
        record_tool_execution_duration_ms(42.0, tool).expect("positive duration"),
        record_mcp_server_init(
            engine,
            McpServerSource::Global,
            McpInitOutcome::Failure,
            Some("postgres"),
            Some(McpErrorKind::Connection),
            Some(McpFailureStage::Connection),
        ),
        record_user_turn(interface, mode, engine),
        record_goal_outcome(engine, GoalOutcome::Completed),
        record_prohibited_telemetry_channel_enabled(TelemetryChannelName::LegacyToolkit),
        record_telemetry_export_dropped(1, ExportDropReason::PermanentRejection).expect("positive drop count"),
        record_turn_failure(interface, mode, engine, TurnFailureReason::ModelError),
        record_turn_cancelled(interface, mode, engine),
        record_process_peak_rss_bytes(256.0 * 1024.0 * 1024.0, os, engine, role).expect("non-negative value"),
        record_tui_heap_used_bytes(96.0 * 1024.0 * 1024.0, os, engine).expect("non-negative value"),
        record_tui_event_loop_delay_p99_seconds(0.012, os, engine).expect("non-negative value"),
        record_tui_input_to_render_p95_seconds(0.008, os, engine).expect("non-negative value"),
        record_tui_render_duration_seconds(0.004, os, engine, RenderKind::Full).expect("non-negative value"),
        record_auth_failure(
            AuthMethod::BuilderId,
            AuthFlow::Pkce,
            AuthOperation::Login,
            AuthFailureReason::AuthorizationDenied,
        ),
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
            let fallback_status = statuses.last().copied().unwrap_or(200);
            let mut statuses = statuses.into_iter();
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let status = statuses.next().unwrap_or(fallback_status);
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
        let harness = in_memory_telemetry(TelemetryConfig::new(
            true,
            crate::OtelMode::DualWrite,
            None,
            tempdir.path().to_path_buf(),
        ));

        harness
            .client
            .emit(metric::record_model_invocation(
                metric::Engine::V2,
                Some("claude-sonnet-4"),
            ))
            .expect("metric should emit");

        expect_metric(
            &harness.sink.records(),
            metric::record_model_invocation(metric::Engine::V2, Some("claude-sonnet-4")),
        );
    }

    #[test]
    fn typed_metric_expectations_match_constructed_records() {
        let expected = metric::record_chat_session_started(
            metric::SessionInterface::InteractiveCli,
            metric::AgentMode::Plan,
            metric::Engine::V2,
        );
        let records = vec![
            metric::record_chat_session_started(
                metric::SessionInterface::InteractiveCli,
                metric::AgentMode::Default,
                metric::Engine::V2,
            ),
            expected.clone(),
        ];

        let record = expect_metric(&records, expected);

        assert_eq!(metric_attr(record, "agent_mode"), Some("plan"));
    }

    #[test]
    fn metric_value_expectation_accepts_matching_kind() {
        let record = MetricRecord::histogram("duration_seconds", 0.25);

        expect_metric_value(&record, MetricValue::Histogram(0.25));
    }

    #[test]
    fn otlp_proto_contract_types_are_available_for_test_collectors() {
        use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;

        let metrics = ExportMetricsServiceRequest::default();

        assert!(metrics.resource_metrics.is_empty());
    }
}
