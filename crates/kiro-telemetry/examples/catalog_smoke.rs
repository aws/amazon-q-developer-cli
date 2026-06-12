//! Emits one record for every catalog metric and log_event that has a typed
//! constructor, then flushes through the OTLP/HTTP exporter. Paired with
//! `dev/telemetry/verify-catalog.sh`, this proves the full metric catalog reaches
//! the local collector -> Prometheus stack.
//!
//! The record set is shared with the `catalog_coverage` integration test via
//! `kiro_telemetry::testing`, so both the live smoke run and CI assert the same
//! catalog. Requires the `test-support` feature:
//!
//! ```bash
//! KIRO_TELEMETRY_OTLP_ENDPOINT=http://localhost:4318 \
//!   cargo run -p kiro-telemetry --features test-support --example catalog_smoke
//! ```

use std::env;
use std::error::Error;
use std::sync::Arc;

use kiro_telemetry::testing::{
    catalog_log_records,
    catalog_metric_records,
};
use kiro_telemetry::{
    OtelLogsSink,
    OtelMetricsSink,
    OtelPipelineKind,
    TelemetryClient,
    TelemetryConfig,
    init_otel,
};
use opentelemetry::metrics::MeterProvider as _;

fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = env::var("KIRO_TELEMETRY_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string());
    let state_dir = env::var("KIRO_STATE_DIR").unwrap_or_else(|_| {
        env::temp_dir()
            .join("kiro-telemetry-catalog-smoke")
            .to_string_lossy()
            .into_owned()
    });
    let mut config_pairs = vec![
        ("KIRO_TELEMETRY_ENABLED".to_string(), "1".to_string()),
        ("KIRO_TELEMETRY_OTEL".to_string(), "2".to_string()),
        ("KIRO_TELEMETRY_OTLP_ENDPOINT".to_string(), endpoint.clone()),
        ("KIRO_STATE_DIR".to_string(), state_dir),
    ];
    config_pairs.extend(env::vars());
    let config = TelemetryConfig::from_pairs(config_pairs);
    let providers = init_otel(&config);

    if providers.pipeline_kind() != OtelPipelineKind::OtlpHttp {
        return Err("OTLP exporter did not initialize; check KIRO_TELEMETRY_OTLP_ENDPOINT".into());
    }

    let client = TelemetryClient::new(config)
        .with_sink(Arc::new(OtelMetricsSink::new(
            providers.meter_provider().meter("kiro-telemetry-catalog-smoke"),
        )))
        .with_sink(Arc::new(OtelLogsSink::from_providers(&providers)));

    let metrics = catalog_metric_records();
    let logs = catalog_log_records();
    let (metric_count, log_count) = (metrics.len(), logs.len());
    for record in metrics {
        client.emit(record)?;
    }
    for record in logs {
        client.emit_log(record)?;
    }

    providers.force_flush()?;
    providers.shutdown()?;

    println!("emitted {metric_count} catalog metric records + {log_count} logs to {endpoint}");
    Ok(())
}
