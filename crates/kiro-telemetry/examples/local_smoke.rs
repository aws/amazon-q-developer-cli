//! Emits a representative metric set through the OTLP/HTTP exporter.

use std::env;
use std::error::Error;
use std::sync::Arc;

use kiro_telemetry::{
    OtelMetricsSink,
    OtelPipelineKind,
    TelemetryClient,
    TelemetryConfig,
    init_otel,
    metric,
};
use opentelemetry::metrics::MeterProvider as _;

fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = env::var("KIRO_TELEMETRY_OTLP_ENDPOINT").unwrap_or_else(|_| "http://localhost:4318".to_string());
    let state_dir = env::var("KIRO_STATE_DIR").unwrap_or_else(|_| {
        env::temp_dir()
            .join("kiro-telemetry-local-smoke")
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

    let client = TelemetryClient::new(config).with_sink(Arc::new(OtelMetricsSink::new(
        providers.meter_provider().meter("kiro-telemetry-local-smoke"),
    )));
    let os_type = metric::OsType::from_name(env::consts::OS);
    let interface = metric::SessionInterface::InteractiveCli;
    let engine = metric::Engine::V3;
    let mode = metric::AgentMode::Plan;
    client.emit(metric::record_run_started(interface, engine, os_type))?;
    client.emit(metric::record_chat_session_started(
        interface,
        mode,
        engine,
        metric::TrustPosture::Unknown,
    ))?;
    client.emit(metric::record_model_invocation(engine, Some("claude-4-sonnet")))?;
    client.emit(metric::record_user_turn(interface, mode, engine))?;
    if let Some(record) = metric::record_user_turn_duration_seconds(1.5, interface, mode, engine) {
        client.emit(record)?;
    }
    client.emit(metric::record_run_outcome(
        interface,
        engine,
        os_type,
        metric::RunOutcome::Success,
    ))?;

    providers.force_flush()?;
    providers.shutdown()?;

    println!("sent kiro-telemetry smoke metrics to {endpoint}");
    Ok(())
}
