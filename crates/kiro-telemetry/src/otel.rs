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
use crate::config::otel_export_interval_from_env;
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
        .with_interval(otel_export_interval_from_env())
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
        if let Some(instrument) = instruments.get(name) {
            return match instrument {
                OtelInstrument::Counter(counter) => Ok(counter.clone()),
                _ => Err(instrument_kind_error(name)),
            };
        }

        let counter = self.meter.u64_counter(name.to_string()).build();
        instruments.insert(name.to_string(), OtelInstrument::Counter(counter.clone()));
        Ok(counter)
    }

    fn f64_counter(&self, name: &str) -> Result<Counter<f64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        if let Some(instrument) = instruments.get(name) {
            return match instrument {
                OtelInstrument::FloatCounter(counter) => Ok(counter.clone()),
                _ => Err(instrument_kind_error(name)),
            };
        }

        let counter = self.meter.f64_counter(name.to_string()).build();
        instruments.insert(name.to_string(), OtelInstrument::FloatCounter(counter.clone()));
        Ok(counter)
    }

    fn histogram(&self, name: &str) -> Result<Histogram<f64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        if let Some(instrument) = instruments.get(name) {
            return match instrument {
                OtelInstrument::Histogram(histogram) => Ok(histogram.clone()),
                _ => Err(instrument_kind_error(name)),
            };
        }

        let histogram = self.meter.f64_histogram(name.to_string()).build();
        instruments.insert(name.to_string(), OtelInstrument::Histogram(histogram.clone()));
        Ok(histogram)
    }

    fn gauge(&self, name: &str) -> Result<Gauge<f64>, TelemetryError> {
        let mut instruments = self.instruments.lock().expect("otel instrument cache mutex poisoned");
        if let Some(instrument) = instruments.get(name) {
            return match instrument {
                OtelInstrument::Gauge(gauge) => Ok(gauge.clone()),
                _ => Err(instrument_kind_error(name)),
            };
        }

        let gauge = self.meter.f64_gauge(name.to_string()).build();
        instruments.insert(name.to_string(), OtelInstrument::Gauge(gauge.clone()));
        Ok(gauge)
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
    use opentelemetry::KeyValue;
    use opentelemetry::logs::LoggerProvider as _;
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::logs::{
        LogProcessor,
        SdkLogRecord,
        SdkLoggerProvider,
    };

    use super::*;
    use crate::testing::{
        OtlpTestCollector,
        expect_otlp_log,
        expect_otlp_log_resource_attribute,
        expect_otlp_metric,
        expect_otlp_metric_resource_attribute,
        expect_otlp_request,
    };
    use crate::{
        OtelMode,
        TelemetryConfig,
        log,
        metric,
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
            .emit(metric::cli_session_completed(
                metric::ExitReason::Clean,
                metric::AgentKind::V2,
            ))
            .expect("counter emit should succeed");
        client
            .emit(metric::estimated_cost_usd(
                0.00042,
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                false,
            ))
            .expect("float counter emit should succeed");
        client
            .emit(metric::bedrock_stream_ttft(
                0.25,
                metric::ModelClass::AnthropicSonnet,
                metric::PromptSizeBucket::Small,
                false,
            ))
            .expect("histogram emit should succeed");
        client
            .emit(metric::meta_meter_up(metric::Partition::Aws, metric::OsType::Macos))
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
                log::subagent_invoked("code-review")
                    .model_class(Some(metric::ModelClass::AnthropicSonnet))
                    .build(),
            )
            .expect("log emit should succeed");

        let logs = records.lock().expect("capture log mutex poisoned");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_name(), Some("kiro_cli_subagent_invoked"));
        assert!(sdk_log_has_string_attr(&logs[0], "model_class", "anthropic_sonnet"));
    }

    #[test]
    fn otel_logs_sink_buckets_raw_legacy_log_dimensions() {
        let config = TelemetryConfig {
            enabled: true,
            otel_mode: OtelMode::DualWrite,
            otlp_endpoint: None,
            state_dir: std::env::temp_dir(),
        };
        let processor = CaptureLogProcessor::default();
        let records = processor.records.clone();
        let logger_provider = SdkLoggerProvider::builder().with_log_processor(processor).build();

        let sink = std::sync::Arc::new(OtelLogsSink::new(
            logger_provider.logger("kiro-telemetry-test-raw-logs"),
        ));
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
        let collector = OtlpTestCollector::start(2);
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

        let expected_metric = metric::cli_session_completed(metric::ExitReason::Clean, metric::AgentKind::V2);
        let expected_log = log::subagent_invoked("review")
            .model_class(Some(metric::ModelClass::AnthropicSonnet))
            .build();

        client
            .emit(expected_metric.clone())
            .expect("metric emit should succeed");
        client.emit_log(expected_log.clone()).expect("log emit should succeed");

        providers.force_flush().expect("otlp provider flush should succeed");
        let requests = collector.collect();
        providers.shutdown().expect("otlp provider shutdown should succeed");

        let metrics = expect_otlp_request(&requests, "/v1/metrics");
        assert_eq!(metrics.content_type.as_deref(), Some("application/x-protobuf"));
        expect_otlp_metric_resource_attribute(
            &requests,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_DIMENSION,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_VALUE,
        );
        expect_otlp_metric(&requests, &expected_metric);

        let logs = expect_otlp_request(&requests, "/v1/logs");
        assert_eq!(logs.content_type.as_deref(), Some("application/x-protobuf"));
        expect_otlp_log_resource_attribute(
            &requests,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_DIMENSION,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_VALUE,
        );
        expect_otlp_log(&requests, &expected_log);
    }

    #[derive(Debug, Default, Clone)]
    struct CaptureLogProcessor {
        records: std::sync::Arc<std::sync::Mutex<Vec<SdkLogRecord>>>,
    }

    fn sdk_log_has_string_attr(record: &SdkLogRecord, expected_key: &str, expected_value: &str) -> bool {
        record.attributes_iter().any(|(key, value)| {
            matches!(
                (key.as_str(), value),
                (actual_key, AnyValue::String(value)) if actual_key == expected_key && value.as_str() == expected_value
            )
        })
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
