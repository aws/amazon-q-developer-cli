use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{
    Deserialize,
    Serialize,
};

mod command;
mod turn;
pub use command::{
    SlashCommandMetricName,
    TopLevelCommandMetricName,
};
pub use turn::TurnFailureReason;

const MAX_DASHBOARD_SERIES_PER_METRIC: usize = 10_000;
const MAX_DASHBOARD_SERIES_TOTAL: usize = 50_000;

pub const CLOUDWATCH_LEGACY_NAMESPACE: &str = "Toolkit";
pub const CLOUDWATCH_OTEL_NAMESPACE: &str = "ChatCLI";
pub const CLOUDWATCH_PRODUCT_DIMENSION: &str = "product";
pub const CLOUDWATCH_PRODUCT_VALUE: &str = "CodewhispererTerminal";

const METRICS_YAML: &str = include_str!("../schema/metrics.yaml");
const TYPES_YAML: &str = include_str!("../schema/types.yaml");
const LEGACY_MAPPING_YAML: &str = include_str!("../schema/legacy_mapping.yaml");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("failed to parse telemetry schema YAML: {0}")]
    Parse(String),
    #[error("metric `{metric}` references unknown attribute `{attribute}`")]
    UnknownAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` has {count} CloudWatch dimensions; maximum is 10")]
    TooManyCloudWatchDimensions { metric: String, count: usize },
    #[error("metric `{metric}` declares CloudWatch dimension `{dimension}` without allowing it as an attribute")]
    CloudWatchDimensionNotAttribute { metric: String, dimension: String },
    #[error("metric `{metric}` uses metric-forbidden attribute `{attribute}`")]
    ForbiddenMetricAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` is missing a temporality")]
    MissingTemporality { metric: String },
    #[error("dashboard metric `{metric}` uses open CloudWatch dimension `{dimension}` without a cardinality hint")]
    MissingCloudWatchCardinalityHint { metric: String, dimension: String },
    #[error("dashboard metric `{metric}` can create an estimated {estimated} series; maximum is {maximum}")]
    DashboardMetricSeriesBudgetExceeded {
        metric: String,
        estimated: usize,
        maximum: usize,
    },
    #[error("dashboard metrics can create an estimated {estimated} series in total; maximum is {maximum}")]
    DashboardSeriesBudgetExceeded { estimated: usize, maximum: usize },
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
        let mut estimated_dashboard_series = 0usize;

        for metric in &self.metrics {
            if metric.kind.is_metric() && metric.cloudwatch_dimensions.len() > 10 {
                return Err(SchemaError::TooManyCloudWatchDimensions {
                    metric: metric.name.clone(),
                    count: metric.cloudwatch_dimensions.len(),
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

            for dimension in &metric.cloudwatch_dimensions {
                if !metric.attributes.contains(dimension) {
                    return Err(SchemaError::CloudWatchDimensionNotAttribute {
                        metric: metric.name.clone(),
                        dimension: dimension.clone(),
                    });
                }
            }

            if metric.dashboard && metric.kind.is_metric() {
                let estimated = self.estimated_cloudwatch_series(metric)?;
                if estimated > MAX_DASHBOARD_SERIES_PER_METRIC {
                    return Err(SchemaError::DashboardMetricSeriesBudgetExceeded {
                        metric: metric.name.clone(),
                        estimated,
                        maximum: MAX_DASHBOARD_SERIES_PER_METRIC,
                    });
                }
                estimated_dashboard_series = estimated_dashboard_series.saturating_add(estimated);
            }
        }

        if estimated_dashboard_series > MAX_DASHBOARD_SERIES_TOTAL {
            return Err(SchemaError::DashboardSeriesBudgetExceeded {
                estimated: estimated_dashboard_series,
                maximum: MAX_DASHBOARD_SERIES_TOTAL,
            });
        }

        Ok(())
    }

    pub fn metric(&self, name: &str) -> Option<&MetricSpec> {
        self.metrics.iter().find(|metric| metric.name == name)
    }

    pub fn attribute(&self, name: &str) -> Option<&AttributeSpec> {
        self.attributes.iter().find(|attr| attr.name == name)
    }

    pub fn estimated_cloudwatch_series(&self, metric: &MetricSpec) -> Result<usize, SchemaError> {
        metric
            .cloudwatch_dimensions
            .iter()
            .try_fold(1usize, |estimate, dimension| {
                let attribute = self.attribute(dimension).ok_or_else(|| SchemaError::UnknownAttribute {
                    metric: metric.name.clone(),
                    attribute: dimension.clone(),
                })?;
                let cardinality = if attribute.allowed_values.is_empty() {
                    attribute.cloudwatch_cardinality_hint.ok_or_else(|| {
                        SchemaError::MissingCloudWatchCardinalityHint {
                            metric: metric.name.clone(),
                            dimension: dimension.clone(),
                        }
                    })?
                } else {
                    attribute.allowed_values.len()
                };
                Ok(estimate.saturating_mul(cardinality))
            })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MetricSpec {
    pub name: String,
    pub description: String,
    pub kind: MetricKind,
    pub unit: String,
    pub priority: Priority,
    #[serde(default)]
    pub dashboard: bool,
    #[serde(default)]
    pub attributes: Vec<String>,
    #[serde(default)]
    pub cloudwatch_dimensions: Vec<String>,
    #[serde(default)]
    pub temporality: Option<Temporality>,
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
    pub description: String,
    #[serde(default = "default_metric_allowed")]
    pub metric_allowed: bool,
    #[serde(default)]
    pub allowed_values: Vec<String>,
    #[serde(default)]
    pub cloudwatch_cardinality_hint: Option<usize>,
}

impl AttributeSpec {
    pub fn is_closed_enum(&self) -> bool {
        !self.allowed_values.is_empty()
    }

    pub fn has_other_bucket(&self) -> bool {
        self.allowed_values.iter().any(|value| value == "_other_")
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

            if let Some(metric) = &mapping.otel_metric
                && registry.metric(metric).is_none()
            {
                return Err(SchemaError::Parse(format!(
                    "legacy mapping for {} references unknown metric {}",
                    mapping.event_type, metric
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
            .and_then(|mapping| mapping.otel_metric.as_deref())
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
    pub otel_metric: Option<String>,
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

        assert_eq!(registry.metrics.len(), 54);
        assert!(registry.metric("kiro_cli_run_started_total").is_some());
        assert!(registry.metric("kiro_cli_mcp_tools_token_count_estimate").is_some());
        assert!(registry.metric("kiro_cli_telemetry_export_dropped_total").is_some());
    }

    #[test]
    fn generated_enums_match_schema_vocabularies() {
        let registry = Registry::parse().expect("schema should load");
        let slash = registry.attribute("slash_command").expect("slash command attribute");
        assert_eq!(slash.allowed_values.len(), SlashCommandMetricName::ALL.len());
        for value in &slash.allowed_values {
            assert_eq!(SlashCommandMetricName::from_name(value).as_str(), value);
        }
        for command in SlashCommandMetricName::ALL {
            assert!(
                slash.allowed_values.iter().any(|value| value == command.as_str()),
                "{} is missing from slash_command allowed_values",
                command.as_str()
            );
        }

        let top_level = registry
            .attribute("top_level_command")
            .expect("top-level command attribute");
        assert_eq!(top_level.allowed_values.len(), TopLevelCommandMetricName::ALL.len());
        for value in &top_level.allowed_values {
            assert_eq!(TopLevelCommandMetricName::from_name(value).as_str(), value);
        }
        for command in TopLevelCommandMetricName::ALL {
            assert!(
                top_level.allowed_values.iter().any(|value| value == command.as_str()),
                "{} is missing from top_level_command allowed_values",
                command.as_str()
            );
        }

        let turn_failure = registry
            .attribute("turn_failure_reason")
            .expect("turn failure reason attribute");
        assert_eq!(turn_failure.allowed_values.len(), TurnFailureReason::ALL.len());
        for value in &turn_failure.allowed_values {
            assert_eq!(TurnFailureReason::from_name(value).as_str(), value);
        }
        for reason in TurnFailureReason::ALL {
            assert!(
                turn_failure.allowed_values.iter().any(|value| value == reason.as_str()),
                "{} is missing from turn_failure_reason allowed_values",
                reason.as_str()
            );
        }
    }

    #[test]
    fn dashboard_series_budget_catches_mixed_command_vocabularies() {
        let registry = Registry::parse().expect("schema should load");
        let slash = registry
            .metric("kiro_cli_slash_command_invoked_total")
            .expect("slash command metric");
        assert_eq!(registry.estimated_cloudwatch_series(slash).unwrap(), 9_380);

        let total: usize = registry
            .metrics
            .iter()
            .filter(|metric| metric.dashboard && metric.kind.is_metric())
            .map(|metric| registry.estimated_cloudwatch_series(metric).unwrap())
            .sum();
        assert!(total <= MAX_DASHBOARD_SERIES_TOTAL, "estimated {total} series");

        let mut mixed = registry.clone();
        mixed.attributes.push(AttributeSpec {
            name: "command".to_string(),
            description: "Synthetic mixed command vocabulary".to_string(),
            metric_allowed: true,
            allowed_values: (0..80).map(|value| format!("command_{value}")).collect(),
            cloudwatch_cardinality_hint: None,
        });
        let slash = mixed
            .metrics
            .iter_mut()
            .find(|metric| metric.name == "kiro_cli_slash_command_invoked_total")
            .expect("slash command metric");
        slash.attributes.retain(|attribute| attribute != "slash_command");
        slash.attributes.push("command".to_string());
        slash
            .cloudwatch_dimensions
            .retain(|dimension| dimension != "slash_command");
        slash.cloudwatch_dimensions.push("command".to_string());

        assert_eq!(
            mixed.validate(),
            Err(SchemaError::DashboardMetricSeriesBudgetExceeded {
                metric: "kiro_cli_slash_command_invoked_total".to_string(),
                estimated: 11_200,
                maximum: MAX_DASHBOARD_SERIES_PER_METRIC,
            })
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
    fn catalog_entries_have_descriptions() {
        let registry = Registry::parse().expect("schema should load");

        for metric in &registry.metrics {
            assert!(
                !metric.description.trim().is_empty(),
                "{} is missing a description",
                metric.name
            );
        }
        for attribute in &registry.attributes {
            assert!(
                !attribute.description.trim().is_empty(),
                "{} is missing a description",
                attribute.name
            );
        }
    }

    #[test]
    fn workflow_metrics_use_bounded_dimensions() {
        let registry = Registry::parse().expect("schema should load");
        let metrics = [
            (
                "kiro_cli_workflow_run_total",
                MetricKind::Counter,
                &[
                    "version_full",
                    "workflow_run_event",
                    "workflow_topology",
                    "workflow_step_bucket",
                    "agent_engine",
                ][..],
            ),
            (
                "kiro_cli_workflow_run_duration_seconds",
                MetricKind::Histogram,
                &[
                    "version_full",
                    "workflow_outcome",
                    "workflow_topology",
                    "workflow_step_bucket",
                    "agent_engine",
                ][..],
            ),
            (
                "kiro_cli_workflow_node_total",
                MetricKind::Counter,
                &[
                    "version_full",
                    "workflow_node_type",
                    "workflow_node_outcome",
                    "agent_engine",
                ][..],
            ),
            (
                "kiro_cli_workflow_node_duration_seconds",
                MetricKind::Histogram,
                &[
                    "version_full",
                    "workflow_node_type",
                    "workflow_node_outcome",
                    "agent_engine",
                ][..],
            ),
            (
                "kiro_cli_workflow_control_total",
                MetricKind::Counter,
                &[
                    "version_full",
                    "workflow_control_action",
                    "workflow_control_result",
                    "agent_engine",
                ][..],
            ),
            (
                "kiro_cli_workflow_restore_total",
                MetricKind::Counter,
                &["version_full", "workflow_restore_result", "agent_engine"][..],
            ),
            (
                "kiro_cli_workflow_concurrent_runs",
                MetricKind::ObservableGauge,
                &["version_full", "agent_engine"][..],
            ),
        ];

        for (name, kind, attributes) in metrics {
            let metric = registry.metric(name).unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(metric.kind, kind);
            assert_eq!(metric.attributes, attributes);
            assert_eq!(metric.cloudwatch_dimensions, attributes);
            for attribute in attributes.iter().filter(|name| **name != "version_full") {
                assert!(
                    registry.attribute(attribute).is_some_and(AttributeSpec::is_closed_enum),
                    "{name}.{attribute} must be a closed enum"
                );
            }
        }

        let expected_values = [
            (
                "workflow_run_event",
                &["started", "paused", "completed", "failed", "aborted"][..],
            ),
            ("workflow_outcome", &["completed", "failed", "aborted", "_other_"][..]),
            (
                "workflow_topology",
                &["sequential", "parallel", "iterative", "watch", "mixed", "_other_"][..],
            ),
            (
                "workflow_step_bucket",
                &["1", "2", "3_5", "6_10", "11_plus", "_other_"][..],
            ),
            (
                "workflow_node_type",
                &["step", "sequence", "repeat", "parallel", "watch"][..],
            ),
            (
                "workflow_node_outcome",
                &[
                    "pending",
                    "running",
                    "paused",
                    "completed",
                    "failed",
                    "aborted",
                    "skipped",
                ][..],
            ),
            (
                "workflow_control_action",
                &["pause", "resume", "cancel", "retry", "message"][..],
            ),
            ("workflow_control_result", &["success", "failed"][..]),
            (
                "workflow_restore_result",
                &["restored", "discovery_failed", "load_failed", "rejected", "_other_"][..],
            ),
        ];

        for (name, values) in expected_values {
            let attribute = registry.attribute(name).unwrap_or_else(|| panic!("missing {name}"));
            assert_eq!(attribute.allowed_values, values);
        }
    }

    #[test]
    fn dangerous_high_cardinality_fields_are_not_metric_dimensions() {
        let registry = Registry::parse().expect("schema should load");
        let forbidden = [
            "anonymous_client_id",
            "user_id",
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
            for dimension in &metric.cloudwatch_dimensions {
                assert!(
                    !forbidden.contains(&dimension.as_str()),
                    "{} must not use high-cardinality CloudWatch dimension {}",
                    metric.name,
                    dimension
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
            mappings.parity_tolerance_for_event(LegacyEventType::UserLoggedIn),
            Some(ParityTolerance::DailyAggregate)
        );
        assert_eq!(
            mappings.parity_tolerance_for_event(LegacyEventType::DailyHeartbeat),
            Some(ParityTolerance::DailyAggregate)
        );
        assert_eq!(
            mappings.parity_tolerance_for_event(LegacyEventType::ModeChanged),
            Some(ParityTolerance::DailyAggregate)
        );
        assert_eq!(mappings.metric_for_event(LegacyEventType::ModeChanged), None);
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
    fn diagnostic_attributes_are_not_cloudwatch_dimensions() {
        let registry = Registry::parse().expect("schema should load");
        let mcp_init = registry
            .metric("kiro_cli_mcp_server_init_total")
            .expect("MCP init metric");

        assert!(mcp_init.attributes.iter().any(|attr| attr == "mcp_server_name"));
        assert!(
            !mcp_init
                .cloudwatch_dimensions
                .iter()
                .any(|dimension| dimension == "mcp_server_name")
        );
    }

    #[test]
    fn every_client_metric_has_a_version_dimension() {
        let registry = Registry::parse().expect("schema should load");

        for metric in registry.metrics.iter().filter(|metric| metric.kind.is_metric()) {
            assert!(
                metric
                    .cloudwatch_dimensions
                    .iter()
                    .any(|dimension| dimension == "version_full"),
                "{} must be selectable by exact client version",
                metric.name
            );
        }
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
        }

        assert_eq!(
            mappings.metric_for_event(LegacyEventType::ChatAddedMessage),
            Some("kiro_cli_user_turns")
        );
        assert_eq!(mappings.metric_for_event(LegacyEventType::ChatEnd), None);
    }

    #[test]
    fn product_code_uses_telemetry_facade_for_metric_emission() {
        let root = workspace_root();
        let product_roots = [
            root.join("crates/chat-cli/src"),
            root.join("crates/chat-cli-v2/src"),
            root.join("crates/agent/src"),
        ];
        let scan_roots = [
            product_roots[0].clone(),
            product_roots[1].clone(),
            product_roots[2].clone(),
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

        for file in rust_files(&product_roots) {
            let content = fs::read_to_string(&file).expect("read Rust source");
            for pattern in raw_product_telemetry_constructors() {
                assert!(
                    !content.contains(pattern),
                    "{} bypasses typed telemetry constructors with `{}`",
                    file.display(),
                    pattern
                );
            }

            let aliases = raw_telemetry_constructor_aliases(&content);
            assert!(
                aliases.is_empty(),
                "{} aliases raw telemetry constructors: {aliases:?}",
                file.display()
            );

            let record_literals = raw_telemetry_record_literals(&content);
            assert!(
                record_literals.is_empty(),
                "{} handrolls telemetry records with struct literals: {record_literals:?}",
                file.display()
            );
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
            keys.push(rest.chars().take_while(|ch| *ch != '"').collect());
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

    fn raw_product_telemetry_constructors() -> &'static [&'static str] {
        &[
            "metric::counter(",
            "metric::counter_f64(",
            "metric::histogram(",
            "metric::gauge(",
            "MetricRecord::counter(",
            "MetricRecord::counter_f64(",
            "MetricRecord::histogram(",
            "MetricRecord::gauge(",
            "MetricBuilder",
        ]
    }

    fn raw_telemetry_constructor_aliases(content: &str) -> Vec<String> {
        content
            .lines()
            .filter_map(|line| {
                let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
                if !normalized.starts_with("use ") {
                    return None;
                }

                let aliases_telemetry_crate = normalized.contains("use kiro_telemetry as ")
                    || normalized.contains("use kiro_telemetry::{self as ");

                aliases_telemetry_crate.then_some(normalized)
            })
            .collect()
    }

    fn raw_telemetry_record_literals(content: &str) -> Vec<String> {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                let uses_literal = ["MetricRecord {", "MetricRecord{"].iter().any(|literal| {
                    trimmed.starts_with(literal)
                        || trimmed.contains(&format!("= {literal}"))
                        || trimmed.contains(&format!("return {literal}"))
                        || trimmed.contains(&format!("Some({literal}"))
                        || trimmed.contains(&format!("Ok({literal}"))
                        || trimmed.contains(&format!("vec![{literal}"))
                });

                uses_literal.then_some(trimmed.to_string())
            })
            .collect()
    }

    #[test]
    fn raw_telemetry_constructor_alias_detector_flags_escape_hatches() {
        let aliases = raw_telemetry_constructor_aliases(
            r#"
use kiro_telemetry as telemetry;
use kiro_telemetry::metric as typed_metric;
"#,
        );

        assert_eq!(aliases.len(), 1);
    }

    #[test]
    fn raw_telemetry_record_literal_detector_ignores_return_types() {
        let literals = raw_telemetry_record_literals(
            r#"
fn valid_return_type() -> MetricRecord {
    metric::model_invocation(Some("claude-sonnet-4"))
}

fn invalid_literal() -> MetricRecord {
    MetricRecord { name: "raw".into(), value: MetricValue::Counter(1), timestamp_unix_millis: 0, attributes: vec![], resource_attributes: vec![] }
}
"#,
        );

        assert_eq!(literals.len(), 1);
    }
}
