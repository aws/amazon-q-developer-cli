use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_OTEL_EXPORT_INTERVAL: Duration = Duration::from_secs(60);
const KIRO_TELEMETRY_EXPORT_INTERVAL_MS: &str = "KIRO_TELEMETRY_EXPORT_INTERVAL_MS";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OtelMode {
    #[default]
    Off,
    DualWrite,
    OtelOnly,
}

impl OtelMode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "dual" | "dual-write" | "dual_write" => Self::DualWrite,
            "2" | "otel" | "otel-only" | "otel_only" => Self::OtelOnly,
            _ => Self::Off,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub otel_mode: OtelMode,
    pub otlp_endpoint: Option<String>,
    pub state_dir: PathBuf,
}

impl TelemetryConfig {
    pub fn new(enabled: bool, otel_mode: OtelMode, otlp_endpoint: Option<String>, state_dir: PathBuf) -> Self {
        Self {
            enabled,
            otel_mode,
            otlp_endpoint: otlp_endpoint.filter(|value| !value.trim().is_empty()),
            state_dir,
        }
    }

    pub fn from_env() -> Self {
        Self::from_pairs(std::env::vars())
    }

    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let env: HashMap<String, String> = pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();

        let enabled = env
            .get("KIRO_TELEMETRY_ENABLED")
            .is_none_or(|value| !matches_disabled(value));
        let otel_mode = env
            .get("KIRO_TELEMETRY_OTEL")
            .map_or(OtelMode::Off, |value| OtelMode::parse(value));
        let otlp_endpoint = env
            .get("KIRO_TELEMETRY_OTLP_ENDPOINT")
            .filter(|value| !value.trim().is_empty())
            .cloned();
        let state_dir = env
            .get("KIRO_STATE_DIR")
            .map_or_else(|| std::env::temp_dir().join("kiro-telemetry"), PathBuf::from);

        Self::new(enabled, otel_mode, otlp_endpoint, state_dir)
    }

    pub fn exports_enabled(&self) -> bool {
        self.enabled && self.otel_mode != OtelMode::Off
    }
}

pub(crate) fn otel_export_interval_from_env() -> Duration {
    otel_export_interval_from_millis(std::env::var(KIRO_TELEMETRY_EXPORT_INTERVAL_MS).ok().as_deref())
}

pub(crate) fn otel_export_interval_from_millis(value: Option<&str>) -> Duration {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|milliseconds| *milliseconds > 0)
        .map_or(DEFAULT_OTEL_EXPORT_INTERVAL, Duration::from_millis)
}

fn matches_disabled(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "disabled" | "no"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otel_mode_defaults_off() {
        let config = TelemetryConfig::from_pairs(std::iter::empty::<(&str, &str)>());

        assert_eq!(config.otel_mode, OtelMode::Off);
        assert!(!config.exports_enabled());
    }

    #[test]
    fn parses_dual_write_and_endpoint_override() {
        let config = TelemetryConfig::from_pairs([
            ("KIRO_TELEMETRY_OTEL", "1"),
            ("KIRO_TELEMETRY_OTLP_ENDPOINT", "https://otel.example.test"),
        ]);

        assert_eq!(config.otel_mode, OtelMode::DualWrite);
        assert_eq!(config.otlp_endpoint.as_deref(), Some("https://otel.example.test"));
        assert!(config.exports_enabled());
    }

    #[test]
    fn opt_out_disables_exports_even_when_otel_requested() {
        let config = TelemetryConfig::from_pairs([("KIRO_TELEMETRY_ENABLED", "0"), ("KIRO_TELEMETRY_OTEL", "2")]);

        assert_eq!(config.otel_mode, OtelMode::OtelOnly);
        assert!(!config.exports_enabled());
    }

    #[test]
    fn parses_export_interval_override() {
        assert_eq!(otel_export_interval_from_millis(Some("5000")), Duration::from_secs(5));
        assert_eq!(
            otel_export_interval_from_millis(Some("0")),
            DEFAULT_OTEL_EXPORT_INTERVAL
        );
        assert_eq!(
            otel_export_interval_from_millis(Some("not-a-number")),
            DEFAULT_OTEL_EXPORT_INTERVAL
        );
        assert_eq!(otel_export_interval_from_millis(None), DEFAULT_OTEL_EXPORT_INTERVAL);
    }
}
