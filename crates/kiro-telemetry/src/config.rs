use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_OTEL_EXPORT_INTERVAL: Duration = Duration::from_secs(60);
const DEFAULT_MACHINE_ID: &str = "kiro-cli";
const DEFAULT_DEPLOYMENT_ENVIRONMENT: &str = "dev";
const KIRO_TELEMETRY_EXPORT_INTERVAL_MS: &str = "KIRO_TELEMETRY_EXPORT_INTERVAL_MS";
const KIRO_TELEMETRY_MACHINE_ID: &str = "KIRO_TELEMETRY_MACHINE_ID";
const KIRO_TELEMETRY_DEPLOYMENT_ENVIRONMENT: &str = "KIRO_TELEMETRY_DEPLOYMENT_ENVIRONMENT";
const KUTS_ENDPOINT_EU_CENTRAL_1: &str = "https://prod.eu-central-1.telemetry-v2.kiro.dev";
const KUTS_ENDPOINT_US_EAST_1: &str = "https://prod.us-east-1.telemetry-v2.kiro.dev";

pub fn resolve_otlp_endpoint(endpoint_override: Option<String>, region: Option<&str>) -> String {
    match endpoint_override {
        Some(endpoint) if !endpoint.trim().is_empty() => endpoint,
        _ => match region {
            Some("eu-central-1") => KUTS_ENDPOINT_EU_CENTRAL_1.to_string(),
            _ => KUTS_ENDPOINT_US_EAST_1.to_string(),
        },
    }
}

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
    pub machine_id: String,
    pub user_id: Option<String>,
    pub service_version: String,
    pub deployment_environment: String,
    pub state_dir: PathBuf,
}

impl TelemetryConfig {
    pub fn new(enabled: bool, otel_mode: OtelMode, otlp_endpoint: Option<String>, state_dir: PathBuf) -> Self {
        let otlp_endpoint = otlp_endpoint.filter(|value| !value.trim().is_empty());
        let deployment_environment = deployment_environment_from_endpoint(otlp_endpoint.as_deref()).to_string();
        Self {
            enabled,
            otel_mode,
            otlp_endpoint,
            machine_id: DEFAULT_MACHINE_ID.to_string(),
            user_id: None,
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            deployment_environment,
            state_dir,
        }
    }

    pub fn with_machine_id(mut self, machine_id: impl Into<String>) -> Self {
        let machine_id = machine_id.into();
        if !machine_id.trim().is_empty() {
            self.machine_id = machine_id;
        }
        self
    }

    pub fn with_user_id(mut self, user_id: impl Into<Option<String>>) -> Self {
        self.user_id = user_id.into().filter(|user_id| crate::is_valid_raw_user_id(user_id));
        self
    }

    pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
        let version = version.into();
        if !version.trim().is_empty() {
            self.service_version = version;
        }
        self
    }

    pub fn with_deployment_environment(mut self, environment: impl Into<String>) -> Self {
        let environment = environment.into();
        if !environment.trim().is_empty() {
            self.deployment_environment = environment;
        }
        self
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

        let mut config = Self::new(enabled, otel_mode, otlp_endpoint, state_dir);
        if let Some(machine_id) = env.get(KIRO_TELEMETRY_MACHINE_ID) {
            config = config.with_machine_id(machine_id.clone());
        }
        if let Some(environment) = env.get(KIRO_TELEMETRY_DEPLOYMENT_ENVIRONMENT) {
            config = config.with_deployment_environment(environment.clone());
        }
        config
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

fn deployment_environment_from_endpoint(endpoint: Option<&str>) -> &'static str {
    let Some(endpoint) = endpoint else {
        return DEFAULT_DEPLOYMENT_ENVIRONMENT;
    };
    let endpoint = endpoint.to_ascii_lowercase();
    if endpoint.contains("prod.") || endpoint.contains("//prod-") {
        "prod"
    } else if endpoint.contains("gamma.") || endpoint.contains("//gamma-") {
        "gamma"
    } else if endpoint.contains("beta.") || endpoint.contains("//beta-") {
        "beta"
    } else if endpoint.contains(".test.telemetry-v2.") {
        "personal"
    } else {
        DEFAULT_DEPLOYMENT_ENVIRONMENT
    }
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
        assert_eq!(config.deployment_environment, DEFAULT_DEPLOYMENT_ENVIRONMENT);
        assert!(config.exports_enabled());
    }

    #[test]
    fn opt_out_disables_exports_even_when_otel_requested() {
        let config = TelemetryConfig::from_pairs([("KIRO_TELEMETRY_ENABLED", "0"), ("KIRO_TELEMETRY_OTEL", "2")]);

        assert_eq!(config.otel_mode, OtelMode::OtelOnly);
        assert!(!config.exports_enabled());
    }

    #[test]
    fn parses_machine_id_and_deployment_environment() {
        let config = TelemetryConfig::from_pairs([
            ("KIRO_TELEMETRY_MACHINE_ID", "ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            ("KIRO_TELEMETRY_DEPLOYMENT_ENVIRONMENT", "beta"),
        ]);

        assert_eq!(config.machine_id, "ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e");
        assert_eq!(config.deployment_environment, "beta");
    }

    #[test]
    fn validates_user_id_before_resource_injection() {
        let base = TelemetryConfig::from_pairs(std::iter::empty::<(&str, &str)>());
        assert_eq!(
            base.clone()
                .with_user_id(Some("authenticated-user".to_string()))
                .user_id,
            Some("authenticated-user".to_string())
        );
        assert_eq!(base.clone().with_user_id(Some(" \n".to_string())).user_id, None);
        assert_eq!(base.with_user_id(Some("x".repeat(513))).user_id, None);
    }

    #[test]
    fn infers_deployment_environment_from_kuts_endpoint() {
        let config = TelemetryConfig::from_pairs([(
            "KIRO_TELEMETRY_OTLP_ENDPOINT",
            "https://gamma.us-east-1.telemetry-v2.kiro.dev",
        )]);

        assert_eq!(config.deployment_environment, "gamma");
    }

    #[test]
    fn resolves_otlp_endpoint() {
        assert_eq!(
            resolve_otlp_endpoint(None, Some("eu-central-1")),
            "https://prod.eu-central-1.telemetry-v2.kiro.dev"
        );
        assert_eq!(
            resolve_otlp_endpoint(None, Some("us-east-1")),
            "https://prod.us-east-1.telemetry-v2.kiro.dev"
        );
        assert_eq!(
            resolve_otlp_endpoint(None, Some("us-west-2")),
            "https://prod.us-east-1.telemetry-v2.kiro.dev"
        );
        assert_eq!(
            resolve_otlp_endpoint(Some(String::new()), Some("eu-central-1")),
            "https://prod.eu-central-1.telemetry-v2.kiro.dev"
        );
        assert_eq!(
            resolve_otlp_endpoint(Some("   ".to_string()), None),
            "https://prod.us-east-1.telemetry-v2.kiro.dev"
        );
        assert_eq!(
            resolve_otlp_endpoint(Some("https://otel.example.test".to_string()), Some("eu-central-1")),
            "https://otel.example.test"
        );

        let default = resolve_otlp_endpoint(None, None);
        assert!(!default.contains("14318"));
        assert!(!default.contains("127.0.0.1"));
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
