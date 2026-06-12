use std::env;
use std::error::Error;
use std::sync::Arc;

use kiro_telemetry::{
    OtelLogsSink,
    OtelMetricsSink,
    OtelPipelineKind,
    TelemetryClient,
    TelemetryConfig,
    TokenUsage,
    init_otel,
    log,
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

    let client = TelemetryClient::new(config)
        .with_sink(Arc::new(OtelMetricsSink::new(
            providers.meter_provider().meter("kiro-telemetry-local-smoke"),
        )))
        .with_sink(Arc::new(OtelLogsSink::from_providers(&providers)));

    client.emit(metric::cli_session_started(
        metric::OsType::from_name(env::consts::OS),
        metric::InstallSource::Internal,
        metric::ClientApplication::ChatCliV3,
    ))?;
    client.emit(metric::chat_session_started(
        metric::Mode::Plan,
        metric::ClientApplication::ChatCliV3,
    ))?;
    client.emit(metric::cli_session_completed(
        metric::ExitReason::Clean,
        metric::AgentKind::Kas,
    ))?;
    client.emit(metric::feature_used("local_smoke"))?;

    let context = metric::TurnMetricContext::new(Some("claude-4-sonnet"), Some("kas")).app_type(Some("KAS"));
    let response = metric::ModelResponseMetrics::from_turn_context(context, metric::TurnOutcome::Succeeded, true)
        .context_file_length(Some(4096))
        .time_to_first_chunk_ms(Some(250.0))
        .time_between_chunks_ms(Some(&[32.0, 48.0]))
        .request_duration_seconds(Some(1.5))
        .token_usage(TokenUsage {
            uncached_input_tokens: 1200,
            cache_read_input_tokens: 300,
            cache_write_input_tokens: 0,
            output_tokens: 128,
        })
        .emit_user_turn_counter(true);

    for record in metric::model_response_records(response) {
        client.emit(record)?;
    }

    client.emit_log(log::conversation_completed(
        "local-smoke-session",
        "local-smoke-conversation",
        log::CompletionReason::Stop,
    ))?;

    providers.force_flush()?;
    providers.shutdown()?;

    println!("sent kiro-telemetry smoke metrics and logs to {endpoint}");
    Ok(())
}
