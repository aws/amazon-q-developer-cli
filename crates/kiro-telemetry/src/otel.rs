use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;
use std::{
    fmt,
    thread,
};

use async_trait::async_trait;
use opentelemetry::metrics::{
    Counter,
    Gauge,
    Histogram,
    Meter,
    MeterProvider,
};
use opentelemetry::{
    KeyValue,
    global,
};
use opentelemetry_http::{
    Bytes,
    HttpClient,
    HttpError,
    Request,
    Response,
};
use opentelemetry_otlp::{
    MetricExporter,
    Protocol,
    WithExportConfig,
    WithHttpConfig,
};
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::metrics::v1::metric::Data;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::{
    Aggregation,
    Instrument,
    InstrumentKind,
    PeriodicReader,
    SdkMeterProvider,
    Stream,
    Temporality,
};
use prost::Message;
use tracing::{
    trace,
    warn,
};

use crate::client::TelemetryError;
use crate::config::otel_export_interval_from_env;
use crate::drop_store::{
    ExportDropAggregate,
    ExportDropStore,
};
use crate::{
    Attribute,
    MetricRecord,
    MetricValue,
    TelemetryConfig,
    TelemetrySink,
};

const KIRO_MACHINE_ID_HEADER: &str = "x-kiro-machineid";
const KUTS_MAX_REQUEST_BODY_BYTES: usize = 1_048_576;
const KUTS_MAX_EXPORT_RETRIES: usize = 3;
const KUTS_RETRY_INITIAL_DELAY: Duration = Duration::from_secs(1);
const KUTS_RETRY_MAX_DELAY: Duration = Duration::from_secs(60);
const TELEMETRY_EXPORT_DROPPED_METRIC: &str = "kiro_cli_telemetry_export_dropped_total";

/// Explicit histogram bucket boundaries for latencies recorded in **seconds**.
///
/// The OTel Rust SDK's default explicit-bucket boundaries are tuned for
/// milliseconds (`…, 1000, 2500, 5000, 7500, 10000`). Applying them to a
/// second-scale latency squashes every realistic value into the first bucket,
/// so these instruments get a dedicated second-scale ladder instead.
const SECONDS_LATENCY_BOUNDARIES: &[f64] = &[0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0];

/// Explicit boundaries for unit-interval ratios (`0.0..=1.0`), e.g. cache-hit
/// ratio and CPU utilisation. Default ms buckets run to 10000, so a ratio of
/// `0.4` lands in the same bucket as everything `>= 0.0`.
const RATIO_BOUNDARIES: &[f64] = &[0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];

const RETRY_COUNT_BOUNDARIES: &[f64] = &[1.0, 2.0, 3.0, 5.0, 8.0, 13.0];
const MEMORY_BYTES_BOUNDARIES: &[f64] = &[
    67_108_864.0,
    134_217_728.0,
    268_435_456.0,
    536_870_912.0,
    1_073_741_824.0,
    2_147_483_648.0,
    4_294_967_296.0,
    8_589_934_592.0,
];

/// Maps a histogram instrument name to its explicit bucket boundaries.
///
/// Returns `None` for histograms whose values are genuinely milliseconds-scale
/// (e.g. `*_duration_ms`); those keep the SDK's default ms buckets, which are
/// already correct for them.
fn histogram_boundaries(name: &str) -> Option<&'static [f64]> {
    match name {
        // Latencies recorded in seconds.
        "kiro_cli_cloud_session_ready_seconds"
        | "kiro_cli_model_request_duration_seconds"
        | "kiro_cli_user_turn_duration_seconds"
        | "kiro_cli_startup_duration_seconds"
        | "kiro_cli_tui_event_loop_delay_p99_seconds"
        | "kiro_cli_tui_input_to_render_p95_seconds"
        | "kiro_cli_tui_render_duration_seconds" => Some(SECONDS_LATENCY_BOUNDARIES),
        // Unit-interval ratios (0..=1).
        "kiro_cli_process_cpu_utilization_ratio" => Some(RATIO_BOUNDARIES),
        "kiro_cli_automatic_retries_per_operation" => Some(RETRY_COUNT_BOUNDARIES),
        "kiro_cli_process_peak_rss_bytes" => Some(MEMORY_BYTES_BOUNDARIES),
        _ => None,
    }
}

/// A [`View`](opentelemetry_sdk::metrics::View) that installs explicit
/// histogram bucket boundaries for the non-millisecond instruments listed in
/// [`histogram_boundaries`]. Registering this on the `SdkMeterProvider` is the
/// OTel-idiomatic way to override default aggregation per instrument name,
/// keeping the generic name-keyed instrument cache in [`OtelMetricsSink`]
/// boundary-agnostic.
fn histogram_bucket_view(instrument: &Instrument) -> Option<Stream> {
    if instrument.kind() != InstrumentKind::Histogram {
        return None;
    }

    let boundaries = histogram_boundaries(instrument.name())?;
    Stream::builder()
        .with_aggregation(Aggregation::ExplicitBucketHistogram {
            boundaries: boundaries.to_vec(),
            record_min_max: true,
        })
        .build()
        .ok()
}

#[derive(Clone, Debug)]
struct KutsHttpClient {
    inner: reqwest::blocking::Client,
    retry_policy: KutsRetryPolicy,
    drop_store: Option<ExportDropStore>,
}

#[derive(Clone, Copy, Debug)]
struct KutsRetryPolicy {
    max_retries: usize,
    initial_delay: Duration,
    max_delay: Duration,
}

impl Default for KutsRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: KUTS_MAX_EXPORT_RETRIES,
            initial_delay: KUTS_RETRY_INITIAL_DELAY,
            max_delay: KUTS_RETRY_MAX_DELAY,
        }
    }
}

impl KutsHttpClient {
    fn new(drop_store: Option<ExportDropStore>) -> Self {
        // `reqwest::blocking::Client::builder().build()` synchronously waits on
        // its own internal tokio runtime, so dropping it inside an outer tokio
        // runtime panics ("Cannot drop a runtime in a context where blocking is
        // not allowed"). Build it on a fresh thread that has no runtime in its
        // thread-local storage.
        let inner = std::thread::spawn(|| {
            reqwest::blocking::Client::builder()
                .http1_only()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|err| {
                    warn!(%err, "failed to build OTLP HTTP client; using reqwest defaults");
                    reqwest::blocking::Client::new()
                })
        })
        .join()
        .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            inner,
            retry_policy: KutsRetryPolicy::default(),
            drop_store,
        }
    }

    #[cfg(test)]
    fn with_retry_policy(retry_policy: KutsRetryPolicy) -> Self {
        Self {
            retry_policy,
            ..Self::new(None)
        }
    }

    fn send_once(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        let reqwest_request: reqwest::blocking::Request = request.try_into()?;
        let mut response = self.inner.execute(reqwest_request)?;
        let status = response.status();
        let headers = std::mem::take(response.headers_mut());
        let mut http_response = Response::builder().status(status.as_u16()).body(response.bytes()?)?;
        *http_response.headers_mut() = headers;
        Ok(http_response)
    }
}

#[async_trait]
impl HttpClient for KutsHttpClient {
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        let body_len = request.body().len();
        let point_count = droppable_metric_point_count(request.body()).unwrap_or(1);
        if body_len > KUTS_MAX_REQUEST_BODY_BYTES {
            self.record_drop(point_count, crate::metric::ExportDropReason::Oversize);
            return Err(kuts_http_error(format!(
                "OTLP metrics payload is {body_len} bytes; KUTS limit is {KUTS_MAX_REQUEST_BODY_BYTES} bytes"
            )));
        }

        let mut retries = 0;
        let mut backoff = self.retry_policy.initial_delay;
        loop {
            let response = match self.send_once(request.clone()) {
                Ok(response) => response,
                Err(_) if retries < self.retry_policy.max_retries => {
                    if !backoff.is_zero() {
                        thread::sleep(backoff);
                    }
                    backoff = next_backoff(backoff, self.retry_policy.max_delay);
                    retries += 1;
                    continue;
                },
                Err(err) => {
                    self.record_drop(point_count, crate::metric::ExportDropReason::RetryExhausted);
                    return Err(err);
                },
            };
            let status = response.status().as_u16();
            if is_retryable_status(status) && retries >= self.retry_policy.max_retries {
                self.record_drop(point_count, crate::metric::ExportDropReason::RetryExhausted);
                return Ok(response);
            }
            if !is_retryable_status(status) {
                if (400..=499).contains(&status) {
                    self.record_drop(point_count, crate::metric::ExportDropReason::PermanentRejection);
                }
                return Ok(response);
            }

            let delay = retry_delay(&response, backoff, self.retry_policy.max_delay);
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            backoff = next_backoff(backoff, self.retry_policy.max_delay);
            retries += 1;
        }
    }
}

impl KutsHttpClient {
    fn record_drop(&self, count: u64, reason: crate::metric::ExportDropReason) {
        if let Some(store) = &self.drop_store {
            store.record("metrics", reason.as_str(), count);
        }
    }
}

#[derive(Debug)]
struct KutsHttpError(String);

impl fmt::Display for KutsHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KutsHttpError {}

fn kuts_http_error(message: String) -> HttpError {
    Box::new(KutsHttpError(message))
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn retry_delay(response: &Response<Bytes>, fallback: Duration, max_delay: Duration) -> Duration {
    response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(fallback, Duration::from_secs)
        .min(max_delay)
}

fn next_backoff(current: Duration, max_delay: Duration) -> Duration {
    current.saturating_mul(2).min(max_delay)
}

fn droppable_metric_point_count(body: &Bytes) -> Option<u64> {
    let request = ExportMetricsServiceRequest::decode(body.as_ref()).ok()?;
    Some(
        request
            .resource_metrics
            .iter()
            .flat_map(|resource| &resource.scope_metrics)
            .flat_map(|scope| &scope.metrics)
            .filter(|metric| metric.name != TELEMETRY_EXPORT_DROPPED_METRIC)
            .map(|metric| match metric.data.as_ref() {
                Some(Data::Gauge(data)) => data.data_points.len(),
                Some(Data::Sum(data)) => data.data_points.len(),
                Some(Data::Histogram(data)) => data.data_points.len(),
                Some(Data::ExponentialHistogram(data)) => data.data_points.len(),
                Some(Data::Summary(data)) => data.data_points.len(),
                None => 0,
            })
            .sum::<usize>() as u64,
    )
}

#[derive(Clone, Debug)]
pub struct OtelProviders {
    meter_provider: SdkMeterProvider,
    pipeline_kind: OtelPipelineKind,
    drop_store: Option<ExportDropStore>,
    replayed_drops: Arc<Mutex<Vec<ExportDropAggregate>>>,
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

    pub fn pipeline_kind(&self) -> OtelPipelineKind {
        self.pipeline_kind
    }

    pub fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        let result = self.meter_provider.force_flush();
        if result.is_ok() {
            self.acknowledge_replayed_drops();
        }
        result
    }

    pub fn shutdown(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        let result = self.meter_provider.shutdown();
        if result.is_ok() {
            self.acknowledge_replayed_drops();
        }
        result
    }

    fn acknowledge_replayed_drops(&self) {
        let snapshot = std::mem::take(&mut *self.replayed_drops.lock().expect("replayed export-drop mutex poisoned"));
        if let Some(store) = &self.drop_store {
            store.subtract(&snapshot);
        }
    }
}

pub fn init_otel(config: &TelemetryConfig) -> OtelProviders {
    if !config.enabled {
        ExportDropStore::new(config.state_dir.clone()).clear();
        return init_noop_otel(config);
    }
    if config.exports_enabled()
        && let Some(endpoint) = config.otlp_endpoint.as_deref()
    {
        match build_otlp_http_providers(config, endpoint) {
            Ok(providers) => return providers,
            Err(err) => warn!(%err, "failed to initialize OTLP telemetry exporter; falling back to no-op"),
        }
    }

    init_noop_otel(config)
}

pub fn init_noop_otel(_config: &TelemetryConfig) -> OtelProviders {
    let meter_provider = SdkMeterProvider::builder().with_view(histogram_bucket_view).build();
    global::set_meter_provider(meter_provider.clone());
    OtelProviders {
        meter_provider,
        pipeline_kind: OtelPipelineKind::Noop,
        drop_store: None,
        replayed_drops: Arc::new(Mutex::new(Vec::new())),
    }
}

fn build_otlp_http_providers(
    config: &TelemetryConfig,
    endpoint: &str,
) -> Result<OtelProviders, opentelemetry_otlp::ExporterBuildError> {
    let drop_store = ExportDropStore::new(config.state_dir.clone());
    let replayed_drops = drop_store.snapshot();
    let resource = telemetry_resource(config);
    let metric_exporter = MetricExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(signal_endpoint(endpoint, "/v1/metrics"))
        .with_timeout(Duration::from_secs(30))
        .with_http_client(otlp_http_client(Some(drop_store.clone())))
        .with_headers(otlp_headers(config))
        .with_temporality(Temporality::Delta)
        .build()?;

    let reader = PeriodicReader::builder(metric_exporter)
        .with_interval(otel_export_interval_from_env())
        .build();
    let meter_provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .with_view(histogram_bucket_view)
        .build();

    global::set_meter_provider(meter_provider.clone());
    // Record replayed drops through the provider we just built rather than
    // `global::meter(...)`: the global provider is a process-wide singleton that
    // concurrent initializations can swap, which would misroute these metrics to a
    // different provider and leave this provider's reader empty at force_flush.
    let emitted_replayed_drops = emit_replayed_drops(&meter_provider.meter("kiro-telemetry"), &replayed_drops);
    Ok(OtelProviders {
        meter_provider,
        pipeline_kind: OtelPipelineKind::OtlpHttp,
        drop_store: Some(drop_store),
        replayed_drops: Arc::new(Mutex::new(emitted_replayed_drops)),
    })
}

fn emit_replayed_drops(meter: &Meter, entries: &[ExportDropAggregate]) -> Vec<ExportDropAggregate> {
    let mut emitted = Vec::new();
    for entry in entries {
        let reason = match entry.key.drop_reason.as_str() {
            "oversize" => crate::metric::ExportDropReason::Oversize,
            "invalid_record" => crate::metric::ExportDropReason::InvalidRecord,
            "encoding_failure" => crate::metric::ExportDropReason::EncodingFailure,
            "retry_exhausted" => crate::metric::ExportDropReason::RetryExhausted,
            "permanent_rejection" => crate::metric::ExportDropReason::PermanentRejection,
            "unknown" => crate::metric::ExportDropReason::Unknown,
            value => {
                trace!(drop_reason = value, "normalizing unrecognized export-drop reason");
                crate::metric::ExportDropReason::Unknown
            },
        };
        let Some(record) =
            crate::metric::record_telemetry_export_dropped_for_version(entry.count, &entry.key.version_full, reason)
        else {
            trace!("retaining export-drop record that did not produce a metric");
            continue;
        };
        let MetricValue::Counter(value) = record.value else {
            trace!("retaining export-drop record with an unexpected metric kind");
            continue;
        };
        meter
            .u64_counter(record.name)
            .build()
            .add(value, &otel_attributes(&record.attributes));
        emitted.push(entry.clone());
    }
    emitted
}

fn telemetry_resource(config: &TelemetryConfig) -> Resource {
    Resource::builder()
        .with_service_name("kiro-cli")
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .with_attribute(KeyValue::new(
            "deployment.environment",
            config.deployment_environment.clone(),
        ))
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

fn otlp_headers(config: &TelemetryConfig) -> HashMap<String, String> {
    HashMap::from([(KIRO_MACHINE_ID_HEADER.to_string(), config.machine_id.clone())])
}

fn otlp_http_client(drop_store: Option<ExportDropStore>) -> KutsHttpClient {
    KutsHttpClient::new(drop_store)
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

fn instrument_kind_error(name: &str) -> TelemetryError {
    TelemetryError::Sink(format!("metric `{name}` reused with different OTel instrument kind"))
}

#[cfg(test)]
mod tests {
    use opentelemetry::KeyValue;
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_proto::tonic::metrics::v1::{
        Metric,
        NumberDataPoint,
        ResourceMetrics,
        ScopeMetrics,
        Sum,
    };

    use super::*;
    use crate::testing::{
        OtlpTestCollector,
        expect_otlp_metric,
        expect_otlp_metric_resource_attribute,
        expect_otlp_request,
    };
    use crate::{
        OtelMode,
        TelemetryConfig,
        metric,
    };

    const TEST_MACHINE_ID: &str = "ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e";
    const TEST_DEPLOYMENT_ENVIRONMENT: &str = "test";

    fn test_config(enabled: bool, otel_mode: OtelMode, otlp_endpoint: Option<String>) -> TelemetryConfig {
        TelemetryConfig::new(
            enabled,
            otel_mode,
            otlp_endpoint,
            tempfile::tempdir().expect("temporary state").keep(),
        )
        .with_machine_id(TEST_MACHINE_ID)
        .with_deployment_environment(TEST_DEPLOYMENT_ENVIRONMENT)
    }

    fn encoded_counter_request(metric_names: &[&str]) -> Bytes {
        let metrics = metric_names
            .iter()
            .map(|name| Metric {
                name: (*name).to_string(),
                data: Some(Data::Sum(Sum {
                    data_points: vec![NumberDataPoint::default()],
                    is_monotonic: true,
                    ..Default::default()
                })),
                ..Default::default()
            })
            .collect();
        Bytes::from(
            ExportMetricsServiceRequest {
                resource_metrics: vec![ResourceMetrics {
                    scope_metrics: vec![ScopeMetrics {
                        metrics,
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
            }
            .encode_to_vec(),
        )
    }

    fn drop_count(store: &ExportDropStore, reason: &str) -> u64 {
        store
            .snapshot()
            .iter()
            .filter(|entry| entry.key.drop_reason == reason)
            .map(|entry| entry.count)
            .sum()
    }

    #[test]
    fn histogram_boundaries_target_non_millisecond_instruments() {
        // Second-scale latencies must not inherit the SDK's ms-scale defaults.
        assert_eq!(
            histogram_boundaries("kiro_cli_model_request_duration_seconds"),
            Some(SECONDS_LATENCY_BOUNDARIES)
        );
        assert_eq!(
            histogram_boundaries("kiro_cli_user_turn_duration_seconds"),
            Some(SECONDS_LATENCY_BOUNDARIES)
        );
        // Unit-interval ratios.
        assert_eq!(
            histogram_boundaries("kiro_cli_process_cpu_utilization_ratio"),
            Some(RATIO_BOUNDARIES)
        );
        assert_eq!(
            histogram_boundaries("kiro_cli_automatic_retries_per_operation"),
            Some(RETRY_COUNT_BOUNDARIES)
        );
        assert_eq!(
            histogram_boundaries("kiro_cli_process_peak_rss_bytes"),
            Some(MEMORY_BYTES_BOUNDARIES)
        );

        // Genuine millisecond latencies keep the SDK default buckets (no override).
        assert_eq!(histogram_boundaries("kiro_cli_tool_execution_duration_ms"), None);
        assert_eq!(histogram_boundaries("kiro_cli_model_time_to_first_content_ms"), None);

        for boundaries in [
            SECONDS_LATENCY_BOUNDARIES,
            RATIO_BOUNDARIES,
            RETRY_COUNT_BOUNDARIES,
            MEMORY_BYTES_BOUNDARIES,
        ] {
            Stream::builder()
                .with_aggregation(Aggregation::ExplicitBucketHistogram {
                    boundaries: boundaries.to_vec(),
                    record_min_max: true,
                })
                .build()
                .expect("configured histogram boundaries must be valid");
        }
    }

    #[test]
    fn noop_otel_provider_accepts_counter_adds() {
        let config = test_config(true, OtelMode::Off, None);

        let providers = init_noop_otel(&config);
        let counter = global::meter("kiro-telemetry-test")
            .u64_counter("kiro_cli_model_invocations_total")
            .build();

        counter.add(1, &[KeyValue::new("agent_engine", "v2")]);

        providers.force_flush().expect("noop provider flush should succeed");
    }

    #[test]
    fn init_otel_stays_noop_without_endpoint() {
        let config = test_config(true, OtelMode::DualWrite, None);

        let providers = init_otel(&config);

        assert_eq!(providers.pipeline_kind(), OtelPipelineKind::Noop);
        providers.shutdown().expect("noop provider shutdown should succeed");
    }

    #[test]
    fn init_otel_builds_otlp_http_when_endpoint_configured() {
        let config = test_config(true, OtelMode::DualWrite, Some("http://localhost:4318".to_string()));

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
        let config = test_config(true, OtelMode::DualWrite, None);

        let providers = init_noop_otel(&config);
        let sink = std::sync::Arc::new(OtelMetricsSink::new(global::meter("kiro-telemetry-test-sink")));
        let client = crate::TelemetryClient::new(config).with_sink(sink);

        client
            .emit(metric::record_run_outcome(
                metric::SessionInterface::InteractiveCli,
                metric::Engine::V2,
                metric::OsType::Macos,
                metric::RunOutcome::Success,
            ))
            .expect("counter emit should succeed");
        client
            .emit(
                metric::record_model_request_duration_seconds(
                    0.25,
                    metric::Engine::V2,
                    Some("claude-sonnet-4"),
                    metric::ModelRequestOutcome::Success,
                )
                .expect("positive duration"),
            )
            .expect("histogram emit should succeed");
        client
            .emit(
                metric::record_process_memory_rss_bytes(
                    128.0 * 1024.0 * 1024.0,
                    metric::OsType::Macos,
                    metric::Engine::V2,
                    metric::ProcessRole::Host,
                )
                .expect("non-negative memory"),
            )
            .expect("gauge emit should succeed");

        providers.force_flush().expect("noop provider flush should succeed");
    }

    #[test]
    fn otlp_proto_contract_types_are_available_for_mock_collector() {
        use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;

        let metrics = ExportMetricsServiceRequest::default();

        assert!(metrics.resource_metrics.is_empty());
    }

    #[test]
    fn otlp_http_exporter_sends_decodable_metric_payloads() {
        let collector = OtlpTestCollector::start(1);
        let config =
            test_config(true, OtelMode::DualWrite, Some(collector.endpoint())).with_user_id("test-user-id".to_string());
        let providers = init_otel(&config);
        assert_eq!(providers.pipeline_kind(), OtelPipelineKind::OtlpHttp);

        let client = crate::TelemetryClient::new(config).with_sink(std::sync::Arc::new(OtelMetricsSink::new(
            providers.meter_provider().meter("kiro-telemetry-mock-collector-test"),
        )));

        let expected_metric = metric::record_run_outcome(
            metric::SessionInterface::InteractiveCli,
            metric::Engine::V2,
            metric::OsType::Macos,
            metric::RunOutcome::Success,
        );

        let properties = crate::MetricLogProperties::default()
            .with_session_id("test-session-id".to_string())
            .with_request_id("test-request-id".to_string());
        client
            .emit_with_log_properties(expected_metric.clone(), &properties)
            .expect("metric emit should succeed");
        let expected_metric = expected_metric
            .with_attribute("user_id", "test-user-id")
            .with_attribute("session_id", "test-session-id")
            .with_attribute("request_id", "test-request-id");

        providers.force_flush().expect("otlp provider flush should succeed");
        let requests = collector.collect();
        providers.shutdown().expect("otlp provider shutdown should succeed");

        let metrics = expect_otlp_request(&requests, "/v1/metrics");
        assert_eq!(metrics.content_type.as_deref(), Some("application/x-protobuf"));
        assert_eq!(
            metrics.headers.get(KIRO_MACHINE_ID_HEADER).map(String::as_str),
            Some(TEST_MACHINE_ID)
        );
        expect_otlp_metric_resource_attribute(&requests, "deployment.environment", TEST_DEPLOYMENT_ENVIRONMENT);
        expect_otlp_metric_resource_attribute(
            &requests,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_DIMENSION,
            kiro_telemetry_schema::CLOUDWATCH_PRODUCT_VALUE,
        );
        expect_otlp_metric(&requests, &expected_metric);
    }

    #[test]
    fn kuts_http_client_retries_429_and_5xx_statuses() {
        let collector = OtlpTestCollector::start_with_statuses(vec![429, 500, 200]);
        let client = KutsHttpClient::with_retry_policy(KutsRetryPolicy {
            max_retries: 3,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        });
        let request = Request::builder()
            .method("POST")
            .uri(format!("{}/v1/metrics", collector.endpoint()))
            .body(Bytes::from_static(b"test-payload"))
            .expect("test request");

        let response =
            futures::executor::block_on(client.send_bytes(request)).expect("retry should eventually succeed");
        assert_eq!(response.status().as_u16(), 200);

        let requests = collector.collect();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|request| request.is_metrics()));
    }

    #[test]
    fn kuts_http_client_rejects_oversize_payload_before_send() {
        let collector = OtlpTestCollector::start(0);
        let state = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(state.path().to_path_buf());
        let client = KutsHttpClient {
            drop_store: Some(store.clone()),
            ..KutsHttpClient::with_retry_policy(KutsRetryPolicy {
                max_retries: 0,
                initial_delay: Duration::ZERO,
                max_delay: Duration::ZERO,
            })
        };
        let request = Request::builder()
            .method("POST")
            .uri("http://127.0.0.1:9/v1/metrics")
            .body(Bytes::from(vec![0_u8; KUTS_MAX_REQUEST_BODY_BYTES + 1]))
            .expect("test request");

        let err = futures::executor::block_on(client.send_bytes(request)).expect_err("oversize payload should fail");
        assert!(err.to_string().contains("KUTS limit"));

        let requests = collector.collect_timeout(Duration::from_millis(50));
        assert!(requests.is_empty(), "oversize payload should not hit the collector");
        let drops = store.snapshot();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].key.drop_reason, "oversize");
        assert_eq!(drops[0].count, 1);
    }

    #[test]
    fn replays_persisted_export_drops_and_removes_only_after_flush() {
        let state = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(state.path().to_path_buf());
        store.record("metrics", "retry_exhausted", 3);
        let collector = OtlpTestCollector::start(1);
        let config = TelemetryConfig::new(
            true,
            OtelMode::DualWrite,
            Some(collector.endpoint()),
            state.path().to_path_buf(),
        )
        .with_machine_id(TEST_MACHINE_ID)
        .with_deployment_environment(TEST_DEPLOYMENT_ENVIRONMENT);
        let providers = init_otel(&config);

        assert_eq!(store.snapshot()[0].count, 3);
        providers.force_flush().expect("replayed metric should flush");
        let requests = collector.collect();
        let expected = crate::metric::record_telemetry_export_dropped_for_version(
            3,
            env!("CARGO_PKG_VERSION"),
            crate::metric::ExportDropReason::RetryExhausted,
        )
        .unwrap();
        expect_otlp_metric(&requests, &expected);
        assert!(store.snapshot().is_empty());
        providers.shutdown().expect("provider shutdown");
    }

    #[test]
    fn failed_flush_retains_replayed_drop_aggregates() {
        let state = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(state.path().to_path_buf());
        store.record("metrics", "retry_exhausted", 3);
        let collector = OtlpTestCollector::start_with_statuses(vec![400]);
        let config = TelemetryConfig::new(
            true,
            OtelMode::DualWrite,
            Some(collector.endpoint()),
            state.path().to_path_buf(),
        )
        .with_machine_id(TEST_MACHINE_ID)
        .with_deployment_environment(TEST_DEPLOYMENT_ENVIRONMENT);
        let providers = init_otel(&config);
        let client = crate::TelemetryClient::new(config).with_sink(Arc::new(OtelMetricsSink::new(
            providers.meter_provider().meter("failed-flush-retains-drops"),
        )));
        client
            .emit(metric::record_run_outcome(
                metric::SessionInterface::InteractiveCli,
                metric::Engine::V2,
                metric::OsType::Macos,
                metric::RunOutcome::Failure,
            ))
            .unwrap();

        assert!(providers.force_flush().is_err());
        collector.collect();

        assert_eq!(drop_count(&store, "retry_exhausted"), 3);
        assert_eq!(drop_count(&store, "permanent_rejection"), 1);
    }

    #[test]
    fn repeated_replay_failures_do_not_count_the_replayed_point_as_a_new_drop() {
        let state = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(state.path().to_path_buf());
        store.record("metrics", "retry_exhausted", 3);
        let original = store.snapshot();
        let collector = OtlpTestCollector::start_with_statuses(vec![400, 400]);
        let client = KutsHttpClient {
            drop_store: Some(store.clone()),
            ..KutsHttpClient::with_retry_policy(KutsRetryPolicy {
                max_retries: 0,
                initial_delay: Duration::ZERO,
                max_delay: Duration::ZERO,
            })
        };
        let request = Request::builder()
            .method("POST")
            .uri(format!("{}/v1/metrics", collector.endpoint()))
            .body(encoded_counter_request(&[TELEMETRY_EXPORT_DROPPED_METRIC]))
            .unwrap();

        for _ in 0..2 {
            let response = futures::executor::block_on(client.send_bytes(request.clone())).unwrap();
            assert_eq!(response.status().as_u16(), 400);
            assert_eq!(store.snapshot(), original);
        }
        collector.collect();
    }

    #[test]
    fn replay_preserves_unrecognized_export_drop_counts_under_unknown() {
        let state = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(state.path().to_path_buf());
        store.record("metrics", "oversize", 1);
        store.record("future_signal", "oversize", 2);
        store.record("metrics", "future_reason", 3);
        let snapshot = store.snapshot();

        // A standalone provider keeps this test independent of the process-global
        // meter provider; it asserts only on the returned emitted set, not on export.
        let provider = SdkMeterProvider::builder().build();
        let emitted = emit_replayed_drops(&provider.meter("kiro-telemetry-test-replay"), &snapshot);
        store.subtract(&emitted);

        assert_eq!(emitted.len(), 3);
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn telemetry_opt_out_deletes_pending_drop_state_without_exporting() {
        let state = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(state.path().to_path_buf());
        store.record("metrics", "oversize", 2);
        let config = TelemetryConfig::new(
            false,
            OtelMode::DualWrite,
            Some("http://127.0.0.1:9".to_string()),
            state.path().to_path_buf(),
        );

        let providers = init_otel(&config);

        assert!(store.snapshot().is_empty());
        assert_eq!(providers.pipeline_kind(), OtelPipelineKind::Noop);
    }
}
