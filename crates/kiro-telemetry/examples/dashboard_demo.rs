//! Populates the local Prometheus and Grafana stack with coherent synthetic
//! installations that exercise the reviewed dashboard dimensions.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use std::{
    env,
    thread,
};

use kiro_telemetry::{
    OtelMetricsSink,
    OtelPipelineKind,
    TelemetryClient,
    TelemetryConfig,
    init_otel,
    validate_metric_record,
};
use opentelemetry::metrics::MeterProvider as _;

#[path = "dashboard_demo/emission.rs"]
mod emission;
#[path = "dashboard_demo/scenarios.rs"]
mod scenarios;

use emission::{
    activity_records,
    heartbeat_baselines,
    heartbeat_records,
};
use scenarios::SCENARIOS;

fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = env::var("KIRO_TELEMETRY_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string());
    let state_dir = env::var("KIRO_STATE_DIR").unwrap_or_else(|_| {
        env::temp_dir()
            .join("kiro-telemetry-dashboard-demo")
            .to_string_lossy()
            .into_owned()
    });
    let waves = env_u64("KIRO_DASHBOARD_DEMO_WAVES", 3)?;
    let interval_ms = env_u64("KIRO_DASHBOARD_DEMO_INTERVAL_MS", 5_500)?;
    let emit_heartbeats = env_flag("KIRO_DASHBOARD_DEMO_EMIT_HEARTBEATS", true)?;
    let mut config_pairs = env::vars().collect::<Vec<_>>();
    config_pairs.extend([
        ("KIRO_TELEMETRY_ENABLED".to_string(), "1".to_string()),
        ("KIRO_TELEMETRY_OTEL".to_string(), "2".to_string()),
        ("KIRO_TELEMETRY_OTLP_ENDPOINT".to_string(), endpoint.clone()),
        ("KIRO_STATE_DIR".to_string(), state_dir),
    ]);
    let config = TelemetryConfig::from_pairs(config_pairs);
    let providers = init_otel(&config);
    if providers.pipeline_kind() != OtelPipelineKind::OtlpHttp {
        return Err("OTLP exporter did not initialize; check KIRO_TELEMETRY_OTLP_ENDPOINT".into());
    }

    let client = TelemetryClient::new(config).with_sink(Arc::new(OtelMetricsSink::new(
        providers.meter_provider().meter("kiro-telemetry-dashboard-demo"),
    )));
    let baselines = heartbeat_baselines();
    emit_records(&client, baselines.iter().cloned())?;
    providers.force_flush()?;
    if interval_ms > 0 {
        thread::sleep(Duration::from_millis(interval_ms));
    }

    let heartbeats = heartbeat_records();
    if emit_heartbeats {
        emit_records(&client, heartbeats.iter().cloned())?;
    }

    let activity = activity_records();
    for wave in 0..waves {
        emit_records(&client, activity.iter().cloned())?;
        providers.force_flush()?;
        if wave + 1 < waves && interval_ms > 0 {
            thread::sleep(Duration::from_millis(interval_ms));
        }
    }
    providers.shutdown()?;

    let activity_count = activity.len() * usize::try_from(waves).unwrap_or(usize::MAX);
    let heartbeat_count = baselines.len() + usize::from(emit_heartbeats) * heartbeats.len();
    println!(
        "emitted {} synthetic records across {waves} activity wave(s) and {} coherent installations to {endpoint}",
        activity_count.saturating_add(heartbeat_count),
        SCENARIOS.len()
    );
    Ok(())
}

fn emit_records(
    client: &TelemetryClient,
    records: impl IntoIterator<Item = kiro_telemetry::MetricRecord>,
) -> Result<(), Box<dyn Error>> {
    for record in records {
        validate_metric_record(&record)?;
        client.emit(record)?;
    }
    Ok(())
}

fn env_u64(name: &str, default: u64) -> Result<u64, Box<dyn Error>> {
    match env::var(name) {
        Ok(value) => parse_u64(name, &value),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn parse_u64(name: &str, value: &str) -> Result<u64, Box<dyn Error>> {
    value
        .trim()
        .parse()
        .map_err(|error| format!("{name} must be an unsigned integer; got {value:?}: {error}").into())
}

fn env_flag(name: &str, default: bool) -> Result<bool, Box<dyn Error>> {
    match env::var(name) {
        Ok(value) => parse_flag(&value)
            .ok_or_else(|| format!("{name} must be one of 1, 0, true, or false; got {value:?}").into()),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn parse_flag(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_flag,
        parse_u64,
    };

    #[test]
    fn flag_parser_normalizes_known_values_and_rejects_typos() {
        assert_eq!(parse_flag(" TRUE "), Some(true));
        assert_eq!(parse_flag("False"), Some(false));
        assert_eq!(parse_flag("enabled"), None);
    }

    #[test]
    fn numeric_environment_overrides_fail_closed() {
        assert_eq!(parse_u64("WAVES", " 4 ").expect("valid wave count"), 4);
        let error = parse_u64("WAVES", "many").expect_err("word is not an unsigned integer");
        assert!(error.to_string().contains("WAVES must be an unsigned integer"));
    }
}
