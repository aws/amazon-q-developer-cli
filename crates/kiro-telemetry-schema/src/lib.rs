use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{
    Deserialize,
    Serialize,
};

pub const DEFAULT_SERIES_CAP: usize = 5_000;
pub const CLOUDWATCH_LEGACY_NAMESPACE: &str = "Toolkit";
pub const CLOUDWATCH_OTEL_NAMESPACE: &str = "ChatCLI";
pub const CLOUDWATCH_PRODUCT_DIMENSION: &str = "product";
pub const CLOUDWATCH_PRODUCT_VALUE: &str = "CodewhispererTerminal";
pub const KUTS_EXPORT_OVERSIZE_METRIC: &str = "kuts_export_oversize_total";

const METRICS_YAML: &str = include_str!("../schema/metrics.yaml");
const TYPES_YAML: &str = include_str!("../schema/types.yaml");
const LEGACY_MAPPING_YAML: &str = include_str!("../schema/legacy_mapping.yaml");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("failed to parse telemetry schema YAML: {0}")]
    Parse(String),
    #[error("metric `{metric}` references unknown attribute `{attribute}`")]
    UnknownAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` has {count} attributes; maximum is 10")]
    TooManyAttributes { metric: String, count: usize },
    #[error("metric `{metric}` uses metric-forbidden attribute `{attribute}`")]
    ForbiddenMetricAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` is missing a temporality")]
    MissingTemporality { metric: String },
    #[error("legacy mapping contains duplicate event type `{0}`")]
    DuplicateLegacyEvent(String),
    #[error("legacy mapping is missing event type `{0}`")]
    MissingLegacyEvent(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Registry {
    pub metrics: Vec<MetricSpec>,
    pub attributes: Vec<AttributeSpec>,
}

impl Registry {
    pub fn parse() -> Result<Self, SchemaError> {
        #[derive(Deserialize)]
        struct MetricsDoc {
            metrics: Vec<MetricSpec>,
        }

        #[derive(Deserialize)]
        struct TypesDoc {
            attributes: Vec<AttributeSpec>,
        }

        let metrics: MetricsDoc =
            serde_yaml::from_str(METRICS_YAML).map_err(|err| SchemaError::Parse(err.to_string()))?;
        let types: TypesDoc = serde_yaml::from_str(TYPES_YAML).map_err(|err| SchemaError::Parse(err.to_string()))?;

        let registry = Self {
            metrics: metrics.metrics,
            attributes: types.attributes,
        };
        registry.validate()?;
        Ok(registry)
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        let attributes: HashSet<&str> = self.attributes.iter().map(|attr| attr.name.as_str()).collect();

        for metric in &self.metrics {
            if metric.kind.is_metric() && metric.attributes.len() > 10 {
                return Err(SchemaError::TooManyAttributes {
                    metric: metric.name.clone(),
                    count: metric.attributes.len(),
                });
            }

            if metric.kind.requires_temporality() && metric.temporality.is_none() {
                return Err(SchemaError::MissingTemporality {
                    metric: metric.name.clone(),
                });
            }

            for attribute in &metric.attributes {
                if !attributes.contains(attribute.as_str()) {
                    return Err(SchemaError::UnknownAttribute {
                        metric: metric.name.clone(),
                        attribute: attribute.clone(),
                    });
                }

                if metric.kind.is_metric() && self.attribute(attribute).is_some_and(|spec| !spec.metric_allowed) {
                    return Err(SchemaError::ForbiddenMetricAttribute {
                        metric: metric.name.clone(),
                        attribute: attribute.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn metric(&self, name: &str) -> Option<&MetricSpec> {
        self.metrics.iter().find(|metric| metric.name == name)
    }

    pub fn attribute(&self, name: &str) -> Option<&AttributeSpec> {
        self.attributes.iter().find(|attr| attr.name == name)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MetricSpec {
    pub name: String,
    pub kind: MetricKind,
    pub unit: String,
    pub priority: Priority,
    #[serde(default)]
    pub attributes: Vec<String>,
    #[serde(default)]
    pub temporality: Option<Temporality>,
    #[serde(default)]
    pub series_cap: Option<usize>,
}

impl MetricSpec {
    pub fn series_cap(&self) -> usize {
        self.series_cap.unwrap_or(DEFAULT_SERIES_CAP)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Counter,
    Histogram,
    ObservableGauge,
    LogEvent,
    Derived,
}

impl MetricKind {
    pub const fn is_metric(self) -> bool {
        matches!(self, Self::Counter | Self::Histogram | Self::ObservableGauge)
    }

    pub const fn requires_temporality(self) -> bool {
        matches!(self, Self::Counter | Self::Histogram | Self::ObservableGauge)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Priority {
    P0,
    P1,
    P2,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Temporality {
    Delta,
    Cumulative,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AttributeSpec {
    pub name: String,
    #[serde(default = "default_metric_allowed")]
    pub metric_allowed: bool,
    #[serde(default)]
    pub allowed_values: Vec<String>,
    #[serde(default)]
    pub max_distinct: Option<usize>,
}

impl AttributeSpec {
    pub fn is_closed_enum(&self) -> bool {
        !self.allowed_values.is_empty()
    }

    pub fn has_other_bucket(&self) -> bool {
        self.allowed_values.iter().any(|value| value == "_other_")
    }

    pub fn can_overflow_to_other(&self) -> bool {
        self.max_distinct.is_some() || self.has_other_bucket()
    }

    pub fn cardinality_budget(&self) -> Option<usize> {
        if self.is_closed_enum() {
            Some(self.allowed_values.len())
        } else {
            self.max_distinct
        }
    }

    pub fn accepts_value(&self, value: &str) -> bool {
        self.allowed_values.is_empty() || self.allowed_values.iter().any(|allowed| allowed == value)
    }
}

fn default_metric_allowed() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LegacyMappings {
    pub mappings: Vec<LegacyMapping>,
    pub critical_alarms: Vec<CriticalAlarm>,
}

impl LegacyMappings {
    pub fn parse() -> Result<Self, SchemaError> {
        serde_yaml::from_str(LEGACY_MAPPING_YAML).map_err(|err| SchemaError::Parse(err.to_string()))
    }

    pub fn validate_against(&self, registry: &Registry) -> Result<(), SchemaError> {
        let mut mapped_events = HashSet::new();
        for mapping in &self.mappings {
            if !mapped_events.insert(mapping.event_type.as_str()) {
                return Err(SchemaError::DuplicateLegacyEvent(mapping.event_type.clone()));
            }

            if registry.metric(&mapping.otel_metric).is_none() {
                return Err(SchemaError::Parse(format!(
                    "legacy mapping for {} references unknown metric {}",
                    mapping.event_type, mapping.otel_metric
                )));
            }
        }

        for event_type in LegacyEventType::ALL {
            if !mapped_events.contains(event_type.as_str()) {
                return Err(SchemaError::MissingLegacyEvent(event_type.as_str().to_string()));
            }
        }

        for alarm in &self.critical_alarms {
            if registry.metric(&alarm.otel_metric).is_none() {
                return Err(SchemaError::Parse(format!(
                    "critical alarm {} references unknown metric {}",
                    alarm.name, alarm.otel_metric
                )));
            }
        }

        Ok(())
    }

    pub fn metric_for_event(&self, event_type: LegacyEventType) -> Option<&str> {
        self.mappings
            .iter()
            .find(|mapping| mapping.event_type == event_type.as_str())
            .map(|mapping| mapping.otel_metric.as_str())
    }

    pub fn parity_tolerance_for_event(&self, event_type: LegacyEventType) -> Option<ParityTolerance> {
        self.mappings
            .iter()
            .find(|mapping| mapping.event_type == event_type.as_str())
            .map(|mapping| mapping.parity_tolerance)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LegacyMapping {
    pub event_type: String,
    pub legacy_metric: String,
    pub otel_metric: String,
    pub parity_tolerance: ParityTolerance,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CriticalAlarm {
    pub name: String,
    pub legacy_metric: String,
    pub otel_metric: String,
    pub parity_tolerance: ParityTolerance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ParityTolerance {
    HighVolume,
    LowVolume,
    DailyAggregate,
}

impl ParityTolerance {
    pub const fn relative_threshold(self) -> f64 {
        match self {
            Self::HighVolume => 0.02,
            Self::LowVolume => 0.05,
            Self::DailyAggregate => 0.01,
        }
    }

    pub fn relative_drift(self, legacy_value: f64, otel_value: f64) -> f64 {
        let baseline = legacy_value.abs();
        if baseline == 0.0 {
            if otel_value == 0.0 { 0.0 } else { f64::INFINITY }
        } else {
            (legacy_value - otel_value).abs() / baseline
        }
    }

    pub fn is_within_threshold(self, legacy_value: f64, otel_value: f64) -> bool {
        self.relative_drift(legacy_value, otel_value) <= self.relative_threshold()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegacyEventType {
    UserLoggedIn,
    AuthFailed,
    RefreshCredentials,
    CliSubcommandExecuted,
    ChatSlashCommandExecuted,
    ChatStart,
    ChatEnd,
    ChatAddedMessage,
    RecordUserTurnCompletion,
    TangentModeSession,
    ToolUseSuggested,
    AgentContribution,
    McpServerInit,
    AgentConfigInit,
    DidSelectProfile,
    ProfileState,
    MessageResponseError,
    DailyHeartbeat,
    SubagentInvocation,
    VoiceInput,
    ProcessHealthMetric,
    ModeChanged,
    GoalCompleted,
}

impl LegacyEventType {
    pub const ALL: &'static [Self] = &[
        Self::UserLoggedIn,
        Self::AuthFailed,
        Self::RefreshCredentials,
        Self::CliSubcommandExecuted,
        Self::ChatSlashCommandExecuted,
        Self::ChatStart,
        Self::ChatEnd,
        Self::ChatAddedMessage,
        Self::RecordUserTurnCompletion,
        Self::TangentModeSession,
        Self::ToolUseSuggested,
        Self::AgentContribution,
        Self::McpServerInit,
        Self::AgentConfigInit,
        Self::DidSelectProfile,
        Self::ProfileState,
        Self::MessageResponseError,
        Self::DailyHeartbeat,
        Self::SubagentInvocation,
        Self::VoiceInput,
        Self::ProcessHealthMetric,
        Self::ModeChanged,
        Self::GoalCompleted,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserLoggedIn => "UserLoggedIn",
            Self::AuthFailed => "AuthFailed",
            Self::RefreshCredentials => "RefreshCredentials",
            Self::CliSubcommandExecuted => "CliSubcommandExecuted",
            Self::ChatSlashCommandExecuted => "ChatSlashCommandExecuted",
            Self::ChatStart => "ChatStart",
            Self::ChatEnd => "ChatEnd",
            Self::ChatAddedMessage => "ChatAddedMessage",
            Self::RecordUserTurnCompletion => "RecordUserTurnCompletion",
            Self::TangentModeSession => "TangentModeSession",
            Self::ToolUseSuggested => "ToolUseSuggested",
            Self::AgentContribution => "AgentContribution",
            Self::McpServerInit => "McpServerInit",
            Self::AgentConfigInit => "AgentConfigInit",
            Self::DidSelectProfile => "DidSelectProfile",
            Self::ProfileState => "ProfileState",
            Self::MessageResponseError => "MessageResponseError",
            Self::DailyHeartbeat => "DailyHeartbeat",
            Self::SubagentInvocation => "SubagentInvocation",
            Self::VoiceInput => "VoiceInput",
            Self::ProcessHealthMetric => "ProcessHealthMetric",
            Self::ModeChanged => "ModeChanged",
            Self::GoalCompleted => "GoalCompleted",
        }
    }
}

pub fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry::parse().expect("telemetry schema must be valid"))
}

pub fn legacy_mappings() -> &'static LegacyMappings {
    static MAPPINGS: OnceLock<LegacyMappings> = OnceLock::new();
    MAPPINGS.get_or_init(|| LegacyMappings::parse().expect("telemetry legacy mapping must be valid"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{
        Path,
        PathBuf,
    };

    use super::*;

    #[test]
    fn registry_loads_and_validates() {
        let registry = Registry::parse().expect("schema should load");

        assert!(registry.metric("cli_session_started_total").is_some());
        assert!(registry.metric("telemetry.emit.failures").is_some());
        assert!(
            registry
                .attribute("anonymous_client_id")
                .is_some_and(|attr| !attr.metric_allowed)
        );
    }

    #[test]
    fn all_metric_instruments_have_temporality() {
        let registry = Registry::parse().expect("schema should load");

        for metric in registry.metrics.iter().filter(|metric| metric.kind.is_metric()) {
            assert!(metric.temporality.is_some(), "{} must declare temporality", metric.name);
        }
    }

    #[test]
    fn dangerous_high_cardinality_fields_are_not_metric_dimensions() {
        let registry = Registry::parse().expect("schema should load");
        let forbidden = [
            "anonymous_client_id",
            "session_id",
            "conversation_id",
            "request_id",
            "tool_use_id",
            "message_id",
            "panic_location",
            "host_id_hash",
            "reason_desc",
            "error_message",
        ];

        for metric in registry.metrics.iter().filter(|metric| metric.kind.is_metric()) {
            for attribute in &metric.attributes {
                assert!(
                    !forbidden.contains(&attribute.as_str()),
                    "{} must not use high-cardinality attribute {}",
                    metric.name,
                    attribute
                );
            }
        }
    }

    #[test]
    fn legacy_mapping_names_the_five_sev_alarms() {
        let mappings = LegacyMappings::parse().expect("legacy mappings should load");
        mappings
            .validate_against(&Registry::parse().expect("schema should load"))
            .expect("legacy mappings should reference known metrics");
        let alarm_names: HashSet<&str> = mappings
            .critical_alarms
            .iter()
            .map(|alarm| alarm.name.as_str())
            .collect();

        for expected in [
            "QCLI-FirstTokenLatency",
            "QCLI-SuccessRateDown",
            "QCLIRTSCallSuccessRate",
            "QCLIFaultCount",
            "QCLIErrorCount",
        ] {
            assert!(alarm_names.contains(expected), "missing alarm mapping for {expected}");
        }
    }

    #[test]
    fn legacy_mapping_carries_parity_job_tolerances() {
        let mappings = LegacyMappings::parse().expect("legacy mappings should load");

        assert_eq!(
            mappings.parity_tolerance_for_event(LegacyEventType::ChatAddedMessage),
            Some(ParityTolerance::HighVolume)
        );
        assert_eq!(
            mappings.parity_tolerance_for_event(LegacyEventType::ModeChanged),
            Some(ParityTolerance::DailyAggregate)
        );
        assert_eq!(
            mappings
                .critical_alarms
                .iter()
                .find(|alarm| alarm.name == "QCLIFaultCount")
                .map(|alarm| alarm.parity_tolerance),
            Some(ParityTolerance::LowVolume)
        );

        let event_tolerances: HashSet<ParityTolerance> = mappings
            .mappings
            .iter()
            .map(|mapping| mapping.parity_tolerance)
            .collect();
        assert_eq!(
            event_tolerances,
            HashSet::from([
                ParityTolerance::HighVolume,
                ParityTolerance::LowVolume,
                ParityTolerance::DailyAggregate,
            ])
        );
    }

    #[test]
    fn parity_tolerance_thresholds_match_phase_one_gate() {
        assert_eq!(ParityTolerance::HighVolume.relative_threshold(), 0.02);
        assert_eq!(ParityTolerance::LowVolume.relative_threshold(), 0.05);
        assert_eq!(ParityTolerance::DailyAggregate.relative_threshold(), 0.01);

        assert!(ParityTolerance::HighVolume.is_within_threshold(100.0, 98.0));
        assert!(!ParityTolerance::HighVolume.is_within_threshold(100.0, 97.9));
        assert!(ParityTolerance::LowVolume.is_within_threshold(100.0, 95.0));
        assert!(!ParityTolerance::LowVolume.is_within_threshold(100.0, 94.9));
        assert!(ParityTolerance::DailyAggregate.is_within_threshold(100.0, 99.0));
        assert!(!ParityTolerance::DailyAggregate.is_within_threshold(100.0, 98.9));
        assert!(ParityTolerance::HighVolume.is_within_threshold(0.0, 0.0));
        assert!(!ParityTolerance::HighVolume.is_within_threshold(0.0, 1.0));
    }

    #[test]
    fn metric_attributes_have_bounded_series_budget() {
        let registry = Registry::parse().expect("schema should load");

        for metric in registry.metrics.iter().filter(|metric| metric.kind.is_metric()) {
            let mut product = 1usize;
            for attribute in &metric.attributes {
                let attr = registry.attribute(attribute).expect("attribute exists");
                let budget = attr.cardinality_budget().unwrap_or_else(|| {
                    panic!(
                        "{} uses unbounded attribute {}; metric attributes must be closed or max_distinct",
                        metric.name, attribute
                    )
                });
                product = product.saturating_mul(budget);
            }
            assert!(
                product <= metric.series_cap(),
                "{} worst-case series {} exceeds cap {}",
                metric.name,
                product,
                metric.series_cap()
            );
        }
    }

    #[test]
    fn log_events_can_carry_fact_row_attributes() {
        let registry = Registry::parse().expect("schema should load");
        let turn_fact = registry
            .metric("kiro_cli_user_turn_completed")
            .expect("turn completion fact row");

        assert_eq!(turn_fact.kind, MetricKind::LogEvent);
        assert!(turn_fact.attributes.len() > 10);
    }

    #[test]
    fn version_full_is_only_allowed_on_client_version_seen() {
        let registry = Registry::parse().expect("schema should load");
        let users: Vec<&str> = registry
            .metrics
            .iter()
            .filter(|metric| metric.kind.is_metric() && metric.attributes.iter().any(|attr| attr == "version_full"))
            .map(|metric| metric.name.as_str())
            .collect();

        assert_eq!(users, vec!["client_version_seen"]);
    }

    #[test]
    fn legacy_mapping_covers_v1_and_v2_event_types() {
        let mappings = LegacyMappings::parse().expect("legacy mappings should load");
        let mapped: HashSet<&str> = mappings
            .mappings
            .iter()
            .map(|mapping| mapping.event_type.as_str())
            .collect();

        assert_eq!(
            mapped.len(),
            LegacyEventType::ALL.len(),
            "each legacy EventType should appear exactly once"
        );
        for event_type in LegacyEventType::ALL {
            assert!(
                mapped.contains(event_type.as_str()),
                "missing legacy mapping for {}",
                event_type.as_str()
            );
            assert!(
                mappings.metric_for_event(*event_type).is_some(),
                "{} should resolve to an OTel metric",
                event_type.as_str()
            );
        }
    }

    #[test]
    fn product_code_uses_telemetry_facade_for_metric_emission() {
        let root = workspace_root();
        let scan_roots = [
            root.join("crates/chat-cli/src"),
            root.join("crates/chat-cli-v2/src"),
            root.join("crates/agent/src"),
            root.join("crates/kiro-telemetry/src"),
        ];
        let registry = Registry::parse().expect("schema should load");
        let resource_allowlist = resource_attribute_allowlist();

        for file in rust_files(&scan_roots) {
            let content = fs::read_to_string(&file).expect("read Rust source");
            assert!(
                !content.contains("MetricDatum::builder("),
                "{} bypasses the telemetry facade with MetricDatum::builder()",
                file.display()
            );
            assert!(
                !content.contains("attributes!("),
                "{} bypasses the telemetry schema with raw attributes!()",
                file.display()
            );

            for key in key_value_new_string_keys(&content) {
                assert!(
                    registry.attribute(&key).is_some() || resource_allowlist.contains(key.as_str()),
                    "{} uses unregistered telemetry attribute `{}`",
                    file.display(),
                    key
                );
            }
        }
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("schema crate lives under crates/")
            .to_path_buf()
    }

    fn rust_files(roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for root in roots {
            collect_rust_files(root, &mut files);
        }
        files
    }

    fn collect_rust_files(path: &Path, files: &mut Vec<PathBuf>) {
        let Ok(metadata) = fs::metadata(path) else {
            return;
        };

        if metadata.is_file() {
            if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path.to_path_buf());
            }
            return;
        }

        if !metadata.is_dir() {
            return;
        }

        for entry in fs::read_dir(path).expect("read source directory") {
            collect_rust_files(&entry.expect("read source entry").path(), files);
        }
    }

    fn key_value_new_string_keys(content: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for segment in content.split("KeyValue::new(").skip(1) {
            let trimmed = segment.trim_start();
            let Some(rest) = trimmed.strip_prefix('"') else {
                continue;
            };
            let Some(end_quote) = rest.find('"') else {
                continue;
            };
            keys.push(rest[..end_quote].to_string());
        }
        keys
    }

    fn resource_attribute_allowlist() -> HashSet<&'static str> {
        [
            "service.name",
            "service.version",
            "os.type",
            "host.arch",
            "deployment.environment",
            "partition",
            "release_channel",
            "is_internal_amazon",
            "replayed",
        ]
        .into_iter()
        .collect()
    }
}
