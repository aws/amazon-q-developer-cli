use crate::MetricRecord;
use crate::cardinality::{
    LimitError,
    validate_metric_record,
};

mod contract;
pub use contract::*;

pub type MetricBuildError = LimitError;

#[derive(Clone, Debug)]
pub struct MetricBuilder {
    record: MetricRecord,
}

impl MetricBuilder {
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.record = self.record.with_attribute(key, value);
        self
    }

    pub fn resource_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.record = self.record.with_resource_attribute(key, value);
        self
    }

    pub fn optional_attribute(self, key: impl Into<String>, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.attribute(key, value),
            None => self,
        }
    }

    pub fn optional_resource_attribute(self, key: impl Into<String>, value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => self.resource_attribute(key, value),
            None => self,
        }
    }

    pub fn build(self) -> Result<MetricRecord, MetricBuildError> {
        validate_metric_record(&self.record)?;
        Ok(self.record)
    }

    #[track_caller]
    pub fn expect_valid(self) -> MetricRecord {
        self.build()
            .expect("telemetry metric mapping must match the canonical schema")
    }
}

pub fn counter(name: impl Into<String>, value: u64) -> MetricBuilder {
    MetricBuilder {
        record: MetricRecord::counter(name, value),
    }
}

pub fn counter_f64(name: impl Into<String>, value: f64) -> MetricBuilder {
    MetricBuilder {
        record: MetricRecord::counter_f64(name, value),
    }
}

pub fn histogram(name: impl Into<String>, value: f64) -> MetricBuilder {
    MetricBuilder {
        record: MetricRecord::histogram(name, value),
    }
}

pub fn gauge(name: impl Into<String>, value: f64) -> MetricBuilder {
    MetricBuilder {
        record: MetricRecord::gauge(name, value),
    }
}

#[track_caller]
pub fn expect_valid(builder: MetricBuilder) -> MetricRecord {
    builder.expect_valid()
}

macro_rules! impl_metric_string_serde {
    ($ty:ty, $from_name:path) => {
        impl serde::Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Ok($from_name(&value))
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientApplication {
    ChatCli,
    ChatCliV2,
    ChatCliV3,
    AcpExternal,
    KiroIde,
    Other,
}

impl ClientApplication {
    pub fn from_name(value: Option<&str>) -> Self {
        match value {
            Some("chat_cli") => Self::ChatCli,
            Some("chat_cli_v2") => Self::ChatCliV2,
            Some("chat_cli_v3" | "v3" | "kas") => Self::ChatCliV3,
            Some("acp_external") => Self::AcpExternal,
            Some("kiro_ide") => Self::KiroIde,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatCli => "chat_cli",
            Self::ChatCliV2 => "chat_cli_v2",
            Self::ChatCliV3 => "chat_cli_v3",
            Self::AcpExternal => "acp_external",
            Self::KiroIde => "kiro_ide",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsType {
    Linux,
    Macos,
    Windows,
    Other,
}

impl OsType {
    pub fn from_name(value: &str) -> Self {
        match value {
            "linux" => Self::Linux,
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Other => "unknown",
        }
    }
}

impl_metric_string_serde!(OsType, OsType::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallSource {
    Brew,
    Internal,
    Unknown,
}

impl InstallSource {
    pub fn from_name(value: &str) -> Self {
        match value {
            "brew" => Self::Brew,
            "internal" | "internal_toolbox" => Self::Internal,
            "unknown" => Self::Unknown,
            value if value.starts_with("toolbox") => Self::Internal,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Brew => "brew",
            Self::Internal => "internal_toolbox",
            Self::Unknown => "unknown",
        }
    }
}

impl_metric_string_serde!(InstallSource, InstallSource::from_name);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Interactive,
    Oneshot,
    Agent,
    Plan,
    Review,
    Tangent,
    Voice,
    AcpExternal,
    GenerateAgent,
    Other(String),
}

impl Mode {
    pub fn from_name(value: &str) -> Self {
        Self::from_chat_session(None, Some(value))
    }

    pub fn from_chat_session(app_type: Option<&str>, mode: Option<&str>) -> Self {
        if app_type == Some("ACP") {
            return Self::AcpExternal;
        }

        let normalized = mode
            .unwrap_or_default()
            .trim()
            .trim_start_matches('/')
            .to_ascii_lowercase()
            .replace('-', "_");
        match normalized.as_str() {
            "oneshot" => Self::Oneshot,
            "agent" => Self::Agent,
            "plan" | "quick_plan" | "kiro_planner" | "planner" => Self::Plan,
            "review" => Self::Review,
            "tangent" | "tangent_mode" => Self::Tangent,
            "voice" => Self::Voice,
            "acp_external" => Self::AcpExternal,
            "generate_agent" | "generateagent" => Self::GenerateAgent,
            "" | "default" | "kiro" | "vibe" | "interactive" => Self::Interactive,
            "_other_" => Self::Other("_other_".to_string()),
            _ => Self::Other(normalized),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Interactive => "interactive",
            Self::Oneshot | Self::Agent => "custom",
            Self::Plan => "plan",
            Self::Review => "review",
            Self::Tangent | Self::Voice | Self::AcpExternal | Self::GenerateAgent | Self::Other(_) => "custom",
        }
    }
}

impl_metric_string_serde!(Mode, Mode::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    Clean,
    UserInterrupt,
    Crash,
    Oom,
    HangTimeout,
    AuthFailure,
    UpstreamOutage,
    Other,
}

impl ExitReason {
    pub fn from_name(value: &str) -> Self {
        match value {
            "clean" => Self::Clean,
            "user_interrupt" => Self::UserInterrupt,
            "crash" => Self::Crash,
            "oom" => Self::Oom,
            "hang_timeout" => Self::HangTimeout,
            "auth_failure" => Self::AuthFailure,
            "upstream_outage" => Self::UpstreamOutage,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::UserInterrupt => "user_interrupt",
            Self::Crash => "crash",
            Self::Oom => "oom",
            Self::HangTimeout => "hang_timeout",
            Self::AuthFailure => "auth_failure",
            Self::UpstreamOutage => "upstream_outage",
            Self::Other => "_other_",
        }
    }
}

impl_metric_string_serde!(ExitReason, ExitReason::from_name);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentKind {
    V1,
    V2,
    Subagent,
    Kas,
    #[default]
    Other,
}

impl AgentKind {
    pub fn from_name(value: &str) -> Self {
        match value {
            "v1" => Self::V1,
            "v2" => Self::V2,
            "subagent" => Self::Subagent,
            "kas" | "v3" | "chat_cli_v3" => Self::Kas,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::Subagent => "subagent",
            Self::Kas => "kas",
            Self::Other => "_other_",
        }
    }
}

impl_metric_string_serde!(AgentKind, AgentKind::from_name);

/// Agent runtime for metrics where implementation attribution changes meaning.
/// KAS is normalized to V3 so one bounded value identifies that engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    V1,
    #[default]
    V2,
    V3,
    Unknown,
}

impl Engine {
    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "v1" | "chat_cli" => Self::V1,
            "v2" | "chat_cli_v2" => Self::V2,
            "v3" | "kas" | "chat_cli_v3" => Self::V3,
            _ => Self::Unknown,
        }
    }

    /// Coarse 2-value rollup layered on top of the finer `agent_kind`.
    pub const fn from_agent_kind(agent_kind: AgentKind) -> Self {
        match agent_kind {
            AgentKind::V1 => Self::V1,
            AgentKind::Kas => Self::V3,
            AgentKind::V2 | AgentKind::Subagent => Self::V2,
            AgentKind::Other => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V3 => "v3",
            Self::Unknown => "unknown",
        }
    }
}

impl_metric_string_serde!(Engine, Engine::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudSessionEvent {
    Started,
    StartFailed,
    Reattached,
    Ready,
    ProvisionFailed,
    Detached,
    TurnedOff,
    FellBackLocal,
    Other,
}

impl CloudSessionEvent {
    pub fn from_name(value: &str) -> Self {
        match value {
            "created" | "started" => Self::Started,
            "create_failed" | "start_failed" => Self::StartFailed,
            "reattached" => Self::Reattached,
            "ready" => Self::Ready,
            "provision_failed" => Self::ProvisionFailed,
            "detached" => Self::Detached,
            "turned_off" => Self::TurnedOff,
            "fell_back_local" => Self::FellBackLocal,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "created",
            Self::StartFailed => "create_failed",
            Self::Reattached => "reattached",
            Self::Ready => "ready",
            Self::ProvisionFailed => "provision_failed",
            Self::Detached => "detached",
            Self::TurnedOff => "turned_off",
            Self::FellBackLocal => "fell_back_local",
            Self::Other => "unknown",
        }
    }
}

impl_metric_string_serde!(CloudSessionEvent, CloudSessionEvent::from_name);

/// Process in the engine's process tree that owns a process observation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessRole {
    Host,
    #[default]
    Tui,
    KasSubprocess,
}

impl ProcessRole {
    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "host" => Self::Host,
            "kas_subprocess" => Self::KasSubprocess,
            _ => Self::Tui,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Tui => "tui",
            Self::KasSubprocess => "kas_subprocess",
        }
    }
}

impl_metric_string_serde!(ProcessRole, ProcessRole::from_name);

/// Whether a TUI render redrew the whole frame or a partial region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderKind {
    #[default]
    Partial,
    Full,
}

impl RenderKind {
    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" => Self::Full,
            _ => Self::Partial,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }
}

impl_metric_string_serde!(RenderKind, RenderKind::from_name);

/// Bounded set of UI mode values the TUI can resolve to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiMode {
    #[default]
    Tui,
    Lite,
    Unknown,
}

impl UiMode {
    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "tui" => Self::Tui,
            "lite" => Self::Lite,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Lite => "lite",
            Self::Unknown => "unknown",
        }
    }
}

impl_metric_string_serde!(UiMode, UiMode::from_name);

/// Trusted model ids remain intact so newly launched models work without a client update.
pub fn model_attr(model_id: Option<&str>) -> &str {
    match model_id {
        Some(id) if !id.trim().is_empty() => id,
        _ => "unknown",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthOperation {
    Refresh,
    Login,
}

impl AuthOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Login => "login",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Throttling,
    ContextLimit,
    Validation,
    ModelError,
    ServerError,
    Timeout,
    Connection,
    AccessDenied,
    Other,
}

impl ErrorKind {
    pub fn from_name(value: &str) -> Self {
        match value {
            "throttling" => Self::Throttling,
            "context_limit" => Self::ContextLimit,
            "validation" | "invalid_request" => Self::Validation,
            "model_error" => Self::ModelError,
            "server_error" => Self::ServerError,
            "timeout" => Self::Timeout,
            "connection" => Self::Connection,
            "access_denied" => Self::AccessDenied,
            _ => Self::Other,
        }
    }

    pub fn from_reason(reason: Option<&str>, status_code: Option<u16>) -> Self {
        let reason = reason.unwrap_or_default().to_ascii_lowercase();
        if reason.contains("throttl") || reason.contains("quota") {
            Self::Throttling
        } else if reason.contains("context") && (reason.contains("limit") || reason.contains("window")) {
            Self::ContextLimit
        } else if reason.contains("accessdenied")
            || reason.contains("access_denied")
            || reason.contains("unauthorized")
            || reason.contains("forbidden")
        {
            Self::AccessDenied
        } else if reason.contains("timeout") || reason.contains("timed out") {
            Self::Timeout
        } else if reason.contains("connection") || reason.contains("network") || reason.contains("dns") {
            Self::Connection
        } else if reason.contains("invalidmodel") || reason.contains("invalid_model") {
            Self::Validation
        } else if reason.contains("model") && !reason.contains("invalid") {
            Self::ModelError
        } else if matches!(status_code, Some(500..=599)) {
            Self::ServerError
        } else if matches!(status_code, Some(400..=499)) || reason.contains("validation") || reason.contains("invalid")
        {
            Self::Validation
        } else {
            Self::Other
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Throttling => "throttling",
            Self::ContextLimit => "context_limit",
            Self::Validation => "invalid_request",
            Self::ModelError => "model_error",
            Self::ServerError => "server_error",
            Self::Timeout => "timeout",
            Self::Connection => "connection",
            Self::AccessDenied => "access_denied",
            Self::Other => "unknown",
        }
    }
}

impl_metric_string_serde!(ErrorKind, ErrorKind::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryReason {
    Throttled,
    Timeout,
    Connection,
    EmptyResponse,
    ServerError,
    ContextRecovery,
    Other,
}

impl RetryReason {
    pub fn from_reason(value: Option<&str>) -> Self {
        let value = value.unwrap_or_default().to_ascii_lowercase();
        if value.contains("throttl") || value.contains("quota") || value.contains("rate limit") {
            Self::Throttled
        } else if value.contains("timeout") || value.contains("timed out") {
            Self::Timeout
        } else if value.contains("connection") || value.contains("network") || value.contains("dns") {
            Self::Connection
        } else if value.contains("empty") {
            Self::EmptyResponse
        } else if value.contains("context") {
            Self::ContextRecovery
        } else if value.contains("server") || value.contains("5xx") || value.contains("service") {
            Self::ServerError
        } else {
            Self::Other
        }
    }

    pub fn from_name(value: &str) -> Self {
        match value {
            "throttled" => Self::Throttled,
            "timeout" => Self::Timeout,
            "connection" => Self::Connection,
            "empty_response" => Self::EmptyResponse,
            "server_error" => Self::ServerError,
            "context_recovery" => Self::ContextRecovery,
            "other" | "invalid_response" | "_other_" => Self::Other,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Throttled => "throttled",
            Self::Timeout => "timeout",
            Self::Connection => "connection",
            Self::EmptyResponse => "empty_response",
            Self::ServerError => "server_error",
            Self::ContextRecovery => "context_recovery",
            Self::Other => "unknown",
        }
    }
}

impl_metric_string_serde!(RetryReason, RetryReason::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashKind {
    Panic,
    UncleanExit,
    Other,
}

impl CrashKind {
    pub fn from_name(value: &str) -> Self {
        match value {
            "panic" => Self::Panic,
            "unclean_exit" => Self::UncleanExit,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::UncleanExit => "unclean_exit",
            Self::Other => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldClass {
    Prompt,
    Context,
    ToolOutput,
    FileContent,
    HttpHeader,
    Other,
    OtherBucket,
}

impl FieldClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Context => "context",
            Self::ToolOutput => "tool_output",
            Self::FileContent => "file_content",
            Self::HttpHeader => "http_header",
            Self::Other => "other",
            Self::OtherBucket => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiiType {
    Email,
    AwsAccessKey,
    AwsSecret,
    Arn,
    Ipv4,
    HomePath,
    Phone,
    Jwt,
    CreditCard,
    Other,
}

impl PiiType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::AwsAccessKey => "aws_access_key",
            Self::AwsSecret => "aws_secret",
            Self::Arn => "arn",
            Self::Ipv4 => "ipv4",
            Self::HomePath => "home_path",
            Self::Phone => "phone",
            Self::Jwt => "jwt",
            Self::CreditCard => "credit_card",
            Self::Other => "_other_",
        }
    }

    pub(crate) const fn placeholder(self) -> &'static str {
        match self {
            Self::Email => "[REDACTED:email]",
            Self::AwsAccessKey => "[REDACTED:aws_access_key]",
            Self::AwsSecret => "[REDACTED:aws_secret]",
            Self::Arn => "[REDACTED:arn]",
            Self::Ipv4 => "[REDACTED:ipv4]",
            Self::HomePath => "[REDACTED:home_path]",
            Self::Phone => "[REDACTED:phone]",
            Self::Jwt => "[REDACTED:jwt]",
            Self::CreditCard => "[REDACTED:credit_card]",
            Self::Other => "[REDACTED:other]",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedactionResult {
    Scrubbed,
    Passthrough,
    Error,
    Other,
}

impl RedactionResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scrubbed => "scrubbed",
            Self::Passthrough => "passthrough",
            Self::Error => "error",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseChannel {
    Stable,
    Nightly,
    Beta,
    Other,
}

impl ReleaseChannel {
    pub fn from_version(version: &str) -> Self {
        let version = version.to_ascii_lowercase();
        if version.contains("nightly") {
            Self::Nightly
        } else if version.contains("beta") {
            Self::Beta
        } else if version.trim().is_empty() {
            Self::Other
        } else {
            Self::Stable
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Beta => "beta",
            Self::Other => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Partition {
    Aws,
    AwsUsGov,
    AwsCn,
    Other,
}

impl Partition {
    pub fn from_name(value: &str) -> Self {
        match value {
            "aws" => Self::Aws,
            "aws-us-gov" => Self::AwsUsGov,
            "aws-cn" => Self::AwsCn,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aws => "aws",
            Self::AwsUsGov => "aws-us-gov",
            Self::AwsCn => "aws-cn",
            Self::Other => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    InputUncached,
    InputCacheRead,
    InputCacheWrite,
    Output,
    Reasoning,
    Other,
}

impl TokenType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InputUncached => "input_uncached",
            Self::InputCacheRead => "input_cache_read",
            Self::InputCacheWrite => "input_cache_write",
            Self::Output => "output",
            Self::Reasoning => "reasoning",
            Self::Other => "_other_",
        }
    }
}

pub fn version_attr() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            std::env::var("KIRO_VERSION_OVERRIDE")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
        })
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_session_events_round_trip_without_relabeling_unknown_as_failure() {
        for value in ["created", "create_failed"] {
            let event = CloudSessionEvent::from_name(value);
            assert_eq!(event.as_str(), value);
        }
        assert_eq!(CloudSessionEvent::from_name("future_event").as_str(), "unknown");
        assert_eq!(serde_json::to_string(&CloudSessionEvent::Other).unwrap(), "\"unknown\"");
    }

    #[test]
    fn unexpected_ui_mode_uses_a_schema_legal_fallback() {
        let mode = UiMode::from_name("future_mode");
        assert_eq!(mode, UiMode::Unknown);
        assert_eq!(record_ui_mode_session_started(mode).attributes[1].value, "unknown");
    }

    #[test]
    fn retired_install_retry_and_crash_values_use_unknown() {
        assert_eq!(InstallSource::from_name("installation_script").as_str(), "unknown");
        assert_eq!(InstallSource::from_name("cargo").as_str(), "unknown");
        assert_eq!(RetryReason::from_name("invalid_response").as_str(), "unknown");
        assert_eq!(CrashKind::from_name("oom").as_str(), "unknown");
    }

    #[test]
    fn metric_builders_use_registered_attribute_validation() {
        let error = counter("kiro_cli_model_invocations_total", 1)
            .attribute("anonymous_client_id", "abc")
            .build()
            .expect_err("unregistered attributes must be rejected");

        assert!(matches!(error, LimitError::UnknownAttribute { .. }));
    }
}
