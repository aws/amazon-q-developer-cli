use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{
    Duration,
    UNIX_EPOCH,
};

use opentelemetry::logs::{
    AnyValue,
    LogRecord as _,
    Logger as _,
    LoggerProvider as _,
    Severity,
};
use opentelemetry::metrics::{
    Counter,
    Gauge,
    Histogram,
    Meter,
};
use opentelemetry::{
    KeyValue,
    global,
};
use opentelemetry_otlp::{
    LogExporter,
    MetricExporter,
    Protocol,
    WithExportConfig,
};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::{
    SdkLogger,
    SdkLoggerProvider,
};
use opentelemetry_sdk::metrics::{
    PeriodicReader,
    SdkMeterProvider,
    Temporality,
};
use tracing::warn;

use crate::client::TelemetryError;
use crate::{
    Attribute,
    MetricRecord,
    MetricValue,
    TelemetryConfig,
    TelemetryLogRecord,
    TelemetrySink,
};

#[derive(Clone, Debug)]
pub struct OtelProviders {
    meter_provider: SdkMeterProvider,
    logger_provider: SdkLoggerProvider,
    pipeline_kind: OtelPipelineKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OtelPipelineKind {
    Noop,
    OtlpHttp,
}

impl OtelProviders {
    pub fn meter_provider(&self) -> &SdkMeterProvider {
        &self.meter_provider
    }

    pub fn logger_provider(&self) -> &SdkLoggerProvider {
        &self.logger_provider
    }

    pub fn pipeline_kind(&self) -> OtelPipelineKind {
        self.pipeline_kind
    }

    pub fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.meter_provider.force_flush()?;
        self.logger_provider.force_flush()
    }

    pub fn shutdown(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.meter_provider.shutdown()?;
        self.logger_provider.shutdown()
    }
}

pub fn init_otel(config: &TelemetryConfig) -> OtelProviders {
    if config.exports_enabled()
        && let Some(endpoint) = config.otlp_endpoint.as_deref()
    {
        match build_otlp_http_providers(endpoint) {
            Ok(providers) => return providers,
            Err(err) => warn!(%err, "failed to initialize OTLP telemetry exporter; falling back to no-op"),
        }
    }

    init_noop_otel(config)
}

pub fn init_noop_otel(_config: &TelemetryConfig) -> OtelProviders {
    let meter_provider = SdkMeterProvider::builder().build();
    let logger_provider = SdkLoggerProvider::builder().build();
    global::set_meter_provider(meter_provider.clone());
    OtelProviders {
        meter_provider,
        logger_provider,
        pipeline_kind: OtelPipelineKind::Noop,
    }
}

fn build_otlp_http_providers(endpoint: &str) -> Result<OtelProviders, opentelemetry_otlp::ExporterBuildError> {
    let resource = telemetry_resource();
    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(signal_endpoint(endpoint, "/v1/metrics"))
        .with_timeout(Duration::from_secs(30))
        .with_temporality(Temporality::Delta)
        .build()?;
    let log_exporter = LogExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(signal_endpoint(endpoint, "/v1/logs"))
        .with_timeout(Duration::from_secs(30))
        .build()?;

    let reader = PeriodicReader::builder(metric_exporter)
        .with_interval(Duration::from_secs(60))
        .build();
    let meter_provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource.clone())
        .build();
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(log_exporter)
        .with_resource(resource)
        .build();

    global::set_meter_provider(meter_provider.clone());
    Ok(OtelProviders {
        meter_provider,
        logger_provider,
        pipeline_kind: OtelPipelineKind::OtlpHttp,
    })
}

fn telemetry_resource() -> Resource {
    Resource::builder()
        .with_service_name("kiro-cli")
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .with_attribute(KeyValue::new("os.type", std::env::consts::OS))
        .with_attribute(KeyValue::new("host.arch", std::env::consts::ARCH))
        .with_attribute(KeyValue::new(
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_DIMENSION,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_VALUE,
        ))
        .with_attribute(KeyValue::new("partition", "aws"))
        .build()
}

fn signal_endpoint(endpoint: &str, signal_path: &str) -> String {
    let base = endpoint
        .trim_end_matches('/')
        .strip_suffix("/v1/metrics")
        .or_else(|| endpoint.trim_end_matches('/').strip_suffix("/v1/logs"))
        .unwrap_or_else(|| endpoint.trim_end_matches('/'));
    format!("{}/{}", base, signal_path.trim_start_matches('/'))
}

pub struct OtelMetricsSink {
    meter: Meter,
    instruments: Mutex<HashMap<String, OtelInstrument>>,
}

impl OtelMetricsSink {
    pub fn new(meter: Meter) -> Self {
        Self {
            meter,
            instruments: Mutex::new(HashMap::new()),
        }
    }

    fn counter(&self, name: &str) -> Result<Counter<u64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        match instruments.entry(name.to_string()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let counter = self.meter.u64_counter(name.to_string()).build();
                entry.insert(OtelInstrument::Counter(counter.clone()));
                Ok(counter)
            },
            std::collections::hash_map::Entry::Occupied(entry) => match entry.get() {
                OtelInstrument::Counter(counter) => Ok(counter.clone()),
                _ => Err(instrument_kind_error(name)),
            },
        }
    }

    fn f64_counter(&self, name: &str) -> Result<Counter<f64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        match instruments.entry(name.to_string()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let counter = self.meter.f64_counter(name.to_string()).build();
                entry.insert(OtelInstrument::FloatCounter(counter.clone()));
                Ok(counter)
            },
            std::collections::hash_map::Entry::Occupied(entry) => match entry.get() {
                OtelInstrument::FloatCounter(counter) => Ok(counter.clone()),
                _ => Err(instrument_kind_error(name)),
            },
        }
    }

    fn histogram(&self, name: &str) -> Result<Histogram<f64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        match instruments.entry(name.to_string()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let histogram = self.meter.f64_histogram(name.to_string()).build();
                entry.insert(OtelInstrument::Histogram(histogram.clone()));
                Ok(histogram)
            },
            std::collections::hash_map::Entry::Occupied(entry) => match entry.get() {
                OtelInstrument::Histogram(histogram) => Ok(histogram.clone()),
                _ => Err(instrument_kind_error(name)),
            },
        }
    }

    fn gauge(&self, name: &str) -> Result<Gauge<f64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        match instruments.entry(name.to_string()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                let gauge = self.meter.f64_gauge(name.to_string()).build();
                entry.insert(OtelInstrument::Gauge(gauge.clone()));
                Ok(gauge)
            },
            std::collections::hash_map::Entry::Occupied(entry) => match entry.get() {
                OtelInstrument::Gauge(gauge) => Ok(gauge.clone()),
                _ => Err(instrument_kind_error(name)),
            },
        }
    }
}

impl TelemetrySink for OtelMetricsSink {
    fn emit(&self, record: &MetricRecord) -> Result<(), TelemetryError> {
        let attributes = otel_attributes(&record.attributes);
        match record.value {
            MetricValue::Counter(value) => self.counter(&record.name)?.add(value, &attributes),
            MetricValue::FloatCounter(value) => self.f64_counter(&record.name)?.add(value, &attributes),
            MetricValue::Histogram(value) => self.histogram(&record.name)?.record(value, &attributes),
            MetricValue::Gauge(value) => self.gauge(&record.name)?.record(value, &attributes),
        }
        Ok(())
    }
}

pub struct OtelLogsSink {
    logger: SdkLogger,
}

impl OtelLogsSink {
    pub fn new(logger: SdkLogger) -> Self {
        Self { logger }
    }

    pub fn from_providers(providers: &OtelProviders) -> Self {
        Self::new(providers.logger_provider().logger("kiro-telemetry"))
    }
}

impl TelemetrySink for OtelLogsSink {
    fn emit(&self, _record: &MetricRecord) -> Result<(), TelemetryError> {
        Ok(())
    }

    fn emit_log(&self, record: &TelemetryLogRecord) -> Result<(), TelemetryError> {
        let mut otel_record = self.logger.create_log_record();
        otel_record.set_event_name(static_log_event_name(&record.name)?);
        otel_record.set_target("kiro-telemetry");
        otel_record.set_timestamp(UNIX_EPOCH + Duration::from_millis(record.timestamp_unix_millis));
        otel_record.set_severity_number(Severity::Info);
        otel_record.set_severity_text("INFO");
        otel_record.set_body(AnyValue::String(record.name.clone().into()));
        otel_record.add_attributes(otel_log_attributes(&record.attributes));
        self.logger.emit(otel_record);
        Ok(())
    }
}

enum OtelInstrument {
    Counter(Counter<u64>),
    FloatCounter(Counter<f64>),
    Histogram(Histogram<f64>),
    Gauge(Gauge<f64>),
}

fn otel_attributes(attributes: &[Attribute]) -> Vec<KeyValue> {
    attributes
        .iter()
        .map(|attribute| KeyValue::new(attribute.key.clone(), attribute.value.clone()))
        .collect()
}

fn otel_log_attributes(attributes: &[Attribute]) -> Vec<(String, AnyValue)> {
    attributes
        .iter()
        .map(|attribute| (attribute.key.clone(), AnyValue::String(attribute.value.clone().into())))
        .collect()
}

fn instrument_kind_error(name: &str) -> TelemetryError {
    TelemetryError::Sink(format!("metric `{name}` reused with different OTel instrument kind"))
}

fn static_log_event_name(name: &str) -> Result<&'static str, TelemetryError> {
    match name {
        "kiro_cli_client_identity" => Ok("kiro_cli_client_identity"),
        "kiro_cli_feature_first_use" => Ok("kiro_cli_feature_first_use"),
        "kiro_cli_metering_event" => Ok("kiro_cli_metering_event"),
        "kiro_cli_user_turn_completed" => Ok("kiro_cli_user_turn_completed"),
        "kiro_cli_tool_invoked" => Ok("kiro_cli_tool_invoked"),
        "kiro_cli_mcp_server_init" => Ok("kiro_cli_mcp_server_init"),
        "kiro_cli_subagent_invoked" => Ok("kiro_cli_subagent_invoked"),
        "kiro_cli_conversation_completed" => Ok("kiro_cli_conversation_completed"),
        _ => Err(TelemetryError::Sink(format!(
            "metric `{name}` is not a known OTel log event"
        ))),
    }
}

#[cfg(test)]
mod tests {
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
    use std::sync::mpsc;
    use std::time::Instant;

    use opentelemetry::KeyValue;
    use opentelemetry::logs::LoggerProvider as _;
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::logs::{
        LogProcessor,
        SdkLogRecord,
        SdkLoggerProvider,
    };
    use prost::Message as _;

    use super::*;
    use crate::{
        OtelMode,
        TelemetryConfig,
    };

    #[test]
    fn noop_otel_provider_accepts_counter_adds() {
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::Off,
            otlp_endpoint: None,
            state_dir: std::env::temp_dir(),
        };

        let providers = init_noop_otel(&config);
        let counter = global::meter("kiro-telemetry-test")
            .u64_counter("telemetry.sdk.up")
            .build();

        counter.add(1, &[KeyValue::new("partition", "aws")]);

        providers.force_flush().expect("noop provider flush should succeed");
    }

    #[test]
    fn init_otel_stays_noop_without_endpoint() {
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::DualWrite,
            otlp_endpoint: None,
            state_dir: std::env::temp_dir(),
        };

        let providers = init_otel(&config);

        assert_eq!(providers.pipeline_kind(), OtelPipelineKind::Noop);
        providers.shutdown().expect("noop provider shutdown should succeed");
    }

    #[test]
    fn init_otel_builds_otlp_http_when_endpoint_configured() {
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::DualWrite,
            otlp_endpoint: Some("http://localhost:4318".to_string()),
            state_dir: std::env::temp_dir(),
        };

        let providers = init_otel(&config);

        assert_eq!(providers.pipeline_kind(), OtelPipelineKind::OtlpHttp);
        providers.shutdown().expect("otlp provider shutdown should succeed");
    }

    #[test]
    fn signal_endpoint_normalizes_base_endpoint() {
        assert_eq!(
            signal_endpoint("https://otel.example.test/", "/v1/metrics"),
            "https://otel.example.test/v1/metrics"
        );
        assert_eq!(
            signal_endpoint("https://otel.example.test/v1/logs", "/v1/metrics"),
            "https://otel.example.test/v1/metrics"
        );
    }

    #[test]
    fn otel_metrics_sink_accepts_metric_records() {
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::DualWrite,
            otlp_endpoint: None,
            state_dir: std::env::temp_dir(),
        };

        let providers = init_noop_otel(&config);
        let sink = std::sync::Arc::new(OtelMetricsSink::new(global::meter("kiro-telemetry-test-sink")));
        let client = crate::TelemetryClient::new(config).with_sink(sink);

        client
            .emit(crate::MetricRecord::counter("chat_cli.session.completed", 1))
            .expect("counter emit should succeed");
        client
            .emit(
                crate::MetricRecord::counter_f64("kiro_cli_estimated_cost_usd", 0.00042)
                    .with_attribute("model_class", "anthropic_sonnet")
                    .with_attribute("client_application", "chat_cli_v2")
                    .with_attribute("is_subagent", "false"),
            )
            .expect("float counter emit should succeed");
        client
            .emit(crate::MetricRecord::histogram("chat_cli.bedrock.stream.ttft", 0.25))
            .expect("histogram emit should succeed");
        client
            .emit(
                crate::MetricRecord::gauge("meta_meter.up", 1.0)
                    .with_attribute("partition", "aws")
                    .with_attribute("os_type", "macos"),
            )
            .expect("gauge emit should succeed");

        providers.force_flush().expect("noop provider flush should succeed");
    }

    #[test]
    fn otel_logs_sink_accepts_log_records() {
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::DualWrite,
            otlp_endpoint: None,
            state_dir: std::env::temp_dir(),
        };
        let processor = CaptureLogProcessor::default();
        let records = processor.records.clone();
        let logger_provider = SdkLoggerProvider::builder().with_log_processor(processor).build();

        let sink = std::sync::Arc::new(OtelLogsSink::new(logger_provider.logger("kiro-telemetry-test-logs")));
        let client = crate::TelemetryClient::new(config).with_sink(sink);

        client
            .emit_log(
                crate::TelemetryLogRecord::new("kiro_cli_subagent_invoked")
                    .with_attribute("subagent_name", "code-review")
                    .with_attribute("model_class", "raw-model-id"),
            )
            .expect("log emit should succeed");

        // Closed-enum overflow bucketing (substituting unknown values with `_other_`) now
        // happens server-side in the OTel collector via `transform`/`filter` processors.
        // The client passes the raw attribute value through after schema validation accepts
        // the record (a `_other_` bucket is permitted on this attribute, so the validator
        // does not reject the unknown value).
        let logs = records.lock().expect("capture log mutex poisoned");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_name(), Some("kiro_cli_subagent_invoked"));
        assert!(logs[0].attributes_iter().any(|(key, value)| {
            key.as_str() == "model_class" && value == &AnyValue::String("raw-model-id".into())
        }));
    }

    #[test]
    fn otlp_proto_contract_types_are_available_for_mock_collector() {
        use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
        use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;

        let metrics = ExportMetricsServiceRequest::default();
        let logs = ExportLogsServiceRequest::default();

        assert!(metrics.resource_metrics.is_empty());
        assert!(logs.resource_logs.is_empty());
    }

    #[test]
    fn otlp_http_exporter_sends_decodable_metric_and_log_payloads() {
        use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
        use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;

        let collector = MockOtlpCollector::start(2);
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::DualWrite,
            otlp_endpoint: Some(collector.endpoint()),
            state_dir: tempfile::tempdir().expect("tempdir").path().to_path_buf(),
        };
        let providers = init_otel(&config);
        assert_eq!(providers.pipeline_kind(), OtelPipelineKind::OtlpHttp);

        let client = crate::TelemetryClient::new(config)
            .with_sink(std::sync::Arc::new(OtelMetricsSink::new(
                providers.meter_provider().meter("kiro-telemetry-mock-collector-test"),
            )))
            .with_sink(std::sync::Arc::new(OtelLogsSink::from_providers(&providers)));

        client
            .emit(
                crate::MetricRecord::counter("chat_cli.session.completed", 1)
                    .with_attribute("exit_reason", "user_exit")
                    .with_attribute("agent_kind", "interactive"),
            )
            .expect("metric emit should succeed");
        client
            .emit_log(
                crate::TelemetryLogRecord::new("kiro_cli_subagent_invoked")
                    .with_attribute("subagent_name", "review")
                    .with_attribute("model_class", "anthropic_sonnet"),
            )
            .expect("log emit should succeed");

        providers.force_flush().expect("otlp provider flush should succeed");
        let requests = collector.collect();
        providers.shutdown().expect("otlp provider shutdown should succeed");

        let metrics = requests
            .iter()
            .find(|request| request.path == "/v1/metrics")
            .expect("metrics request");
        assert_eq!(metrics.content_type.as_deref(), Some("application/x-protobuf"));
        let metrics = ExportMetricsServiceRequest::decode(metrics.body.as_slice()).expect("decode metrics request");
        assert_metric_resource_attribute(
            &metrics,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_DIMENSION,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_VALUE,
        );
        let metric_names = exported_metric_names(&metrics);
        assert!(
            metric_names.iter().any(|name| *name == "chat_cli.session.completed"),
            "missing metric in OTLP payload: {metric_names:?}"
        );

        let logs = requests
            .iter()
            .find(|request| request.path == "/v1/logs")
            .expect("logs request");
        assert_eq!(logs.content_type.as_deref(), Some("application/x-protobuf"));
        let logs = ExportLogsServiceRequest::decode(logs.body.as_slice()).expect("decode logs request");
        assert_log_resource_attribute(
            &logs,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_DIMENSION,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_VALUE,
        );
        let log_event_names = exported_log_event_names(&logs);
        assert!(
            log_event_names.iter().any(|name| *name == "kiro_cli_subagent_invoked"),
            "missing log event in OTLP payload: {log_event_names:?}"
        );
    }

    fn exported_metric_names(
        request: &opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest,
    ) -> Vec<String> {
        request
            .resource_metrics
            .iter()
            .flat_map(|resource_metrics| &resource_metrics.scope_metrics)
            .flat_map(|scope_metrics| &scope_metrics.metrics)
            .map(|metric| metric.name.clone())
            .collect()
    }

    fn exported_log_event_names(
        request: &opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest,
    ) -> Vec<String> {
        request
            .resource_logs
            .iter()
            .flat_map(|resource_logs| &resource_logs.scope_logs)
            .flat_map(|scope_logs| &scope_logs.log_records)
            .map(|record| record.event_name.clone())
            .collect()
    }

    fn assert_metric_resource_attribute(
        request: &opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest,
        key: &str,
        expected: &str,
    ) {
        assert!(
            request
                .resource_metrics
                .iter()
                .any(|resource_metrics| { resource_has_attribute(resource_metrics.resource.as_ref(), key, expected) }),
            "missing metric resource attribute {key}={expected}"
        );
    }

    fn assert_log_resource_attribute(
        request: &opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest,
        key: &str,
        expected: &str,
    ) {
        assert!(
            request
                .resource_logs
                .iter()
                .any(|resource_logs| { resource_has_attribute(resource_logs.resource.as_ref(), key, expected) }),
            "missing log resource attribute {key}={expected}"
        );
    }

    fn resource_has_attribute(
        resource: Option<&opentelemetry_proto::tonic::resource::v1::Resource>,
        key: &str,
        expected: &str,
    ) -> bool {
        use opentelemetry_proto::tonic::common::v1::any_value::Value;

        resource.is_some_and(|resource| {
            resource.attributes.iter().any(|attribute| {
                attribute.key == key
                    && matches!(
                        attribute.value.as_ref().and_then(|value| value.value.as_ref()),
                        Some(Value::StringValue(value)) if value == expected
                    )
            })
        })
    }

    #[derive(Debug)]
    struct CapturedRequest {
        path: String,
        content_type: Option<String>,
        body: Vec<u8>,
    }

    struct MockOtlpCollector {
        endpoint: String,
        expected_requests: usize,
        rx: mpsc::Receiver<CapturedRequest>,
    }

    impl MockOtlpCollector {
        fn start(expected_requests: usize) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock OTLP collector");
            listener.set_nonblocking(true).expect("set mock collector nonblocking");
            let endpoint = format!("http://{}", listener.local_addr().expect("collector local addr"));
            let (tx, rx) = mpsc::channel();

            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut accepted = 0;
                while accepted < expected_requests && Instant::now() < deadline {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if let Ok(request) = read_http_request(stream) {
                                let _ = tx.send(request);
                                accepted += 1;
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

        fn endpoint(&self) -> String {
            self.endpoint.clone()
        }

        fn collect(self) -> Vec<CapturedRequest> {
            (0..self.expected_requests)
                .map(|_| {
                    self.rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("mock collector request")
                })
                .collect()
        }
    }

    fn read_http_request(mut stream: TcpStream) -> std::io::Result<CapturedRequest> {
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

        stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")?;
        stream.flush()?;

        Ok(CapturedRequest {
            path,
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

    #[derive(Debug, Default, Clone)]
    struct CaptureLogProcessor {
        records: std::sync::Arc<std::sync::Mutex<Vec<SdkLogRecord>>>,
    }

    impl LogProcessor for CaptureLogProcessor {
        fn emit(&self, record: &mut SdkLogRecord, _instrumentation: &opentelemetry::InstrumentationScope) {
            self.records
                .lock()
                .expect("capture log mutex poisoned")
                .push(record.clone());
        }

        fn force_flush(&self) -> OTelSdkResult {
            Ok(())
        }
    }
}
