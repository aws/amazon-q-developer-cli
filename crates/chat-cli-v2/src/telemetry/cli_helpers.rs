//! Os-only wrappers used by `cli/mod.rs` so the CLI dispatch layer never
//! reaches into per-event enrichers / V2-specific telemetry shapes.

use kiro_telemetry_host::TelemetryError;

use crate::os::Os;
#[cfg(feature = "voice")]
use crate::telemetry::TelemetryResult;

/// Daily heartbeat fired before every CLI subcommand if the per-day
/// guard fires. Wraps `send_daily_heartbeat`.
pub fn record_daily_heartbeat(os: &Os) -> Result<(), TelemetryError> {
    super::send_daily_heartbeat(&os.telemetry)
}

/// Records that a CLI subcommand executed. Builds the V2 metadata
/// enricher from `os.database` and forwards through the host
/// TelemetryThread.
pub async fn record_cli_subcommand(os: &Os, subcommand_name: String) -> Result<(), TelemetryError> {
    let enricher = super::build_metadata_enricher(std::sync::Arc::new(os.database.clone()));
    os.telemetry
        .send_cli_subcommand_executed(Some(&enricher), subcommand_name)
        .await
}

#[cfg(feature = "voice")]
#[allow(clippy::too_many_arguments)]
pub fn record_voice_input(
    os: &Os,
    result: TelemetryResult,
    reason: Option<String>,
    reason_desc: Option<String>,
    backend: String,
    input_method: &str,
) -> Result<(), TelemetryError> {
    super::send_voice_input(
        &os.telemetry,
        None, // conversation_id
        result,
        reason,
        reason_desc,
        backend,
        input_method.to_string(),
        None, // recording_duration_ms
        None, // transcription_duration_ms
        None, // text_length
        None, // model_size
        None, // auto_submit
    )
}
