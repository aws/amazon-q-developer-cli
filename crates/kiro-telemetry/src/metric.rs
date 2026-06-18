use std::collections::HashSet;
use std::time::Duration;

use kiro_telemetry_schema::{
    KUTS_EXPORT_OVERSIZE_METRIC,
    LegacyEventType,
    MetricKind,
    registry,
};

use crate::{
    MetricRecord,
    PRICING_TABLE_VERSION,
    TokenUsage,
    estimate_cost_usd as estimate_model_cost_usd,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MetricBuildError {
    #[error("unknown metric `{0}`")]
    UnknownMetric(String),
    #[error("metric `{metric}` expected kind {expected:?}, got {actual:?}")]
    KindMismatch {
        metric: String,
        expected: MetricKind,
        actual: MetricKind,
    },
    #[error("metric `{metric}` does not allow attribute `{attribute}`")]
    UnsupportedAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` uses duplicate attribute `{attribute}`")]
    DuplicateAttribute { metric: String, attribute: String },
    #[error("metric `{metric}` attribute `{attribute}` does not allow value `{value}`")]
    InvalidAttributeValue {
        metric: String,
        attribute: String,
        value: String,
    },
}

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
pub enum SubagentDepthBucket {
    One,
    Two,
    ThreePlus,
    Other,
}

impl SubagentDepthBucket {
    pub const fn from_depth(depth: usize) -> Self {
        match depth {
            1 => Self::One,
            2 => Self::Two,
            3.. => Self::ThreePlus,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::ThreePlus => "3+",
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
            Self::Other => "_other_",
        }
    }
}

impl_metric_string_serde!(OsType, OsType::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallSource {
    Brew,
    Download,
    Internal,
    Cargo,
    Unknown,
    Other,
}

impl InstallSource {
    pub fn from_name(value: &str) -> Self {
        match value {
            "brew" => Self::Brew,
            "download" => Self::Download,
            "internal" => Self::Internal,
            "cargo" => Self::Cargo,
            "unknown" => Self::Unknown,
            value if value.starts_with("toolbox") => Self::Internal,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Brew => "brew",
            Self::Download => "download",
            Self::Internal => "internal",
            Self::Cargo => "cargo",
            Self::Unknown => "unknown",
            Self::Other => "_other_",
        }
    }
}

impl_metric_string_serde!(InstallSource, InstallSource::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StalenessBucket {
    Under30d,
    Days30To60,
    Days60To90,
    Over90d,
    Other,
}

impl StalenessBucket {
    /// Buckets a version's age in days into the catalog's staleness buckets.
    pub fn from_age_days(days: u32) -> Self {
        match days {
            0..=29 => Self::Under30d,
            30..=60 => Self::Days30To60,
            61..=90 => Self::Days60To90,
            _ => Self::Over90d,
        }
    }

    pub fn from_name(value: &str) -> Self {
        match value {
            "<30d" => Self::Under30d,
            "30-60" => Self::Days30To60,
            "60-90" => Self::Days60To90,
            ">90" => Self::Over90d,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Under30d => "<30d",
            Self::Days30To60 => "30-60",
            Self::Days60To90 => "60-90",
            Self::Over90d => ">90",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeTrigger {
    Auto,
    Prompted,
    Manual,
    Other,
}

impl UpgradeTrigger {
    pub fn from_name(value: &str) -> Self {
        match value {
            "auto" => Self::Auto,
            "prompted" => Self::Prompted,
            "manual" => Self::Manual,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Prompted => "prompted",
            Self::Manual => "manual",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
    Other,
}

impl Mode {
    pub fn from_name(value: &str) -> Self {
        Self::from_chat_session(None, Some(value))
    }

    pub fn from_chat_session(app_type: Option<&str>, mode: Option<&str>) -> Self {
        if app_type == Some("ACP") {
            return Self::AcpExternal;
        }

        match mode
            .unwrap_or_default()
            .trim()
            .trim_start_matches('/')
            .to_ascii_lowercase()
            .replace('-', "_")
            .as_str()
        {
            "oneshot" => Self::Oneshot,
            "agent" => Self::Agent,
            "plan" | "quick_plan" | "kiro_planner" | "planner" => Self::Plan,
            "review" => Self::Review,
            "tangent" | "tangent_mode" => Self::Tangent,
            "voice" => Self::Voice,
            "acp_external" => Self::AcpExternal,
            "generate_agent" | "generateagent" => Self::GenerateAgent,
            "" | "default" | "kiro" | "vibe" | "interactive" => Self::Interactive,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Oneshot => "oneshot",
            Self::Agent => "agent",
            Self::Plan => "plan",
            Self::Review => "review",
            Self::Tangent => "tangent",
            Self::Voice => "voice",
            Self::AcpExternal => "acp_external",
            Self::GenerateAgent => "generate_agent",
            Self::Other => "_other_",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelClass {
    AnthropicOpus,
    AnthropicSonnet,
    AnthropicHaiku,
    OpenAiGpt5,
    Other,
}

impl ModelClass {
    pub fn from_model_id(value: Option<&str>) -> Self {
        let value = value.unwrap_or_default().to_ascii_lowercase();
        if value.contains("opus") {
            Self::AnthropicOpus
        } else if value.contains("sonnet") {
            Self::AnthropicSonnet
        } else if value.contains("haiku") {
            Self::AnthropicHaiku
        } else if value.contains("gpt-5") || value.contains("gpt5") {
            Self::OpenAiGpt5
        } else {
            Self::Other
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AnthropicOpus => "anthropic_opus",
            Self::AnthropicSonnet => "anthropic_sonnet",
            Self::AnthropicHaiku => "anthropic_haiku",
            Self::OpenAiGpt5 => "openai_gpt5",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Stream,
    Invoke,
    Refresh,
    Login,
    Logout,
    Other,
}

impl Operation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stream => "stream",
            Self::Invoke => "invoke",
            Self::Refresh => "refresh",
            Self::Login => "login",
            Self::Logout => "logout",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Error,
    Denied,
    Cancelled,
    Recovered,
    StillEmpty,
    Timeout,
    Auth,
    Protocol,
    Other,
    OtherBucket,
}

impl Outcome {
    pub fn from_mcp_init_failure(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Success;
        };
        let value = value.to_ascii_lowercase();
        if value.contains("timeout") || value.contains("timed out") {
            Self::Timeout
        } else if value.contains("auth") || value.contains("unauthorized") || value.contains("forbidden") {
            Self::Auth
        } else if value.contains("protocol") || value.contains("jsonrpc") || value.contains("initialize") {
            Self::Protocol
        } else {
            Self::Other
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::Recovered => "recovered",
            Self::StillEmpty => "still_empty",
            Self::Timeout => "timeout",
            Self::Auth => "auth",
            Self::Protocol => "protocol",
            Self::Other => "other",
            Self::OtherBucket => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultKind {
    Success,
    Failed,
    Cancelled,
    Other,
}

impl ResultKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Throttling,
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
            "validation" => Self::Validation,
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
        } else if reason.contains("model") {
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
            Self::Validation => "validation",
            Self::ModelError => "model_error",
            Self::ServerError => "server_error",
            Self::Timeout => "timeout",
            Self::Connection => "connection",
            Self::AccessDenied => "access_denied",
            Self::Other => "other",
        }
    }
}

impl_metric_string_serde!(ErrorKind, ErrorKind::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upstream {
    Bedrock,
    Rts,
    Kas,
    Krs,
    Cognito,
    Other,
}

impl Upstream {
    pub fn from_name(value: &str) -> Self {
        match value {
            "bedrock" => Self::Bedrock,
            "rts" => Self::Rts,
            "kas" | "v3" | "chat_cli_v3" => Self::Kas,
            "krs" => Self::Krs,
            "cognito" => Self::Cognito,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bedrock => "bedrock",
            Self::Rts => "rts",
            Self::Kas => "kas",
            Self::Krs => "krs",
            Self::Cognito => "cognito",
            Self::Other => "_other_",
        }
    }
}

impl_metric_string_serde!(Upstream, Upstream::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryReason {
    Throttled,
    Timeout,
    Connection,
    EmptyResponse,
    ServerError,
    Other,
    OtherBucket,
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
        } else if value.contains("server") || value.contains("5xx") || value.contains("service") {
            Self::ServerError
        } else if value == "_other_" {
            Self::OtherBucket
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
            "other" => Self::Other,
            "_other_" => Self::OtherBucket,
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
            Self::Other => "other",
            Self::OtherBucket => "_other_",
        }
    }
}

impl_metric_string_serde!(RetryReason, RetryReason::from_name);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptNumberBucket {
    One,
    Two,
    ThreePlus,
    Other,
}

impl AttemptNumberBucket {
    pub const fn from_attempt(attempt: u32) -> Self {
        match attempt {
            1 => Self::One,
            2 => Self::Two,
            3.. => Self::ThreePlus,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::ThreePlus => "3+",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusClass {
    Class2xx,
    Class4xx,
    Class5xx,
    Other,
}

impl StatusClass {
    pub const fn from_status_code(status_code: Option<u16>) -> Self {
        match status_code {
            Some(200..=299) => Self::Class2xx,
            Some(400..=499) => Self::Class4xx,
            Some(500..=599) => Self::Class5xx,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Class2xx => "2xx",
            Self::Class4xx => "4xx",
            Self::Class5xx => "5xx",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptSizeBucket {
    Small,
    Medium,
    Large,
    Xlarge,
    Other,
}

impl PromptSizeBucket {
    pub const fn from_context_file_length(value: Option<usize>) -> Self {
        match value {
            Some(0..=4_000) => Self::Small,
            Some(4_001..=20_000) => Self::Medium,
            Some(20_001..=100_000) => Self::Large,
            Some(_) => Self::Xlarge,
            None => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
            Self::Xlarge => "xlarge",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolOrigin {
    Builtin,
    Mcp,
    Custom,
    SubagentDelegate,
    AwsApi,
    Other,
}

impl ToolOrigin {
    pub fn from_tool_context(tool_name: Option<&str>, aws_service_name: Option<&str>, is_custom_tool: bool) -> Self {
        if is_custom_tool {
            return Self::Mcp;
        }
        if aws_service_name.is_some() || matches!(tool_name, Some("aws" | "use_aws")) {
            return Self::AwsApi;
        }

        match tool_name {
            Some("subagent" | "use_subagent" | "agent_crew") => Self::SubagentDelegate,
            Some(_) => Self::Builtin,
            None => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Mcp => "mcp",
            Self::Custom => "custom",
            Self::SubagentDelegate => "subagent_delegate",
            Self::AwsApi => "aws_api",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpServerClass {
    BuiltinFs,
    BuiltinCode,
    BuiltinKnowledge,
    OfficialThirdParty,
    UserDefined,
    InternalAmazon,
    Other,
}

impl McpServerClass {
    pub fn from_server_name(server_name: &str) -> Self {
        let normalized = server_name.to_ascii_lowercase();
        match normalized.as_str() {
            "fs" | "filesystem" | "file-system" | "builtin_fs" => Self::BuiltinFs,
            "code" | "code-agent" | "builtin_code" => Self::BuiltinCode,
            "knowledge" | "builtin_knowledge" => Self::BuiltinKnowledge,
            name if name.starts_with("awslabs") || name.starts_with("aws-") => Self::OfficialThirdParty,
            name if name.starts_with("amzn-") || name.starts_with("amazon-") || name.contains("internal") => {
                Self::InternalAmazon
            },
            "" => Self::Other,
            _ => Self::UserDefined,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltinFs => "builtin_fs",
            Self::BuiltinCode => "builtin_code",
            Self::BuiltinKnowledge => "builtin_knowledge",
            Self::OfficialThirdParty => "official_third_party",
            Self::UserDefined => "user_defined",
            Self::InternalAmazon => "internal_amazon",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McpServerInit<'a> {
    pub server_name: &'a str,
    pub server_class: McpServerClass,
    pub outcome: Outcome,
}

impl<'a> McpServerInit<'a> {
    pub const fn new(server_name: &'a str, server_class: McpServerClass, outcome: Outcome) -> Self {
        Self {
            server_name,
            server_class,
            outcome,
        }
    }

    pub fn from_name(server_name: &'a str, init_failure_reason: Option<&'a str>) -> Self {
        Self::new(
            server_name,
            McpServerClass::from_server_name(server_name),
            Outcome::from_mcp_init_failure(init_failure_reason),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionOutcome {
    UserQuit,
    TaskCompleted,
    Error,
    Timeout,
    Crash,
    Other,
}

impl SessionOutcome {
    pub fn from_goal_terminal_state(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "completed" | "complete" | "task_completed" => Self::TaskCompleted,
            "cancelled" | "canceled" | "user_quit" => Self::UserQuit,
            "exhausted" | "timeout" | "timed_out" => Self::Timeout,
            "failed" | "error" | "reinjection_failed" => Self::Error,
            "crash" | "panic" => Self::Crash,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserQuit => "user_quit",
            Self::TaskCompleted => "task_completed",
            Self::Error => "error",
            Self::Timeout => "timeout",
            Self::Crash => "crash",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sentiment {
    Positive,
    Negative,
    Neutral,
    Other,
}

impl Sentiment {
    pub fn from_name(value: &str) -> Self {
        match value {
            "positive" => Self::Positive,
            "negative" => Self::Negative,
            "neutral" => Self::Neutral,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::Negative => "negative",
            Self::Neutral => "neutral",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackSurface {
    Chat,
    Review,
    Settings,
    Other,
}

impl FeedbackSurface {
    pub fn from_name(value: &str) -> Self {
        match value {
            "chat" => Self::Chat,
            "review" => Self::Review,
            "settings" => Self::Settings,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Review => "review",
            Self::Settings => "settings",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatConversationKind {
    Interactive,
    Oneshot,
    Subagent,
    Acp,
    Other,
}

impl ChatConversationKind {
    pub fn from_context(app_type: Option<&str>, is_subagent: bool) -> Self {
        if is_subagent {
            Self::Subagent
        } else if app_type == Some("ACP") {
            Self::Acp
        } else {
            Self::Interactive
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Oneshot => "oneshot",
            Self::Subagent => "subagent",
            Self::Acp => "acp",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionMinorBucket {
    Current,
    CurrentMinus1,
    CurrentMinus2,
    Older,
    Other,
}

impl VersionMinorBucket {
    pub fn from_version(value: &str) -> Self {
        if value.is_empty() { Self::Other } else { Self::Current }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::CurrentMinus1 => "current-1",
            Self::CurrentMinus2 => "current-2",
            Self::Older => "older",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    Streaming,
    Idle,
    ToolRunning,
    Compaction,
    Other,
}

impl ProcessState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Idle => "idle",
            Self::ToolRunning => "tool_running",
            Self::Compaction => "compaction",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostArch {
    X86_64,
    Aarch64,
    Arm64,
    Other,
}

impl HostArch {
    pub fn from_name(value: &str) -> Self {
        match value {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            "arm64" => Self::Arm64,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Arm64 => "arm64",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashKind {
    Panic,
    Segfault,
    Abort,
    UnhandledSignal,
    Other,
}

impl CrashKind {
    pub fn from_name(value: &str) -> Self {
        match value {
            "panic" => Self::Panic,
            "segfault" => Self::Segfault,
            "abort" => Self::Abort,
            "unhandled_signal" => Self::UnhandledSignal,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Segfault => "segfault",
            Self::Abort => "abort",
            Self::UnhandledSignal => "unhandled_signal",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureStage {
    Config,
    DbMigrate,
    Runtime,
    Panic,
    Other,
}

impl FailureStage {
    pub fn from_name(value: &str) -> Self {
        match value {
            "config" => Self::Config,
            "db_migrate" => Self::DbMigrate,
            "runtime" => Self::Runtime,
            "panic" => Self::Panic,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::DbMigrate => "db_migrate",
            Self::Runtime => "runtime",
            Self::Panic => "panic",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopPhase {
    ModelCall,
    ToolExec,
    Parse,
    Render,
    Other,
}

impl LoopPhase {
    pub fn from_name(value: &str) -> Self {
        match value {
            "model_call" => Self::ModelCall,
            "tool_exec" => Self::ToolExec,
            "parse" => Self::Parse,
            "render" => Self::Render,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelCall => "model_call",
            Self::ToolExec => "tool_exec",
            Self::Parse => "parse",
            Self::Render => "render",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StuckPhase {
    ModelCall,
    ToolExec,
    Parse,
    Render,
    Unknown,
    Other,
}

impl StuckPhase {
    pub fn from_name(value: &str) -> Self {
        match value {
            "model_call" => Self::ModelCall,
            "tool_exec" => Self::ToolExec,
            "parse" => Self::Parse,
            "render" => Self::Render,
            "unknown" => Self::Unknown,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelCall => "model_call",
            Self::ToolExec => "tool_exec",
            Self::Parse => "parse",
            Self::Render => "render",
            Self::Unknown => "unknown",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StuckDetection {
    Watchdog,
    Heartbeat,
    Other,
}

impl StuckDetection {
    pub fn from_name(value: &str) -> Self {
        match value {
            "watchdog" => Self::Watchdog,
            "heartbeat" => Self::Heartbeat,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Watchdog => "watchdog",
            Self::Heartbeat => "heartbeat",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dependency {
    Bedrock,
    Rts,
    Kas,
    Krs,
    Cognito,
    OauthIdp,
    Other,
}

impl Dependency {
    pub fn from_name(value: &str) -> Self {
        match value {
            "bedrock" => Self::Bedrock,
            "rts" => Self::Rts,
            "kas" => Self::Kas,
            "krs" => Self::Krs,
            "cognito" => Self::Cognito,
            "oauth_idp" => Self::OauthIdp,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bedrock => "bedrock",
            Self::Rts => "rts",
            Self::Kas => "kas",
            Self::Krs => "krs",
            Self::Cognito => "cognito",
            Self::OauthIdp => "oauth_idp",
            Self::Other => "_other_",
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
pub enum Redactor {
    Default,
    Other,
}

impl Redactor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedactionFailAction {
    Dropped,
    Passthrough,
    Other,
}

impl RedactionFailAction {
    pub fn from_name(value: &str) -> Self {
        match value {
            "dropped" => Self::Dropped,
            "passthrough" => Self::Passthrough,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dropped => "dropped",
            Self::Passthrough => "passthrough",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentCheckKind {
    Hash,
    Perms,
    Owner,
    Signature,
    Other,
}

impl ConsentCheckKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hash => "hash",
            Self::Perms => "perms",
            Self::Owner => "owner",
            Self::Signature => "signature",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentIntegrityResult {
    Ok,
    Tampered,
    Missing,
    Unreadable,
    Other,
}

impl ConsentIntegrityResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Tampered => "tampered",
            Self::Missing => "missing",
            Self::Unreadable => "unreadable",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventClass {
    Metric,
    Log,
    Audit,
    LegacyEvent,
}

impl EventClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::Log => "log",
            Self::Audit => "audit",
            Self::LegacyEvent => "legacy_event",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryChannel {
    LegacyToolkit,
    LegacyCodewhisperer,
    Otel,
    MetaMeter,
    Other,
}

impl TelemetryChannel {
    pub fn from_name(value: &str) -> Self {
        match value {
            "legacy_toolkit" => Self::LegacyToolkit,
            "legacy_codewhisperer" => Self::LegacyCodewhisperer,
            "otel" => Self::Otel,
            "meta_meter" => Self::MetaMeter,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyToolkit => "legacy_toolkit",
            Self::LegacyCodewhisperer => "legacy_codewhisperer",
            Self::Otel => "otel",
            Self::MetaMeter => "meta_meter",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryExporter {
    LegacyToolkit,
    LegacyCodewhisperer,
    Otel,
    MetaMeter,
    Other,
}

impl TelemetryExporter {
    pub fn from_name(value: &str) -> Self {
        match value {
            "legacy_toolkit" => Self::LegacyToolkit,
            "legacy_codewhisperer" => Self::LegacyCodewhisperer,
            "otel" => Self::Otel,
            "meta_meter" => Self::MetaMeter,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyToolkit => "legacy_toolkit",
            Self::LegacyCodewhisperer => "legacy_codewhisperer",
            Self::Otel => "otel",
            Self::MetaMeter => "meta_meter",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetrySignal {
    Metrics,
    Logs,
    Traces,
    Other,
}

impl TelemetrySignal {
    pub fn from_name(value: &str) -> Self {
        match value {
            "metrics" => Self::Metrics,
            "logs" => Self::Logs,
            "traces" => Self::Traces,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metrics => "metrics",
            Self::Logs => "logs",
            Self::Traces => "traces",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportOutcome {
    Success,
    Retry,
    Dropped,
    PermanentFailure,
    Other,
}

impl ExportOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Retry => "retry",
            Self::Dropped => "dropped",
            Self::PermanentFailure => "permanent_failure",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropReason {
    QueueFull,
    Timeout,
    OptOut,
    Cardinality,
    Redaction,
    ExporterError,
    Oversize,
    Other,
}

impl DropReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QueueFull => "queue_full",
            Self::Timeout => "timeout",
            Self::OptOut => "opt_out",
            Self::Cardinality => "cardinality",
            Self::Redaction => "redaction",
            Self::ExporterError => "exporter_error",
            Self::Oversize => "oversize",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetrySubsystem {
    Schema,
    Limiter,
    Wal,
    Exporter,
    Redactor,
    Config,
    Other,
}

impl TelemetrySubsystem {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Limiter => "limiter",
            Self::Wal => "wal",
            Self::Exporter => "exporter",
            Self::Redactor => "redactor",
            Self::Config => "config",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitFailureKind {
    InvalidSchema,
    Io,
    Serialization,
    UnknownMetric,
    UnknownAttribute,
    Other,
}

impl EmitFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSchema => "invalid_schema",
            Self::Io => "io",
            Self::Serialization => "serialization",
            Self::UnknownMetric => "unknown_metric",
            Self::UnknownAttribute => "unknown_attribute",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseChannel {
    Stable,
    Beta,
    Other,
}

impl ReleaseChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Other => "_other_",
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
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownPath {
    Clean,
    Signal,
    Panic,
    Other,
}

impl ShutdownPath {
    pub fn from_name(value: &str) -> Self {
        match value {
            "clean" => Self::Clean,
            "signal" => Self::Signal,
            "panic" => Self::Panic,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Signal => "signal",
            Self::Panic => "panic",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostureReason {
    GovcloudDisabled,
    Other,
}

impl PostureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GovcloudDisabled => "govcloud_disabled",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthProvider {
    BuilderId,
    Cognito,
    Sso,
    Other,
}

impl AuthProvider {
    pub fn from_name(value: &str) -> Self {
        match value {
            "builder_id" => Self::BuilderId,
            "cognito" => Self::Cognito,
            "sso" => Self::Sso,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuilderId => "builder_id",
            Self::Cognito => "cognito",
            Self::Sso => "sso",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsFailureReason {
    CertExpired,
    BadHostname,
    UntrustedRoot,
    Protocol,
    Other,
}

impl TlsFailureReason {
    pub fn from_name(value: &str) -> Self {
        match value {
            "cert_expired" => Self::CertExpired,
            "bad_hostname" => Self::BadHostname,
            "untrusted_root" => Self::UntrustedRoot,
            "protocol" => Self::Protocol,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CertExpired => "cert_expired",
            Self::BadHostname => "bad_hostname",
            Self::UntrustedRoot => "untrusted_root",
            Self::Protocol => "protocol",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestinationClass {
    PublicInternet,
    AwsEndpoint,
    InternalAmzn,
    Localhost,
    Other,
}

impl DestinationClass {
    pub fn from_name(value: &str) -> Self {
        match value {
            "public_internet" => Self::PublicInternet,
            "aws_endpoint" => Self::AwsEndpoint,
            "internal_amzn" => Self::InternalAmzn,
            "localhost" => Self::Localhost,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PublicInternet => "public_internet",
            Self::AwsEndpoint => "aws_endpoint",
            Self::InternalAmzn => "internal_amzn",
            Self::Localhost => "localhost",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlScheme {
    Http,
    Https,
    Ws,
    Wss,
    Other,
}

impl UrlScheme {
    pub fn from_name(value: &str) -> Self {
        match value {
            "http" => Self::Http,
            "https" => Self::Https,
            "ws" => Self::Ws,
            "wss" => Self::Wss,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Ws => "ws",
            Self::Wss => "wss",
            Self::Other => "_other_",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GovcloudChannelDisabled {
    pub channel: TelemetryChannel,
    pub partition: Partition,
    pub reason: PostureReason,
}

impl GovcloudChannelDisabled {
    pub const fn new(channel: TelemetryChannel, partition: Partition, reason: PostureReason) -> Self {
        Self {
            channel,
            partition,
            reason,
        }
    }

    pub fn from_names(channel: &str, partition: &str) -> Self {
        Self::new(
            TelemetryChannel::from_name(channel),
            Partition::from_name(partition),
            PostureReason::GovcloudDisabled,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GovcloudChannelLeak {
    pub channel: TelemetryChannel,
}

impl GovcloudChannelLeak {
    pub const fn new(channel: TelemetryChannel) -> Self {
        Self { channel }
    }

    pub fn from_name(channel: &str) -> Self {
        Self::new(TelemetryChannel::from_name(channel))
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

impl Mode {
    pub fn from_message_context(app_type: Option<&str>, is_tangent: bool, is_generate_agent: bool) -> Self {
        if is_tangent {
            Self::Tangent
        } else if is_generate_agent {
            Self::GenerateAgent
        } else if app_type == Some("ACP") {
            Self::AcpExternal
        } else {
            Self::Interactive
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvocationContext {
    pub model_class: ModelClass,
    pub client_application: ClientApplication,
    pub is_subagent: bool,
}

impl InvocationContext {
    pub const fn new(model_class: ModelClass, client_application: ClientApplication, is_subagent: bool) -> Self {
        Self {
            model_class,
            client_application,
            is_subagent,
        }
    }

    pub fn from_names(model_id: Option<&str>, client_application: Option<&str>, is_subagent: bool) -> Self {
        Self::new(
            ModelClass::from_model_id(model_id),
            ClientApplication::from_name(client_application),
            is_subagent,
        )
    }

    pub fn estimated_cost_usd(self, usage: TokenUsage) -> Option<f64> {
        estimate_model_cost_usd(self.model_class.as_str(), usage)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

impl TurnOutcome {
    pub const fn result_kind(self) -> ResultKind {
        match self {
            Self::Succeeded => ResultKind::Success,
            Self::Failed => ResultKind::Failed,
            Self::Cancelled => ResultKind::Cancelled,
        }
    }

    pub const fn request_outcome(self) -> Outcome {
        match self {
            Self::Succeeded => Outcome::Success,
            Self::Failed => Outcome::Error,
            Self::Cancelled => Outcome::Cancelled,
        }
    }

    pub const fn completion_reason(self, tools_enabled: bool) -> crate::log::CompletionReason {
        match self {
            Self::Succeeded if tools_enabled => crate::log::CompletionReason::ToolUse,
            Self::Succeeded => crate::log::CompletionReason::Stop,
            Self::Failed => crate::log::CompletionReason::Error,
            Self::Cancelled => crate::log::CompletionReason::Cancelled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurnMetricContext<'a> {
    pub model_id: Option<&'a str>,
    pub client_application: Option<&'a str>,
    pub app_type: Option<&'a str>,
    pub is_subagent: bool,
    pub is_tangent: bool,
    pub is_generate_agent: bool,
}

impl<'a> TurnMetricContext<'a> {
    pub const fn new(model_id: Option<&'a str>, client_application: Option<&'a str>) -> Self {
        Self {
            model_id,
            client_application,
            app_type: None,
            is_subagent: false,
            is_tangent: false,
            is_generate_agent: false,
        }
    }

    pub const fn app_type(mut self, app_type: Option<&'a str>) -> Self {
        self.app_type = app_type;
        self
    }

    pub const fn subagent(mut self, is_subagent: bool) -> Self {
        self.is_subagent = is_subagent;
        self
    }

    pub const fn tangent(mut self, is_tangent: bool) -> Self {
        self.is_tangent = is_tangent;
        self
    }

    pub const fn generate_agent(mut self, is_generate_agent: bool) -> Self {
        self.is_generate_agent = is_generate_agent;
        self
    }

    pub fn invocation_context(self) -> InvocationContext {
        InvocationContext::from_names(self.model_id, self.client_application, self.is_subagent)
    }

    pub fn mode(self) -> Mode {
        Mode::from_message_context(self.app_type, self.is_tangent, self.is_generate_agent)
    }

    pub fn conversation_type(self) -> ChatConversationKind {
        ChatConversationKind::from_context(self.app_type, self.is_subagent)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextUsageMetric {
    pub percentage: f64,
    pub context: InvocationContext,
}

impl ContextUsageMetric {
    pub const fn new(percentage: f64, context: InvocationContext) -> Self {
        Self { percentage, context }
    }

    pub fn from_names(
        percentage: f64,
        model_id: Option<&str>,
        client_application: Option<&str>,
        is_subagent: bool,
    ) -> Self {
        Self::new(
            percentage,
            InvocationContext::from_names(model_id, client_application, is_subagent),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolInvocation<'a> {
    pub tool_name: Option<&'a str>,
    pub aws_service_name: Option<&'a str>,
    pub is_custom_tool: bool,
    pub is_accepted: bool,
    pub is_valid: Option<bool>,
    pub is_success: Option<bool>,
}

impl<'a> ToolInvocation<'a> {
    pub const fn new(
        tool_name: Option<&'a str>,
        aws_service_name: Option<&'a str>,
        is_custom_tool: bool,
        is_accepted: bool,
        is_valid: Option<bool>,
        is_success: Option<bool>,
    ) -> Self {
        Self {
            tool_name,
            aws_service_name,
            is_custom_tool,
            is_accepted,
            is_valid,
            is_success,
        }
    }

    pub fn origin(self) -> ToolOrigin {
        ToolOrigin::from_tool_context(self.tool_name, self.aws_service_name, self.is_custom_tool)
    }

    pub fn outcome(self) -> Outcome {
        if !self.is_accepted {
            Outcome::Denied
        } else if self.is_valid == Some(false) {
            Outcome::Error
        } else {
            match self.is_success {
                Some(true) => Outcome::Success,
                Some(false) => Outcome::Error,
                None => Outcome::Cancelled,
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToolUseMetrics<'a> {
    pub emit_tool_call_total: bool,
    pub invocation: ToolInvocation<'a>,
    pub execution_duration_ms: Option<f64>,
}

impl<'a> ToolUseMetrics<'a> {
    pub const fn new(invocation: ToolInvocation<'a>) -> Self {
        Self {
            emit_tool_call_total: false,
            invocation,
            execution_duration_ms: None,
        }
    }

    pub const fn from_tool_context(
        tool_name: Option<&'a str>,
        aws_service_name: Option<&'a str>,
        is_custom_tool: bool,
        is_accepted: bool,
        is_valid: Option<bool>,
        is_success: Option<bool>,
    ) -> Self {
        Self::new(ToolInvocation::new(
            tool_name,
            aws_service_name,
            is_custom_tool,
            is_accepted,
            is_valid,
            is_success,
        ))
    }

    pub fn legacy_event(mut self, event_type: Option<LegacyEventType>) -> Self {
        self.emit_tool_call_total = event_type.is_some_and(crate::legacy::emits_legacy_tool_call_total);
        self
    }

    pub const fn emit_tool_call_total(mut self, emit_tool_call_total: bool) -> Self {
        self.emit_tool_call_total = emit_tool_call_total;
        self
    }

    pub fn execution_duration(mut self, execution_duration: Option<Duration>) -> Self {
        self.execution_duration_ms = execution_duration.map(|duration| duration.as_secs_f64() * 1000.0);
        self
    }

    pub const fn execution_duration_ms(mut self, execution_duration_ms: Option<f64>) -> Self {
        self.execution_duration_ms = execution_duration_ms;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BedrockStreamMetrics<'a> {
    pub model_id: Option<&'a str>,
    pub context_file_length: Option<usize>,
    pub tools_enabled: bool,
    pub time_to_first_chunk_ms: Option<f64>,
    pub time_between_chunks_ms: Option<&'a [f64]>,
    pub request_duration_seconds: Option<f64>,
    pub completion_reason: crate::log::CompletionReason,
    pub request_outcome: Outcome,
}

impl<'a> BedrockStreamMetrics<'a> {
    pub const fn new(
        model_id: Option<&'a str>,
        completion_reason: crate::log::CompletionReason,
        request_outcome: Outcome,
    ) -> Self {
        Self {
            model_id,
            context_file_length: None,
            tools_enabled: false,
            time_to_first_chunk_ms: None,
            time_between_chunks_ms: None,
            request_duration_seconds: None,
            completion_reason,
            request_outcome,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ModelResponseMetrics<'a> {
    pub emit_user_turn_counter: bool,
    pub context: InvocationContext,
    pub result: ResultKind,
    pub mode: Mode,
    pub conversation_type: ChatConversationKind,
    pub context_file_length: Option<usize>,
    pub tools_enabled: bool,
    pub time_to_first_chunk_ms: Option<f64>,
    pub time_between_chunks_ms: Option<&'a [f64]>,
    pub request_duration_seconds: Option<f64>,
    pub completion_reason: crate::log::CompletionReason,
    pub request_outcome: Outcome,
    pub token_usage: TokenUsage,
}

impl<'a> ModelResponseMetrics<'a> {
    pub fn new(
        context: InvocationContext,
        result: ResultKind,
        mode: Mode,
        conversation_type: ChatConversationKind,
        completion_reason: crate::log::CompletionReason,
        request_outcome: Outcome,
    ) -> Self {
        Self {
            emit_user_turn_counter: false,
            context,
            result,
            mode,
            conversation_type,
            context_file_length: None,
            tools_enabled: false,
            time_to_first_chunk_ms: None,
            time_between_chunks_ms: None,
            request_duration_seconds: None,
            completion_reason,
            request_outcome,
            token_usage: TokenUsage::default(),
        }
    }

    pub fn from_turn_context(context: TurnMetricContext<'a>, outcome: TurnOutcome, tools_enabled: bool) -> Self {
        let mut metrics = Self::new(
            context.invocation_context(),
            outcome.result_kind(),
            context.mode(),
            context.conversation_type(),
            outcome.completion_reason(tools_enabled),
            outcome.request_outcome(),
        );
        metrics.tools_enabled = tools_enabled;
        metrics
    }

    pub fn legacy_event(mut self, event_type: Option<LegacyEventType>) -> Self {
        self.emit_user_turn_counter = event_type.is_some_and(crate::legacy::emits_legacy_user_turn_counter);
        self
    }

    pub const fn emit_user_turn_counter(mut self, emit_user_turn_counter: bool) -> Self {
        self.emit_user_turn_counter = emit_user_turn_counter;
        self
    }

    pub const fn context_file_length(mut self, context_file_length: Option<usize>) -> Self {
        self.context_file_length = context_file_length;
        self
    }

    pub const fn time_to_first_chunk_ms(mut self, time_to_first_chunk_ms: Option<f64>) -> Self {
        self.time_to_first_chunk_ms = time_to_first_chunk_ms;
        self
    }

    pub const fn time_between_chunks_ms(mut self, time_between_chunks_ms: Option<&'a [f64]>) -> Self {
        self.time_between_chunks_ms = time_between_chunks_ms;
        self
    }

    pub const fn request_duration_seconds(mut self, request_duration_seconds: Option<f64>) -> Self {
        self.request_duration_seconds = request_duration_seconds;
        self
    }

    pub const fn token_usage(mut self, token_usage: TokenUsage) -> Self {
        self.token_usage = token_usage;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UserTurnCompletionMetrics {
    pub emit_user_turn_counter: bool,
    pub context: InvocationContext,
    pub result: ResultKind,
    pub mode: Mode,
    pub conversation_type: ChatConversationKind,
    pub token_usage: TokenUsage,
    pub estimated_cost_usd: Option<f64>,
    pub duration_seconds: Option<f64>,
}

impl UserTurnCompletionMetrics {
    pub fn new(
        context: InvocationContext,
        result: ResultKind,
        mode: Mode,
        conversation_type: ChatConversationKind,
    ) -> Self {
        Self {
            emit_user_turn_counter: false,
            context,
            result,
            mode,
            conversation_type,
            token_usage: TokenUsage::default(),
            estimated_cost_usd: None,
            duration_seconds: None,
        }
    }

    pub fn from_turn_context(context: TurnMetricContext<'_>, outcome: TurnOutcome) -> Self {
        Self::new(
            context.invocation_context(),
            outcome.result_kind(),
            context.mode(),
            context.conversation_type(),
        )
    }

    pub fn legacy_event(mut self, event_type: Option<LegacyEventType>) -> Self {
        self.emit_user_turn_counter = event_type.is_some_and(crate::legacy::emits_legacy_user_turn_counter);
        self
    }

    pub const fn emit_user_turn_counter(mut self, emit_user_turn_counter: bool) -> Self {
        self.emit_user_turn_counter = emit_user_turn_counter;
        self
    }

    pub const fn token_usage(mut self, token_usage: TokenUsage) -> Self {
        self.token_usage = token_usage;
        self
    }

    pub const fn estimated_cost_usd(mut self, estimated_cost_usd: Option<f64>) -> Self {
        self.estimated_cost_usd = estimated_cost_usd;
        self
    }

    pub const fn duration_seconds(mut self, duration_seconds: Option<f64>) -> Self {
        self.duration_seconds = duration_seconds;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessHealthSnapshot<'a> {
    pub rss_mb: f64,
    pub cpu_user_pct: f64,
    pub cpu_system_pct: f64,
    pub version: &'a str,
    pub agent_kind: AgentKind,
    pub state: ProcessState,
}

impl<'a> ProcessHealthSnapshot<'a> {
    pub const fn new(
        rss_mb: f64,
        cpu_user_pct: f64,
        cpu_system_pct: f64,
        version: &'a str,
        agent_kind: AgentKind,
        state: ProcessState,
    ) -> Self {
        Self {
            rss_mb,
            cpu_user_pct,
            cpu_system_pct,
            version,
            agent_kind,
            state,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CliSessionStarted {
    pub os_type: OsType,
    pub install_source: InstallSource,
    pub client_application: ClientApplication,
}

impl CliSessionStarted {
    pub const fn new(os_type: OsType, install_source: InstallSource, client_application: ClientApplication) -> Self {
        Self {
            os_type,
            install_source,
            client_application,
        }
    }

    pub fn from_names(os_type: &str, install_source: &str, client_application: Option<&str>) -> Self {
        Self::new(
            OsType::from_name(os_type),
            InstallSource::from_name(install_source),
            ClientApplication::from_name(client_application),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatSessionStarted {
    pub mode: Mode,
    pub client_application: ClientApplication,
}

impl ChatSessionStarted {
    pub const fn new(mode: Mode, client_application: ClientApplication) -> Self {
        Self {
            mode,
            client_application,
        }
    }

    pub fn from_context(app_type: Option<&str>, mode: Option<&str>, client_application: Option<&str>) -> Self {
        Self::new(
            Mode::from_chat_session(app_type, mode),
            ClientApplication::from_name(client_application),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CliSessionCompleted {
    pub exit_reason: ExitReason,
    pub agent_kind: AgentKind,
}

impl CliSessionCompleted {
    pub const fn new(exit_reason: ExitReason, agent_kind: AgentKind) -> Self {
        Self {
            exit_reason,
            agent_kind,
        }
    }

    pub fn from_names(exit_reason: &str, agent_kind: &str) -> Self {
        Self::new(ExitReason::from_name(exit_reason), AgentKind::from_name(agent_kind))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DailyHeartbeat {
    pub client_application: ClientApplication,
    pub install_method: InstallSource,
}

impl DailyHeartbeat {
    pub const fn new(client_application: ClientApplication, install_method: InstallSource) -> Self {
        Self {
            client_application,
            install_method,
        }
    }

    pub fn from_names(client_application: Option<&str>, install_method: Option<&str>) -> Self {
        Self::new(
            ClientApplication::from_name(client_application),
            InstallSource::from_name(install_method.unwrap_or("unknown")),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelInvocation {
    pub model_class: ModelClass,
}

impl ModelInvocation {
    pub const fn new(model_class: ModelClass) -> Self {
        Self { model_class }
    }

    pub fn from_id(model_id: Option<&str>) -> Self {
        Self::new(ModelClass::from_model_id(model_id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyResponseRetry {
    pub model_class: ModelClass,
    pub outcome: Outcome,
}

impl EmptyResponseRetry {
    pub const fn new(model_class: ModelClass, outcome: Outcome) -> Self {
        Self { model_class, outcome }
    }

    pub fn from_id(model_id: Option<&str>, outcome: Outcome) -> Self {
        Self::new(ModelClass::from_model_id(model_id), outcome)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BedrockRequestError {
    pub model_class: ModelClass,
    pub operation: Operation,
    pub error_kind: ErrorKind,
    pub status_class: StatusClass,
}

impl BedrockRequestError {
    pub const fn new(
        model_class: ModelClass,
        operation: Operation,
        error_kind: ErrorKind,
        status_class: StatusClass,
    ) -> Self {
        Self {
            model_class,
            operation,
            error_kind,
            status_class,
        }
    }

    pub fn from_stream_reason(model_id: Option<&str>, reason: Option<&str>, status_code: Option<u16>) -> Self {
        Self::new(
            ModelClass::from_model_id(model_id),
            Operation::Stream,
            ErrorKind::from_reason(reason, status_code),
            StatusClass::from_status_code(status_code),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryAttempt {
    pub upstream: Upstream,
    pub retry_reason: RetryReason,
    pub attempt_number_bucket: AttemptNumberBucket,
}

impl RetryAttempt {
    pub const fn new(
        upstream: Upstream,
        retry_reason: RetryReason,
        attempt_number_bucket: AttemptNumberBucket,
    ) -> Self {
        Self {
            upstream,
            retry_reason,
            attempt_number_bucket,
        }
    }

    pub const fn from_attempt(upstream: Upstream, retry_reason: RetryReason, attempt: u32) -> Self {
        Self::new(upstream, retry_reason, AttemptNumberBucket::from_attempt(attempt))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryExhausted {
    pub upstream: Upstream,
    pub final_error_kind: ErrorKind,
}

impl RetryExhausted {
    pub const fn new(upstream: Upstream, final_error_kind: ErrorKind) -> Self {
        Self {
            upstream,
            final_error_kind,
        }
    }

    pub fn from_reason(upstream: Upstream, reason: Option<&str>, status_code: Option<u16>) -> Self {
        Self::new(upstream, ErrorKind::from_reason(reason, status_code))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureUsed<'a> {
    pub feature: &'a str,
}

impl<'a> FeatureUsed<'a> {
    pub const fn new(feature: &'a str) -> Self {
        Self { feature }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashCommandInvoked<'a> {
    pub command: &'a str,
}

impl<'a> SlashCommandInvoked<'a> {
    pub const fn new(command: &'a str) -> Self {
        Self { command }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionOutcomeMetric {
    pub outcome: SessionOutcome,
}

impl SessionOutcomeMetric {
    pub const fn new(outcome: SessionOutcome) -> Self {
        Self { outcome }
    }

    pub fn from_goal_terminal_state(terminal_state: &str) -> Self {
        Self::new(SessionOutcome::from_goal_terminal_state(terminal_state))
    }
}

pub fn cli_session_started(
    os_type: OsType,
    install_source: InstallSource,
    client_application: ClientApplication,
) -> MetricRecord {
    counter("cli_session_started_total", 1)
        .attribute("version_minor_bucket", "current")
        .attribute("os_type", os_type.as_str())
        .attribute("install_source", install_source.as_str())
        .attribute("client_application", client_application.as_str())
        .expect_valid()
}

pub fn cli_session_started_record(input: CliSessionStarted) -> MetricRecord {
    cli_session_started(input.os_type, input.install_source, input.client_application)
}

pub fn cli_session_started_from_names(
    os_type: &str,
    install_source: &str,
    client_application: Option<&str>,
) -> MetricRecord {
    cli_session_started_record(CliSessionStarted::from_names(
        os_type,
        install_source,
        client_application,
    ))
}

pub fn chat_session_started(mode: Mode, client_application: ClientApplication) -> MetricRecord {
    counter("chat_session_started_total", 1)
        .attribute("version_minor_bucket", "current")
        .attribute("mode", mode.as_str())
        .attribute("client_application", client_application.as_str())
        .expect_valid()
}

pub fn chat_session_started_record(input: ChatSessionStarted) -> MetricRecord {
    chat_session_started(input.mode, input.client_application)
}

pub fn chat_session_started_from_context(
    app_type: Option<&str>,
    mode: Option<&str>,
    client_application: Option<&str>,
) -> MetricRecord {
    chat_session_started_record(ChatSessionStarted::from_context(app_type, mode, client_application))
}

pub fn cli_session_completed(exit_reason: ExitReason, agent_kind: AgentKind) -> MetricRecord {
    counter("chat_cli.session.completed", 1)
        .attribute("exit_reason", exit_reason.as_str())
        .attribute("agent_kind", agent_kind.as_str())
        .expect_valid()
}

pub fn cli_session_completed_record(input: CliSessionCompleted) -> MetricRecord {
    cli_session_completed(input.exit_reason, input.agent_kind)
}

pub fn cli_session_completed_from_names(exit_reason: &str, agent_kind: &str) -> MetricRecord {
    cli_session_completed_record(CliSessionCompleted::from_names(exit_reason, agent_kind))
}

pub fn daily_heartbeat(client_application: ClientApplication, install_method: InstallSource) -> MetricRecord {
    counter("kiro_cli_daily_heartbeat", 1)
        .attribute("client_application", client_application.as_str())
        .attribute("install_method", install_method.as_str())
        .expect_valid()
}

pub fn daily_heartbeat_record(input: DailyHeartbeat) -> MetricRecord {
    daily_heartbeat(input.client_application, input.install_method)
}

pub fn daily_heartbeat_from_names(client_application: Option<&str>, install_method: Option<&str>) -> MetricRecord {
    daily_heartbeat_record(DailyHeartbeat::from_names(client_application, install_method))
}

// §5.1 adoption gauges. These are aggregate rollups computed by the nightly
// batch job (never per-process), so they take a precomputed value.

pub fn active_users_daily(
    users: f64,
    install_method: InstallSource,
    client_application: ClientApplication,
    is_internal_amazon: bool,
) -> MetricRecord {
    gauge("active_users_daily", users)
        .attribute("install_method", install_method.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("is_internal_amazon", is_internal_amazon.to_string())
        .expect_valid()
}

pub fn active_users_weekly(
    users: f64,
    install_method: InstallSource,
    client_application: ClientApplication,
    is_internal_amazon: bool,
) -> MetricRecord {
    gauge("active_users_weekly", users)
        .attribute("install_method", install_method.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("is_internal_amazon", is_internal_amazon.to_string())
        .expect_valid()
}

pub fn active_users_monthly(users: f64) -> MetricRecord {
    gauge("active_users_monthly", users).expect_valid()
}

pub fn dau_mau_ratio(ratio: f64) -> MetricRecord {
    gauge("dau_mau_ratio", ratio).expect_valid()
}

pub fn new_users_daily(users: f64, install_source: InstallSource) -> MetricRecord {
    gauge("new_users_daily", users)
        .attribute("install_source", install_source.as_str())
        .expect_valid()
}

pub fn client_version_seen(
    users: f64,
    version_full: &str,
    release_channel: ReleaseChannel,
    os_type: OsType,
) -> MetricRecord {
    gauge("client_version_seen", users)
        .attribute("version_full", normalized_dynamic_name(version_full))
        .attribute("release_channel", release_channel.as_str())
        .attribute("os_type", os_type.as_str())
        .expect_valid()
}

pub fn version_adoption_pct(
    percent: f64,
    version_minor_bucket: VersionMinorBucket,
    release_channel: ReleaseChannel,
) -> MetricRecord {
    gauge("version_adoption_pct", percent)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("release_channel", release_channel.as_str())
        .expect_valid()
}

pub fn stale_version_users(users: f64, staleness_bucket: StalenessBucket) -> MetricRecord {
    gauge("stale_version_users", users)
        .attribute("staleness_bucket", staleness_bucket.as_str())
        .expect_valid()
}

pub fn feature_unique_users_weekly(users: f64, feature: &str) -> MetricRecord {
    gauge("feature_unique_users_weekly", users)
        .attribute("feature", normalized_dynamic_name(feature))
        .expect_valid()
}

pub fn tool_using_sessions_pct(percent: f64) -> MetricRecord {
    gauge("tool_using_sessions_pct", percent).expect_valid()
}

pub fn mode_active_users_weekly(users: f64, mode: Mode) -> MetricRecord {
    gauge("mode_active_users_weekly", users)
        .attribute("mode", mode.as_str())
        .expect_valid()
}

pub fn upgrade_completed(
    from_version_minor_bucket: VersionMinorBucket,
    to_version_minor_bucket: VersionMinorBucket,
    upgrade_trigger: UpgradeTrigger,
) -> MetricRecord {
    counter("upgrade_completed_total", 1)
        .attribute("from_version_minor_bucket", from_version_minor_bucket.as_str())
        .attribute("to_version_minor_bucket", to_version_minor_bucket.as_str())
        .attribute("upgrade_trigger", upgrade_trigger.as_str())
        .expect_valid()
}

pub fn model_invocation(model_class: ModelClass) -> MetricRecord {
    counter("model_invocations_total", 1)
        .attribute("model_class", model_class.as_str())
        .expect_valid()
}

pub fn model_invocation_record(input: ModelInvocation) -> MetricRecord {
    model_invocation(input.model_class)
}

pub fn model_invocation_from_id(model_id: Option<&str>) -> MetricRecord {
    model_invocation_record(ModelInvocation::from_id(model_id))
}

pub fn empty_response_retry(model_class: ModelClass, outcome: Outcome) -> MetricRecord {
    counter("chat_cli.bedrock.empty_response.retries", 1)
        .attribute("model_class", model_class.as_str())
        .attribute("outcome", outcome.as_str())
        .expect_valid()
}

pub fn empty_response_retry_record(input: EmptyResponseRetry) -> MetricRecord {
    empty_response_retry(input.model_class, input.outcome)
}

pub fn empty_response_retry_from_id(model_id: Option<&str>, outcome: Outcome) -> MetricRecord {
    empty_response_retry_record(EmptyResponseRetry::from_id(model_id, outcome))
}

pub fn bedrock_request_error(
    model_class: ModelClass,
    operation: Operation,
    error_kind: ErrorKind,
    status_class: StatusClass,
) -> MetricRecord {
    counter("chat_cli.bedrock.request.errors", 1)
        .attribute("model_class", model_class.as_str())
        .attribute("operation", operation.as_str())
        .attribute("error_kind", error_kind.as_str())
        .attribute("status_class", status_class.as_str())
        .expect_valid()
}

pub fn bedrock_request_error_record(input: BedrockRequestError) -> MetricRecord {
    bedrock_request_error(input.model_class, input.operation, input.error_kind, input.status_class)
}

pub fn bedrock_stream_request_error_from_reason(
    model_id: Option<&str>,
    reason: Option<&str>,
    status_code: Option<u16>,
) -> MetricRecord {
    bedrock_request_error_record(BedrockRequestError::from_stream_reason(model_id, reason, status_code))
}

pub fn retry_attempt(
    upstream: Upstream,
    retry_reason: RetryReason,
    attempt_number_bucket: AttemptNumberBucket,
) -> MetricRecord {
    counter("chat_cli.retry.attempts", 1)
        .attribute("upstream", upstream.as_str())
        .attribute("retry_reason", retry_reason.as_str())
        .attribute("attempt_number_bucket", attempt_number_bucket.as_str())
        .expect_valid()
}

pub fn retry_attempt_record(input: RetryAttempt) -> MetricRecord {
    retry_attempt(input.upstream, input.retry_reason, input.attempt_number_bucket)
}

pub fn retry_attempt_from_attempt(upstream: Upstream, retry_reason: RetryReason, attempt: u32) -> MetricRecord {
    retry_attempt_record(RetryAttempt::from_attempt(upstream, retry_reason, attempt))
}

pub fn retry_exhausted(upstream: Upstream, final_error_kind: ErrorKind) -> MetricRecord {
    counter("chat_cli.retry.exhausted", 1)
        .attribute("upstream", upstream.as_str())
        .attribute("final_error_kind", final_error_kind.as_str())
        .expect_valid()
}

pub fn retry_exhausted_record(input: RetryExhausted) -> MetricRecord {
    retry_exhausted(input.upstream, input.final_error_kind)
}

pub fn retry_exhausted_from_reason(upstream: Upstream, reason: Option<&str>, status_code: Option<u16>) -> MetricRecord {
    retry_exhausted_record(RetryExhausted::from_reason(upstream, reason, status_code))
}

pub fn feature_used(feature: &str) -> MetricRecord {
    counter("feature_used_total", 1)
        .attribute("feature", normalized_dynamic_name(feature))
        .attribute("version_minor_bucket", VersionMinorBucket::Current.as_str())
        .expect_valid()
}

pub fn feature_used_record(input: FeatureUsed<'_>) -> MetricRecord {
    feature_used(input.feature)
}

pub fn slash_command_invoked(command: &str) -> MetricRecord {
    counter("slash_command_invoked_total", 1)
        .attribute("command", normalized_dynamic_name(command))
        .attribute("version_minor_bucket", VersionMinorBucket::Current.as_str())
        .expect_valid()
}

pub fn slash_command_invoked_record(input: SlashCommandInvoked<'_>) -> MetricRecord {
    slash_command_invoked(input.command)
}

fn tool_call_metric(
    metric_name: &'static str,
    tool_origin: ToolOrigin,
    builtin_tool_name: Option<&str>,
    outcome: Outcome,
) -> MetricRecord {
    let mut builder = counter(metric_name, 1)
        .attribute("tool_origin", tool_origin.as_str())
        .attribute("outcome", outcome.as_str());
    if tool_origin == ToolOrigin::Builtin {
        builder = builder.attribute("builtin_tool_name", builtin_tool_name_value(builtin_tool_name));
    }
    builder.expect_valid()
}

pub fn tool_call_total(tool_origin: ToolOrigin, builtin_tool_name: Option<&str>, outcome: Outcome) -> MetricRecord {
    tool_call_metric("tool_call_total", tool_origin, builtin_tool_name, outcome)
}

pub fn tool_call_total_for_invocation(invocation: ToolInvocation<'_>) -> MetricRecord {
    tool_call_total(invocation.origin(), invocation.tool_name, invocation.outcome())
}

pub fn tool_invocations(tool_origin: ToolOrigin, outcome: Outcome) -> MetricRecord {
    counter("kiro_cli_tool_invocations", 1)
        .attribute("tool_origin", tool_origin.as_str())
        .attribute("outcome", outcome.as_str())
        .expect_valid()
}

pub fn tool_invocations_for_invocation(invocation: ToolInvocation<'_>) -> MetricRecord {
    tool_invocations(invocation.origin(), invocation.outcome())
}

pub fn tool_execution_duration_ms(duration_ms: f64, tool_origin: ToolOrigin, is_success: bool) -> MetricRecord {
    histogram("kiro_cli_tool_execution_duration_ms", duration_ms)
        .attribute("tool_origin", tool_origin.as_str())
        .attribute("is_success", is_success.to_string())
        .expect_valid()
}

pub fn tool_execution_duration_ms_for_invocation(
    duration_ms: f64,
    invocation: ToolInvocation<'_>,
) -> Option<MetricRecord> {
    let is_success = invocation.is_success?;
    if !duration_ms.is_finite() || duration_ms <= 0.0 {
        return None;
    }

    Some(tool_execution_duration_ms(duration_ms, invocation.origin(), is_success))
}

pub fn tool_use_records(input: ToolUseMetrics<'_>) -> Vec<MetricRecord> {
    let mut records = Vec::new();

    if input.emit_tool_call_total {
        records.push(tool_call_total_for_invocation(input.invocation));
    }
    records.push(tool_invocations_for_invocation(input.invocation));
    if let Some(record) = input
        .execution_duration_ms
        .and_then(|duration_ms| tool_execution_duration_ms_for_invocation(duration_ms, input.invocation))
    {
        records.push(record);
    }

    records
}

pub fn user_turns(
    model_class: ModelClass,
    client_application: ClientApplication,
    result: ResultKind,
    is_subagent: bool,
    mode: Mode,
) -> MetricRecord {
    counter("kiro_cli_user_turns", 1)
        .attribute("model_class", model_class.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("result", result.as_str())
        .attribute("is_subagent", is_subagent.to_string())
        .attribute("mode", mode.as_str())
        .expect_valid()
}

pub fn user_turns_for_invocation(context: InvocationContext, result: ResultKind, mode: Mode) -> MetricRecord {
    user_turns(
        context.model_class,
        context.client_application,
        result,
        context.is_subagent,
        mode,
    )
}

pub fn time_to_first_chunk_ms(
    milliseconds: f64,
    model_class: ModelClass,
    client_application: ClientApplication,
    is_subagent: bool,
) -> MetricRecord {
    histogram("kiro_cli_time_to_first_chunk_ms", milliseconds)
        .attribute("model_class", model_class.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("is_subagent", is_subagent.to_string())
        .expect_valid()
}

pub fn time_to_first_chunk_ms_for_invocation(milliseconds: f64, context: InvocationContext) -> MetricRecord {
    time_to_first_chunk_ms(
        milliseconds,
        context.model_class,
        context.client_application,
        context.is_subagent,
    )
}

pub fn bedrock_stream_ttft(
    seconds: f64,
    model_class: ModelClass,
    prompt_size: PromptSizeBucket,
    tools_enabled: bool,
) -> MetricRecord {
    histogram("chat_cli.bedrock.stream.ttft", seconds)
        .attribute("model_class", model_class.as_str())
        .attribute("prompt_size_bucket", prompt_size.as_str())
        .attribute("tools_enabled", tools_enabled.to_string())
        .expect_valid()
}

pub fn bedrock_stream_inter_token_latency(seconds: f64, model_class: ModelClass) -> MetricRecord {
    histogram("chat_cli.bedrock.stream.inter_token_latency", seconds)
        .attribute("model_class", model_class.as_str())
        .expect_valid()
}

pub fn bedrock_stream_duration(
    seconds: f64,
    model_class: ModelClass,
    completion_reason: crate::log::CompletionReason,
) -> MetricRecord {
    histogram("chat_cli.bedrock.stream.duration", seconds)
        .attribute("model_class", model_class.as_str())
        .attribute("completion_reason", completion_reason.as_str())
        .expect_valid()
}

pub fn bedrock_request_duration(
    seconds: f64,
    model_class: ModelClass,
    operation: Operation,
    outcome: Outcome,
) -> MetricRecord {
    histogram("chat_cli.bedrock.request.duration", seconds)
        .attribute("model_class", model_class.as_str())
        .attribute("operation", operation.as_str())
        .attribute("outcome", outcome.as_str())
        .expect_valid()
}

pub fn bedrock_stream_timing_records(input: BedrockStreamMetrics<'_>) -> Vec<MetricRecord> {
    let model_class = ModelClass::from_model_id(input.model_id);
    bedrock_stream_timing_records_for_model_class(input, model_class)
}

fn bedrock_stream_timing_records_for_model_class(
    input: BedrockStreamMetrics<'_>,
    model_class: ModelClass,
) -> Vec<MetricRecord> {
    let mut records = Vec::new();

    if let Some(milliseconds) = input
        .time_to_first_chunk_ms
        .filter(|milliseconds| milliseconds.is_finite() && *milliseconds > 0.0)
    {
        records.push(bedrock_stream_ttft(
            milliseconds / 1000.0,
            model_class,
            PromptSizeBucket::from_context_file_length(input.context_file_length),
            input.tools_enabled,
        ));
    }

    records.extend(
        input
            .time_between_chunks_ms
            .unwrap_or_default()
            .iter()
            .filter(|milliseconds| milliseconds.is_finite() && **milliseconds > 0.0)
            .map(|milliseconds| bedrock_stream_inter_token_latency(milliseconds / 1000.0, model_class)),
    );

    if let Some(seconds) = input
        .request_duration_seconds
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
    {
        records.push(bedrock_stream_duration(seconds, model_class, input.completion_reason));
        records.push(bedrock_request_duration(
            seconds,
            model_class,
            Operation::Stream,
            input.request_outcome,
        ));
    }

    records
}

pub fn model_response_records(input: ModelResponseMetrics<'_>) -> Vec<MetricRecord> {
    let mut records = Vec::new();

    if input.emit_user_turn_counter {
        records.push(user_turns_for_invocation(input.context, input.result, input.mode));
    }

    records.push(model_invocation(input.context.model_class));

    if let Some(milliseconds) = input
        .time_to_first_chunk_ms
        .filter(|milliseconds| milliseconds.is_finite() && *milliseconds >= 0.0)
    {
        records.push(time_to_first_chunk_ms_for_invocation(milliseconds, input.context));
    }

    records.extend(bedrock_stream_timing_records_for_model_class(
        BedrockStreamMetrics {
            model_id: None,
            context_file_length: input.context_file_length,
            tools_enabled: input.tools_enabled,
            time_to_first_chunk_ms: input.time_to_first_chunk_ms,
            time_between_chunks_ms: input.time_between_chunks_ms,
            request_duration_seconds: input.request_duration_seconds,
            completion_reason: input.completion_reason,
            request_outcome: input.request_outcome,
        },
        input.context.model_class,
    ));

    records.extend(estimated_token_economics_records(input.context, input.token_usage));

    if let Some(record) = cache_hit_ratio_from_usage(input.context, input.conversation_type, input.token_usage) {
        records.push(record);
    }

    records
}

pub fn user_turn_completion_records(input: UserTurnCompletionMetrics) -> Vec<MetricRecord> {
    let mut records = Vec::new();

    if input.emit_user_turn_counter {
        records.push(user_turns_for_invocation(input.context, input.result, input.mode));
        records.extend(token_economics_records(
            input.context,
            input.token_usage,
            input
                .estimated_cost_usd
                .or_else(|| input.context.estimated_cost_usd(input.token_usage)),
        ));
    }

    if let Some(seconds) = input
        .duration_seconds
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
    {
        records.push(user_turn_duration_seconds_for_invocation(
            seconds,
            input.context,
            input.conversation_type,
            input.mode,
        ));
    }

    records
}

pub fn tokens_consumed(
    value: u64,
    model_class: ModelClass,
    token_type: TokenType,
    client_application: ClientApplication,
    is_subagent: bool,
) -> MetricRecord {
    counter("kiro_cli_tokens_consumed", value)
        .attribute("model_class", model_class.as_str())
        .attribute("token_type", token_type.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("is_subagent", is_subagent.to_string())
        .expect_valid()
}

pub fn estimated_cost_usd(
    value: f64,
    model_class: ModelClass,
    client_application: ClientApplication,
    is_subagent: bool,
) -> MetricRecord {
    counter_f64("kiro_cli_estimated_cost_usd", value)
        .attribute("model_class", model_class.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("is_subagent", is_subagent.to_string())
        .expect_valid()
}

pub fn pricing_table_active(version: f64) -> MetricRecord {
    gauge("kiro_cli_pricing_table_active", version).expect_valid()
}

pub fn token_records(context: InvocationContext, usage: TokenUsage) -> Vec<MetricRecord> {
    let mut records = Vec::new();
    push_token_record(
        &mut records,
        context,
        TokenType::InputUncached,
        usage.uncached_input_tokens,
    );
    push_token_record(
        &mut records,
        context,
        TokenType::InputCacheRead,
        usage.cache_read_input_tokens,
    );
    push_token_record(
        &mut records,
        context,
        TokenType::InputCacheWrite,
        usage.cache_write_input_tokens,
    );
    push_token_record(&mut records, context, TokenType::Output, usage.output_tokens);
    records
}

pub fn token_economics_records(
    context: InvocationContext,
    usage: TokenUsage,
    estimated_cost_usd: Option<f64>,
) -> Vec<MetricRecord> {
    let mut records = token_records(context, usage);
    if let Some(cost) = estimated_cost_usd.filter(|cost| cost.is_finite() && *cost > 0.0) {
        records.push(estimated_cost_usd_metric(context, cost));
        records.push(pricing_table_active(PRICING_TABLE_VERSION));
    }
    records
}

pub fn estimated_token_economics_records(context: InvocationContext, usage: TokenUsage) -> Vec<MetricRecord> {
    token_economics_records(context, usage, context.estimated_cost_usd(usage))
}

fn push_token_record(records: &mut Vec<MetricRecord>, context: InvocationContext, token_type: TokenType, value: u64) {
    if value == 0 {
        return;
    }

    records.push(tokens_consumed(
        value,
        context.model_class,
        token_type,
        context.client_application,
        context.is_subagent,
    ));
}

fn estimated_cost_usd_metric(context: InvocationContext, value: f64) -> MetricRecord {
    estimated_cost_usd(
        value,
        context.model_class,
        context.client_application,
        context.is_subagent,
    )
}

pub fn cache_hit_ratio(
    value: f64,
    model_class: ModelClass,
    conversation_type: ChatConversationKind,
    client_application: ClientApplication,
) -> MetricRecord {
    histogram("kiro_cli_cache_hit_ratio", value)
        .attribute("model_class", model_class.as_str())
        .attribute("chat_conversation_type", conversation_type.as_str())
        .attribute("client_application", client_application.as_str())
        .expect_valid()
}

pub fn cache_hit_ratio_from_usage(
    context: InvocationContext,
    conversation_type: ChatConversationKind,
    usage: TokenUsage,
) -> Option<MetricRecord> {
    let uncached_input_tokens = usage.uncached_input_tokens as f64;
    let cache_read_input_tokens = usage.cache_read_input_tokens as f64;
    let total_input_tokens = uncached_input_tokens + cache_read_input_tokens;
    (total_input_tokens > 0.0).then(|| {
        cache_hit_ratio(
            cache_read_input_tokens / total_input_tokens,
            context.model_class,
            conversation_type,
            context.client_application,
        )
    })
}

pub fn user_turn_duration_seconds(
    seconds: f64,
    model_class: ModelClass,
    conversation_type: ChatConversationKind,
    is_subagent: bool,
    mode: Mode,
) -> MetricRecord {
    histogram("kiro_cli_user_turn_duration_seconds", seconds)
        .attribute("model_class", model_class.as_str())
        .attribute("chat_conversation_type", conversation_type.as_str())
        .attribute("is_subagent", is_subagent.to_string())
        .attribute("mode", mode.as_str())
        .expect_valid()
}

pub fn user_turn_duration_seconds_for_invocation(
    seconds: f64,
    context: InvocationContext,
    conversation_type: ChatConversationKind,
    mode: Mode,
) -> MetricRecord {
    user_turn_duration_seconds(
        seconds,
        context.model_class,
        conversation_type,
        context.is_subagent,
        mode,
    )
}

pub fn context_usage_percentage(
    percentage: f64,
    model_class: ModelClass,
    client_application: ClientApplication,
    is_subagent: bool,
) -> MetricRecord {
    // Context usage is a point-in-time "how full is the context window right now"
    // reading (0-100), not a distribution worth quantiling. A gauge renders the
    // 0-100 range directly and avoids the bucket-boundary problem entirely (the
    // OTel SDK's default histogram buckets are tuned for ms-scale latencies and
    // would otherwise stretch the axis to 10000).
    gauge("kiro_cli_context_usage_percentage", percentage)
        .attribute("model_class", model_class.as_str())
        .attribute("client_application", client_application.as_str())
        .attribute("is_subagent", is_subagent.to_string())
        .expect_valid()
}

pub fn context_usage_percentage_for_invocation(percentage: f64, context: InvocationContext) -> MetricRecord {
    context_usage_percentage(
        percentage,
        context.model_class,
        context.client_application,
        context.is_subagent,
    )
}

pub fn context_usage_percentage_record(input: ContextUsageMetric) -> Option<MetricRecord> {
    if !input.percentage.is_finite() || input.percentage < 0.0 {
        return None;
    }

    Some(context_usage_percentage_for_invocation(input.percentage, input.context))
}

pub fn mcp_server_init_total(server_class: McpServerClass, outcome: Outcome) -> MetricRecord {
    counter("kiro_cli_mcp_server_init_total", 1)
        .attribute("mcp_server_class", server_class.as_str())
        .attribute("outcome", outcome.as_str())
        .expect_valid()
}

pub fn mcp_server_init_total_record(input: McpServerInit<'_>) -> MetricRecord {
    mcp_server_init_total(input.server_class, input.outcome)
}

pub fn mcp_server_connected_total(server_class: McpServerClass) -> MetricRecord {
    counter("mcp_server_connected_total", 1)
        .attribute("mcp_server_class", server_class.as_str())
        .expect_valid()
}

pub fn mcp_server_connected_total_record(input: McpServerInit<'_>) -> Option<MetricRecord> {
    (input.outcome == Outcome::Success).then(|| mcp_server_connected_total(input.server_class))
}

pub fn mcp_server_init_records(input: McpServerInit<'_>) -> Vec<MetricRecord> {
    let mut records = vec![mcp_server_init_total_record(input)];
    records.extend(mcp_server_connected_total_record(input));
    records
}

pub fn mcp_server_init_total_from_name(server_name: &str, init_failure_reason: Option<&str>) -> MetricRecord {
    mcp_server_init_total_record(McpServerInit::from_name(server_name, init_failure_reason))
}

pub fn mcp_server_connected_total_from_name(
    server_name: &str,
    init_failure_reason: Option<&str>,
) -> Option<MetricRecord> {
    mcp_server_connected_total_record(McpServerInit::from_name(server_name, init_failure_reason))
}

pub fn session_outcome(outcome: SessionOutcome) -> MetricRecord {
    counter("session_outcome_total", 1)
        .attribute("session_outcome", outcome.as_str())
        .expect_valid()
}

pub fn session_outcome_record(input: SessionOutcomeMetric) -> MetricRecord {
    session_outcome(input.outcome)
}

pub fn session_outcome_from_goal_terminal_state(terminal_state: &str) -> MetricRecord {
    session_outcome_record(SessionOutcomeMetric::from_goal_terminal_state(terminal_state))
}

pub fn user_feedback(sentiment: Sentiment, surface: FeedbackSurface) -> MetricRecord {
    counter("user_feedback_total", 1)
        .attribute("sentiment", sentiment.as_str())
        .attribute("surface", surface.as_str())
        .expect_valid()
}

pub fn user_feedback_from_names(sentiment: &str, surface: &str) -> MetricRecord {
    user_feedback(Sentiment::from_name(sentiment), FeedbackSurface::from_name(surface))
}

pub fn message_regenerated(model_class: ModelClass) -> MetricRecord {
    counter("message_regenerated_total", 1)
        .attribute("model_class", model_class.as_str())
        .expect_valid()
}

pub fn message_regenerated_from_id(model_id: Option<&str>) -> MetricRecord {
    message_regenerated(ModelClass::from_model_id(model_id))
}

pub fn process_memory_rss(bytes: f64, version_minor_bucket: VersionMinorBucket, agent_kind: AgentKind) -> MetricRecord {
    gauge("chat_cli.process.memory.rss", bytes)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("agent_kind", agent_kind.as_str())
        .expect_valid()
}

pub fn process_memory_rss_from_names(bytes: f64, version: &str, agent_kind: Option<&str>) -> MetricRecord {
    process_memory_rss(
        bytes,
        VersionMinorBucket::from_version(version),
        AgentKind::from_name(agent_kind.unwrap_or_default()),
    )
}

pub fn process_cpu_utilization(
    utilization: f64,
    version_minor_bucket: VersionMinorBucket,
    agent_kind: AgentKind,
    state: ProcessState,
) -> MetricRecord {
    histogram("chat_cli.process.cpu.utilization", utilization)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("agent_kind", agent_kind.as_str())
        .attribute("state", state.as_str())
        .expect_valid()
}

pub fn process_cpu_utilization_from_names(
    utilization: f64,
    version: &str,
    agent_kind: Option<&str>,
    state: ProcessState,
) -> MetricRecord {
    process_cpu_utilization(
        utilization,
        VersionMinorBucket::from_version(version),
        AgentKind::from_name(agent_kind.unwrap_or_default()),
        state,
    )
}

pub fn process_health_records(snapshot: ProcessHealthSnapshot<'_>) -> Vec<MetricRecord> {
    let mut records = Vec::new();
    if snapshot.rss_mb.is_finite() && snapshot.rss_mb >= 0.0 {
        records.push(process_memory_rss(
            snapshot.rss_mb * 1024.0 * 1024.0,
            VersionMinorBucket::from_version(snapshot.version),
            snapshot.agent_kind,
        ));
    }

    let cpu_utilization_pct = snapshot.cpu_user_pct + snapshot.cpu_system_pct;
    if snapshot.cpu_user_pct.is_finite() && snapshot.cpu_system_pct.is_finite() && cpu_utilization_pct >= 0.0 {
        records.push(process_cpu_utilization(
            cpu_utilization_pct / 100.0,
            VersionMinorBucket::from_version(snapshot.version),
            snapshot.agent_kind,
            snapshot.state,
        ));
    }

    records
}

pub fn process_memory_growth_rate(
    bytes_per_second: f64,
    version_minor_bucket: VersionMinorBucket,
    agent_kind: AgentKind,
) -> MetricRecord {
    histogram("chat_cli.process.memory.growth_rate", bytes_per_second)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("agent_kind", agent_kind.as_str())
        .expect_valid()
}

pub fn process_fds_open(count: f64, version_minor_bucket: VersionMinorBucket, agent_kind: AgentKind) -> MetricRecord {
    gauge("chat_cli.process.fds.open", count)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("agent_kind", agent_kind.as_str())
        .expect_valid()
}

pub fn process_threads(count: f64, version_minor_bucket: VersionMinorBucket, agent_kind: AgentKind) -> MetricRecord {
    gauge("chat_cli.process.threads", count)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("agent_kind", agent_kind.as_str())
        .expect_valid()
}

pub fn govcloud_channel_disabled(
    channel: TelemetryChannel,
    partition: Partition,
    reason: PostureReason,
) -> MetricRecord {
    counter("govcloud_channel_disabled_total", 1)
        .attribute("channel", channel.as_str())
        .attribute("partition", partition.as_str())
        .attribute("reason", reason.as_str())
        .expect_valid()
}

pub fn govcloud_channel_disabled_record(input: GovcloudChannelDisabled) -> MetricRecord {
    govcloud_channel_disabled(input.channel, input.partition, input.reason)
}

pub fn govcloud_channel_disabled_from_names(channel: &str, partition: &str) -> MetricRecord {
    govcloud_channel_disabled_record(GovcloudChannelDisabled::from_names(channel, partition))
}

pub fn govcloud_channel_leak(channel: TelemetryChannel) -> MetricRecord {
    counter("govcloud_channel_leak_total", 1)
        .attribute("channel", channel.as_str())
        .expect_valid()
}

pub fn govcloud_channel_leak_record(input: GovcloudChannelLeak) -> MetricRecord {
    govcloud_channel_leak(input.channel)
}

pub fn govcloud_channel_leak_from_name(channel: &str) -> MetricRecord {
    govcloud_channel_leak_record(GovcloudChannelLeak::from_name(channel))
}

pub fn telemetry_opt_out_respected(channel: TelemetryChannel, event_class: EventClass) -> MetricRecord {
    counter("telemetry_opt_out_respected_total", 1)
        .attribute("channel", channel.as_str())
        .attribute("event_class", event_class.as_str())
        .expect_valid()
}

pub fn telemetry_opt_out_violation(channel: TelemetryChannel) -> MetricRecord {
    counter("telemetry_opt_out_violation_total", 1)
        .attribute("channel", channel.as_str())
        .expect_valid()
}

pub fn pii_redaction_run(
    redactor: Redactor,
    event_class: EventClass,
    channel: TelemetryChannel,
    result: RedactionResult,
) -> MetricRecord {
    counter("pii_redaction_runs_total", 1)
        .attribute("redactor", redactor.as_str())
        .attribute("event_class", event_class.as_str())
        .attribute("channel", channel.as_str())
        .attribute("redaction_result", result.as_str())
        .expect_valid()
}

pub fn pii_redaction_match(count: u64, pii_type: PiiType, field_class: FieldClass) -> MetricRecord {
    counter("pii_redaction_matches_total", count)
        .attribute("pii_type", pii_type.as_str())
        .attribute("field_class", field_class.as_str())
        .expect_valid()
}

pub fn pii_redaction_error(
    redactor: Redactor,
    error_kind: ErrorKind,
    fail_action: RedactionFailAction,
) -> MetricRecord {
    counter("pii_redaction_errors_total", 1)
        .attribute("redactor", redactor.as_str())
        .attribute("error_kind", error_kind.as_str())
        .attribute("fail_action", fail_action.as_str())
        .expect_valid()
}

pub fn consent_record_integrity(check_kind: ConsentCheckKind, result: ConsentIntegrityResult) -> MetricRecord {
    counter("consent_record_integrity_total", 1)
        .attribute("check_kind", check_kind.as_str())
        .attribute("integrity_result", result.as_str())
        .expect_valid()
}

pub fn auth_credential_failure(
    auth_provider: AuthProvider,
    error_code: &str,
    operation: Operation,
    partition: Partition,
) -> MetricRecord {
    counter("auth_credential_failure_total", 1)
        .attribute("auth_provider", auth_provider.as_str())
        .attribute("error_code", normalized_dynamic_name(error_code))
        .attribute("operation", operation.as_str())
        .attribute("partition", partition.as_str())
        .expect_valid()
}

pub fn auth_unexpected_identity(
    expected_partition: Partition,
    actual_partition: Partition,
    operation: Operation,
) -> MetricRecord {
    counter("auth_unexpected_identity_total", 1)
        .attribute("expected_partition", expected_partition.as_str())
        .attribute("actual_partition", actual_partition.as_str())
        .attribute("operation", operation.as_str())
        .expect_valid()
}

pub fn tls_validation_failure(destination_class: DestinationClass, failure_reason: TlsFailureReason) -> MetricRecord {
    counter("tls_validation_failure_total", 1)
        .attribute("destination_class", destination_class.as_str())
        .attribute("failure_reason", failure_reason.as_str())
        .expect_valid()
}

pub fn tool_egress_destinations(
    destination_class: DestinationClass,
    scheme: UrlScheme,
    is_allowlisted: bool,
) -> MetricRecord {
    counter("tool_egress_destinations_total", 1)
        .attribute("destination_class", destination_class.as_str())
        .attribute("scheme", scheme.as_str())
        .attribute("is_allowlisted", is_allowlisted.to_string())
        .expect_valid()
}

pub fn kuts_export_oversize(signal: TelemetrySignal) -> MetricRecord {
    counter(KUTS_EXPORT_OVERSIZE_METRIC, 1)
        .attribute("signal", signal.as_str())
        .expect_valid()
}

pub fn telemetry_export_send_attempt(
    exporter: TelemetryExporter,
    signal: TelemetrySignal,
    outcome: ExportOutcome,
) -> MetricRecord {
    counter("telemetry.exporter.send.attempts", 1)
        .attribute("exporter", exporter.as_str())
        .attribute("signal", signal.as_str())
        .attribute("export_outcome", outcome.as_str())
        .expect_valid()
}

pub fn telemetry_export_send_duration(
    seconds: f64,
    exporter: TelemetryExporter,
    signal: TelemetrySignal,
) -> MetricRecord {
    histogram("telemetry.exporter.send.duration", seconds)
        .attribute("exporter", exporter.as_str())
        .attribute("signal", signal.as_str())
        .expect_valid()
}

pub fn telemetry_exporter_dropped(
    exporter: TelemetryExporter,
    signal: TelemetrySignal,
    drop_reason: DropReason,
) -> MetricRecord {
    counter("telemetry.exporter.dropped", 1)
        .attribute("exporter", exporter.as_str())
        .attribute("signal", signal.as_str())
        .attribute("drop_reason", drop_reason.as_str())
        .expect_valid()
}

pub fn telemetry_queue_depth(depth: f64, exporter: TelemetryExporter, signal: TelemetrySignal) -> MetricRecord {
    gauge("telemetry.queue.depth", depth)
        .attribute("exporter", exporter.as_str())
        .attribute("signal", signal.as_str())
        .expect_valid()
}

pub fn telemetry_batch_size(size: f64, exporter: TelemetryExporter, signal: TelemetrySignal) -> MetricRecord {
    histogram("telemetry.batch.size", size)
        .attribute("exporter", exporter.as_str())
        .attribute("signal", signal.as_str())
        .expect_valid()
}

pub fn telemetry_emit_failure(subsystem: TelemetrySubsystem, failure_kind: EmitFailureKind) -> MetricRecord {
    counter("telemetry.emit.failures", 1)
        .attribute("subsystem", subsystem.as_str())
        .attribute("failure_kind", failure_kind.as_str())
        .expect_valid()
}

pub fn telemetry_sdk_up(partition: Partition, os_type: OsType, release_channel: ReleaseChannel) -> MetricRecord {
    gauge("telemetry.sdk.up", 1.0)
        .attribute("partition", partition.as_str())
        .attribute("os_type", os_type.as_str())
        .attribute("release_channel", release_channel.as_str())
        .expect_valid()
}

pub fn telemetry_flush_on_exit_dropped(count: u64, shutdown_path: ShutdownPath) -> MetricRecord {
    counter("telemetry.flush_on_exit.dropped_total", count)
        .attribute("shutdown_path", shutdown_path.as_str())
        .expect_valid()
}

pub fn meta_meter_up(partition: Partition, os_type: OsType) -> MetricRecord {
    gauge("meta_meter.up", 1.0)
        .attribute("partition", partition.as_str())
        .attribute("os_type", os_type.as_str())
        .expect_valid()
}

pub fn crash_total(crash_kind: CrashKind, os_type: OsType, host_arch: HostArch) -> MetricRecord {
    counter("chat_cli.crash.total", 1)
        .attribute("crash_kind", crash_kind.as_str())
        .attribute("os_type", os_type.as_str())
        .attribute("host_arch", host_arch.as_str())
        .expect_valid()
}

pub fn crash_total_from_names(crash_kind: &str, os_type: &str, host_arch: &str) -> MetricRecord {
    crash_total(
        CrashKind::from_name(crash_kind),
        OsType::from_name(os_type),
        HostArch::from_name(host_arch),
    )
}

pub fn startup_failure(failure_stage: FailureStage, os_type: OsType) -> MetricRecord {
    counter("chat_cli.startup.failures", 1)
        .attribute("failure_stage", failure_stage.as_str())
        .attribute("os_type", os_type.as_str())
        .expect_valid()
}

pub fn startup_failure_from_names(failure_stage: &str, os_type: &str) -> MetricRecord {
    startup_failure(FailureStage::from_name(failure_stage), OsType::from_name(os_type))
}

pub fn startup_duration(
    seconds: f64,
    version_minor_bucket: VersionMinorBucket,
    cold_start: bool,
    os_type: OsType,
) -> MetricRecord {
    histogram("chat_cli.startup.duration", seconds)
        .attribute("version_minor_bucket", version_minor_bucket.as_str())
        .attribute("cold_start", cold_start.to_string())
        .attribute("os_type", os_type.as_str())
        .expect_valid()
}

pub fn agent_loop_iteration_duration(seconds: f64, loop_phase: LoopPhase) -> MetricRecord {
    histogram("chat_cli.agent.loop.iteration_duration", seconds)
        .attribute("loop_phase", loop_phase.as_str())
        .expect_valid()
}

pub fn agent_loop_stuck(stuck_phase: StuckPhase, detection: StuckDetection) -> MetricRecord {
    counter("chat_cli.agent.loop.stuck", 1)
        .attribute("stuck_phase", stuck_phase.as_str())
        .attribute("detection", detection.as_str())
        .expect_valid()
}

pub fn upstream_dependency_up(up: bool, dependency: Dependency, partition: Partition) -> MetricRecord {
    gauge("chat_cli.upstream.dependency.up", if up { 1.0 } else { 0.0 })
        .attribute("dependency", dependency.as_str())
        .attribute("partition", partition.as_str())
        .expect_valid()
}

fn normalized_dynamic_name(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "_other_".to_string()
    } else {
        value.to_ascii_lowercase()
    }
}

fn builtin_tool_name_value(value: Option<&str>) -> String {
    value.filter(|value| !value.is_empty()).unwrap_or("_other_").to_string()
}

pub fn validate_metric_record(record: &MetricRecord) -> Result<(), MetricBuildError> {
    let Some(spec) = registry().metric(&record.name) else {
        return Err(MetricBuildError::UnknownMetric(record.name.clone()));
    };

    let actual = record.value.metric_kind();
    if spec.kind != actual {
        return Err(MetricBuildError::KindMismatch {
            metric: record.name.clone(),
            expected: spec.kind,
            actual,
        });
    }

    let mut seen = HashSet::new();
    for attr in &record.attributes {
        if !seen.insert(attr.key.as_str()) {
            return Err(MetricBuildError::DuplicateAttribute {
                metric: record.name.clone(),
                attribute: attr.key.clone(),
            });
        }

        if !spec.attributes.iter().any(|allowed| allowed == &attr.key) {
            return Err(MetricBuildError::UnsupportedAttribute {
                metric: record.name.clone(),
                attribute: attr.key.clone(),
            });
        }

        if let Some(attr_spec) = registry().attribute(&attr.key)
            && !attr_spec.accepts_value(&attr.value)
        {
            return Err(MetricBuildError::InvalidAttributeValue {
                metric: record.name.clone(),
                attribute: attr.key.clone(),
                value: attr.value.clone(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetricValue;
    use crate::testing::{
        expect_counter_metric,
        expect_float_counter_metric,
        expect_gauge_metric,
        expect_histogram_metric,
        expect_metric_attrs,
    };

    fn assert_metric_shape(record: MetricRecord, name: &str, value: MetricValue, attributes: &[(&str, &str)]) {
        assert_eq!(record.name, name);
        assert_eq!(record.value, value);
        expect_metric_attrs(&record, attributes);
    }

    #[test]
    fn builds_schema_valid_metric_records() {
        let record = counter("model_invocations_total", 1)
            .attribute("model_class", "anthropic_sonnet")
            .expect_valid();

        assert_eq!(record.name, "model_invocations_total");
        assert_eq!(record.value, MetricValue::Counter(1));
    }

    #[test]
    fn preserves_resource_attributes_for_export_only_metadata() {
        let record = gauge("telemetry.sdk.up", 1.0)
            .resource_attribute("replayed", "true")
            .expect_valid();

        assert!(
            record
                .resource_attributes
                .iter()
                .any(|attr| { attr.key == "replayed" && attr.value == "true" })
        );
    }

    #[test]
    fn typed_constructors_build_schema_valid_records() {
        assert_metric_shape(
            cli_session_started(OsType::Macos, InstallSource::Internal, ClientApplication::ChatCliV2),
            "cli_session_started_total",
            MetricValue::Counter(1),
            &[
                ("version_minor_bucket", "current"),
                ("os_type", "macos"),
                ("install_source", "internal"),
                ("client_application", "chat_cli_v2"),
            ],
        );
        assert_metric_shape(
            cli_session_started_from_names("macos", "internal", Some("chat_cli_v2")),
            "cli_session_started_total",
            MetricValue::Counter(1),
            &[
                ("version_minor_bucket", "current"),
                ("os_type", "macos"),
                ("install_source", "internal"),
                ("client_application", "chat_cli_v2"),
            ],
        );

        assert_metric_shape(
            chat_session_started(Mode::Plan, ClientApplication::ChatCliV2),
            "chat_session_started_total",
            MetricValue::Counter(1),
            &[
                ("version_minor_bucket", "current"),
                ("mode", "plan"),
                ("client_application", "chat_cli_v2"),
            ],
        );
        assert_metric_shape(
            chat_session_started_from_context(Some("V2"), Some("kiro_planner"), Some("chat_cli_v2")),
            "chat_session_started_total",
            MetricValue::Counter(1),
            &[
                ("version_minor_bucket", "current"),
                ("mode", "plan"),
                ("client_application", "chat_cli_v2"),
            ],
        );

        assert_metric_shape(
            cli_session_completed(ExitReason::Clean, AgentKind::Kas),
            "chat_cli.session.completed",
            MetricValue::Counter(1),
            &[("exit_reason", "clean"), ("agent_kind", "kas")],
        );
        assert_metric_shape(
            cli_session_completed_from_names("clean", "kas"),
            "chat_cli.session.completed",
            MetricValue::Counter(1),
            &[("exit_reason", "clean"), ("agent_kind", "kas")],
        );

        assert_metric_shape(
            bedrock_request_error(
                ModelClass::AnthropicSonnet,
                Operation::Stream,
                ErrorKind::Throttling,
                StatusClass::Class5xx,
            ),
            "chat_cli.bedrock.request.errors",
            MetricValue::Counter(1),
            &[
                ("model_class", "anthropic_sonnet"),
                ("operation", "stream"),
                ("error_kind", "throttling"),
                ("status_class", "5xx"),
            ],
        );
        assert_metric_shape(
            bedrock_stream_request_error_from_reason(Some("claude-4-sonnet"), Some("throttling"), Some(503)),
            "chat_cli.bedrock.request.errors",
            MetricValue::Counter(1),
            &[
                ("model_class", "anthropic_sonnet"),
                ("operation", "stream"),
                ("error_kind", "throttling"),
                ("status_class", "5xx"),
            ],
        );

        assert_metric_shape(
            tool_call_total(ToolOrigin::Builtin, Some("fs_read"), Outcome::Success),
            "tool_call_total",
            MetricValue::Counter(1),
            &[
                ("tool_origin", "builtin"),
                ("outcome", "success"),
                ("builtin_tool_name", "fs_read"),
            ],
        );
        let tool_invocation = ToolInvocation::new(Some("fs_read"), None, false, true, Some(true), Some(true));
        assert_metric_shape(
            tool_call_total_for_invocation(tool_invocation),
            "tool_call_total",
            MetricValue::Counter(1),
            &[
                ("tool_origin", "builtin"),
                ("outcome", "success"),
                ("builtin_tool_name", "fs_read"),
            ],
        );
        assert_metric_shape(
            tool_invocations_for_invocation(tool_invocation),
            "kiro_cli_tool_invocations",
            MetricValue::Counter(1),
            &[("tool_origin", "builtin"), ("outcome", "success")],
        );
        assert_metric_shape(
            tool_execution_duration_ms_for_invocation(25.0, tool_invocation).expect("duration metric"),
            "kiro_cli_tool_execution_duration_ms",
            MetricValue::Histogram(25.0),
            &[("tool_origin", "builtin"), ("is_success", "true")],
        );

        assert_metric_shape(
            user_turns(
                ModelClass::OpenAiGpt5,
                ClientApplication::ChatCliV2,
                ResultKind::Success,
                false,
                Mode::Plan,
            ),
            "kiro_cli_user_turns",
            MetricValue::Counter(1),
            &[
                ("model_class", "openai_gpt5"),
                ("client_application", "chat_cli_v2"),
                ("result", "success"),
                ("is_subagent", "false"),
                ("mode", "plan"),
            ],
        );

        assert_metric_shape(
            tokens_consumed(
                42,
                ModelClass::AnthropicHaiku,
                TokenType::InputCacheRead,
                ClientApplication::ChatCliV2,
                true,
            ),
            "kiro_cli_tokens_consumed",
            MetricValue::Counter(42),
            &[
                ("model_class", "anthropic_haiku"),
                ("token_type", "input_cache_read"),
                ("client_application", "chat_cli_v2"),
                ("is_subagent", "true"),
            ],
        );

        let invocation = InvocationContext::from_names(Some("claude-4-sonnet"), Some("kas"), true);
        let economics = estimated_token_economics_records(invocation, TokenUsage {
            uncached_input_tokens: 10,
            cache_read_input_tokens: 2,
            cache_write_input_tokens: 3,
            output_tokens: 5,
        });
        assert_eq!(
            economics
                .iter()
                .filter(|record| record.name == "kiro_cli_tokens_consumed")
                .count(),
            4
        );
        assert!(
            economics
                .iter()
                .any(|record| record.name == "kiro_cli_estimated_cost_usd")
        );
        assert!(
            economics
                .iter()
                .any(|record| record.name == "kiro_cli_pricing_table_active")
        );

        assert_metric_shape(
            mcp_server_init_total_from_name("awslabs.tools", None),
            "kiro_cli_mcp_server_init_total",
            MetricValue::Counter(1),
            &[("mcp_server_class", "official_third_party"), ("outcome", "success")],
        );
        assert_metric_shape(
            mcp_server_connected_total_from_name("awslabs.tools", None).expect("successful init"),
            "mcp_server_connected_total",
            MetricValue::Counter(1),
            &[("mcp_server_class", "official_third_party")],
        );
        assert_metric_shape(
            session_outcome_from_goal_terminal_state("completed"),
            "session_outcome_total",
            MetricValue::Counter(1),
            &[("session_outcome", "task_completed")],
        );
        assert_metric_shape(
            process_memory_rss_from_names(1024.0, "2.4.0", Some("kas")),
            "chat_cli.process.memory.rss",
            MetricValue::Gauge(1024.0),
            &[("version_minor_bucket", "current"), ("agent_kind", "kas")],
        );
        assert_metric_shape(
            process_cpu_utilization_from_names(0.5, "2.4.0", Some("kas"), ProcessState::Idle),
            "chat_cli.process.cpu.utilization",
            MetricValue::Histogram(0.5),
            &[
                ("version_minor_bucket", "current"),
                ("agent_kind", "kas"),
                ("state", "idle"),
            ],
        );
        assert_metric_shape(
            pii_redaction_run(
                Redactor::Default,
                EventClass::Log,
                TelemetryChannel::Otel,
                RedactionResult::Scrubbed,
            ),
            "pii_redaction_runs_total",
            MetricValue::Counter(1),
            &[
                ("redactor", "default"),
                ("event_class", "log"),
                ("channel", "otel"),
                ("redaction_result", "scrubbed"),
            ],
        );
        assert_metric_shape(
            pii_redaction_match(2, PiiType::Email, FieldClass::Prompt),
            "pii_redaction_matches_total",
            MetricValue::Counter(2),
            &[("pii_type", "email"), ("field_class", "prompt")],
        );
        assert_metric_shape(
            pii_redaction_error(Redactor::Default, ErrorKind::Other, RedactionFailAction::Dropped),
            "pii_redaction_errors_total",
            MetricValue::Counter(1),
            &[
                ("redactor", "default"),
                ("error_kind", "other"),
                ("fail_action", "dropped"),
            ],
        );
        assert_metric_shape(
            consent_record_integrity(ConsentCheckKind::Perms, ConsentIntegrityResult::Ok),
            "consent_record_integrity_total",
            MetricValue::Counter(1),
            &[("check_kind", "perms"), ("integrity_result", "ok")],
        );
        assert_metric_shape(
            telemetry_export_send_attempt(
                TelemetryExporter::Otel,
                TelemetrySignal::Metrics,
                ExportOutcome::Success,
            ),
            "telemetry.exporter.send.attempts",
            MetricValue::Counter(1),
            &[
                ("exporter", "otel"),
                ("signal", "metrics"),
                ("export_outcome", "success"),
            ],
        );
        assert_metric_shape(
            telemetry_exporter_dropped(TelemetryExporter::Otel, TelemetrySignal::Logs, DropReason::QueueFull),
            "telemetry.exporter.dropped",
            MetricValue::Counter(1),
            &[("exporter", "otel"), ("signal", "logs"), ("drop_reason", "queue_full")],
        );
        assert_metric_shape(
            telemetry_emit_failure(TelemetrySubsystem::Exporter, EmitFailureKind::Io),
            "telemetry.emit.failures",
            MetricValue::Counter(1),
            &[("subsystem", "exporter"), ("failure_kind", "io")],
        );
        assert_metric_shape(
            telemetry_sdk_up(Partition::Aws, OsType::Macos, ReleaseChannel::Stable),
            "telemetry.sdk.up",
            MetricValue::Gauge(1.0),
            &[
                ("partition", "aws"),
                ("os_type", "macos"),
                ("release_channel", "stable"),
            ],
        );
        assert_metric_shape(
            telemetry_flush_on_exit_dropped(3, ShutdownPath::Clean),
            "telemetry.flush_on_exit.dropped_total",
            MetricValue::Counter(3),
            &[("shutdown_path", "clean")],
        );
        assert_metric_shape(
            meta_meter_up(Partition::Aws, OsType::Macos),
            "meta_meter.up",
            MetricValue::Gauge(1.0),
            &[("partition", "aws"), ("os_type", "macos")],
        );

        let stream_records = bedrock_stream_timing_records(BedrockStreamMetrics {
            model_id: Some("claude-4-sonnet"),
            context_file_length: Some(4_000),
            tools_enabled: true,
            time_to_first_chunk_ms: Some(125.0),
            time_between_chunks_ms: Some(&[40.0, f64::NAN, 0.0]),
            request_duration_seconds: Some(0.8),
            completion_reason: crate::log::CompletionReason::ToolUse,
            request_outcome: Outcome::Success,
        });
        assert_eq!(stream_records.len(), 4);
        assert!(stream_records.iter().any(|record| {
            record.name == "chat_cli.bedrock.stream.ttft"
                && record.value == MetricValue::Histogram(0.125)
                && record
                    .attributes
                    .iter()
                    .any(|attribute| attribute.key == "prompt_size_bucket" && attribute.value == "small")
        }));
        assert!(stream_records.iter().any(|record| {
            record.name == "chat_cli.bedrock.request.duration" && record.value == MetricValue::Histogram(0.8)
        }));

        let response = ModelResponseMetrics::new(
            InvocationContext::new(ModelClass::AnthropicSonnet, ClientApplication::ChatCliV2, false),
            ResultKind::Success,
            Mode::Interactive,
            ChatConversationKind::Interactive,
            crate::log::CompletionReason::Stop,
            Outcome::Success,
        )
        .legacy_event(Some(LegacyEventType::ChatAddedMessage))
        .context_file_length(Some(4_000))
        .time_to_first_chunk_ms(Some(125.0))
        .time_between_chunks_ms(Some(&[40.0]))
        .request_duration_seconds(Some(0.8))
        .token_usage(TokenUsage {
            uncached_input_tokens: 10,
            cache_read_input_tokens: 2,
            cache_write_input_tokens: 3,
            output_tokens: 5,
        });
        let response_records = model_response_records(response);
        assert!(response_records.iter().any(|record| {
            record.name == "kiro_cli_user_turns"
                && record.value == MetricValue::Counter(1)
                && record
                    .attributes
                    .iter()
                    .any(|attribute| attribute.key == "client_application" && attribute.value == "chat_cli_v2")
        }));
        assert!(
            response_records
                .iter()
                .any(|record| record.name == "model_invocations_total")
        );
        assert!(
            response_records
                .iter()
                .any(|record| record.name == "kiro_cli_time_to_first_chunk_ms")
        );
        assert!(response_records.iter().any(|record| {
            record.name == "chat_cli.bedrock.stream.ttft"
                && record.value == MetricValue::Histogram(0.125)
                && record
                    .attributes
                    .iter()
                    .any(|attribute| attribute.key == "model_class" && attribute.value == "anthropic_sonnet")
        }));
        assert!(
            response_records
                .iter()
                .any(|record| record.name == "kiro_cli_tokens_consumed")
        );
        assert!(
            response_records
                .iter()
                .any(|record| record.name == "kiro_cli_cache_hit_ratio")
        );

        let process_records = process_health_records(ProcessHealthSnapshot::new(
            128.0,
            12.5,
            7.5,
            "2.4.0",
            AgentKind::Kas,
            ProcessState::Other,
        ));
        assert_eq!(process_records.len(), 2);
        assert!(process_records.iter().any(|record| {
            record.name == "chat_cli.process.memory.rss" && record.value == MetricValue::Gauge(128.0 * 1024.0 * 1024.0)
        }));
        assert!(process_records.iter().any(|record| {
            record.name == "chat_cli.process.cpu.utilization" && record.value == MetricValue::Histogram(0.2)
        }));
    }

    #[test]
    fn typed_record_inputs_build_common_metrics() {
        assert_metric_shape(
            cli_session_started_record(CliSessionStarted::from_names("macos", "internal", Some("chat_cli_v2"))),
            "cli_session_started_total",
            MetricValue::Counter(1),
            &[
                ("version_minor_bucket", "current"),
                ("os_type", "macos"),
                ("install_source", "internal"),
                ("client_application", "chat_cli_v2"),
            ],
        );
        assert_metric_shape(
            chat_session_started_record(ChatSessionStarted::from_context(
                Some("V2"),
                Some("/quick-plan"),
                Some("kas"),
            )),
            "chat_session_started_total",
            MetricValue::Counter(1),
            &[
                ("version_minor_bucket", "current"),
                ("mode", "plan"),
                ("client_application", "chat_cli_v3"),
            ],
        );
        assert_metric_shape(
            cli_session_completed_record(CliSessionCompleted::from_names("clean", "v3")),
            "chat_cli.session.completed",
            MetricValue::Counter(1),
            &[("exit_reason", "clean"), ("agent_kind", "kas")],
        );
        assert_metric_shape(
            daily_heartbeat_record(DailyHeartbeat::from_names(Some("kas"), Some("toolbox (2.0.0)"))),
            "kiro_cli_daily_heartbeat",
            MetricValue::Counter(1),
            &[("client_application", "chat_cli_v3"), ("install_method", "internal")],
        );
        assert_metric_shape(
            model_invocation_record(ModelInvocation::from_id(Some("claude-4-sonnet"))),
            "model_invocations_total",
            MetricValue::Counter(1),
            &[("model_class", "anthropic_sonnet")],
        );
        assert_metric_shape(
            empty_response_retry_record(EmptyResponseRetry::from_id(Some("claude-4-sonnet"), Outcome::Recovered)),
            "chat_cli.bedrock.empty_response.retries",
            MetricValue::Counter(1),
            &[("model_class", "anthropic_sonnet"), ("outcome", "recovered")],
        );
        assert_metric_shape(
            bedrock_request_error_record(BedrockRequestError::from_stream_reason(
                Some("claude-4-sonnet"),
                Some("AccessDeniedException"),
                Some(403),
            )),
            "chat_cli.bedrock.request.errors",
            MetricValue::Counter(1),
            &[
                ("model_class", "anthropic_sonnet"),
                ("operation", "stream"),
                ("error_kind", "access_denied"),
                ("status_class", "4xx"),
            ],
        );
        assert_metric_shape(
            retry_attempt_record(RetryAttempt::from_attempt(Upstream::Rts, RetryReason::Throttled, 3)),
            "chat_cli.retry.attempts",
            MetricValue::Counter(1),
            &[
                ("upstream", "rts"),
                ("retry_reason", "throttled"),
                ("attempt_number_bucket", "3+"),
            ],
        );
        assert_metric_shape(
            retry_attempt_from_attempt(Upstream::Kas, RetryReason::Other, 2),
            "chat_cli.retry.attempts",
            MetricValue::Counter(1),
            &[
                ("upstream", "kas"),
                ("retry_reason", "other"),
                ("attempt_number_bucket", "2"),
            ],
        );
        assert_metric_shape(
            retry_exhausted_record(RetryExhausted::from_reason(
                Upstream::Rts,
                Some("QuotaBreachError"),
                None,
            )),
            "chat_cli.retry.exhausted",
            MetricValue::Counter(1),
            &[("upstream", "rts"), ("final_error_kind", "throttling")],
        );
        assert_metric_shape(
            retry_exhausted_from_reason(Upstream::Kas, Some("AccessDeniedException"), Some(403)),
            "chat_cli.retry.exhausted",
            MetricValue::Counter(1),
            &[("upstream", "kas"), ("final_error_kind", "access_denied")],
        );
        assert_metric_shape(
            feature_used_record(FeatureUsed::new(" Knowledge ")),
            "feature_used_total",
            MetricValue::Counter(1),
            &[("feature", "knowledge"), ("version_minor_bucket", "current")],
        );
        assert_metric_shape(
            slash_command_invoked_record(SlashCommandInvoked::new(" /review ")),
            "slash_command_invoked_total",
            MetricValue::Counter(1),
            &[("command", "/review"), ("version_minor_bucket", "current")],
        );
        assert_metric_shape(
            session_outcome_record(SessionOutcomeMetric::from_goal_terminal_state("reinjection_failed")),
            "session_outcome_total",
            MetricValue::Counter(1),
            &[("session_outcome", "error")],
        );
    }

    #[test]
    fn model_response_records_keep_timing_filters_explicit() {
        let response = ModelResponseMetrics::new(
            InvocationContext::new(ModelClass::AnthropicSonnet, ClientApplication::ChatCliV2, false),
            ResultKind::Success,
            Mode::Interactive,
            ChatConversationKind::Interactive,
            crate::log::CompletionReason::Stop,
            Outcome::Success,
        )
        .time_to_first_chunk_ms(Some(0.0))
        .time_between_chunks_ms(Some(&[0.0, -1.0, f64::NAN]))
        .request_duration_seconds(Some(0.0));

        let records = model_response_records(response);

        assert!(records.iter().any(|record| {
            record.name == "kiro_cli_time_to_first_chunk_ms" && record.value == MetricValue::Histogram(0.0)
        }));
        assert!(
            records
                .iter()
                .all(|record| record.name != "chat_cli.bedrock.stream.ttft")
        );
        assert!(
            records
                .iter()
                .all(|record| record.name != "chat_cli.bedrock.stream.inter_token_latency")
        );
        assert!(
            records
                .iter()
                .all(|record| record.name != "chat_cli.bedrock.stream.duration")
        );
        assert!(
            records
                .iter()
                .all(|record| record.name != "chat_cli.bedrock.request.duration")
        );
    }

    #[test]
    fn turn_context_constructors_centralize_v2_and_kas_mappings() {
        let context = TurnMetricContext::new(Some("claude-4-sonnet"), Some("kas"))
            .app_type(Some("KAS"))
            .subagent(true)
            .tangent(true);

        let response = ModelResponseMetrics::from_turn_context(context, TurnOutcome::Succeeded, true);
        assert_eq!(
            response.context,
            InvocationContext::new(ModelClass::AnthropicSonnet, ClientApplication::ChatCliV3, true)
        );
        assert_eq!(response.result, ResultKind::Success);
        assert_eq!(response.mode, Mode::Tangent);
        assert_eq!(response.conversation_type, ChatConversationKind::Subagent);
        assert_eq!(response.completion_reason, crate::log::CompletionReason::ToolUse);
        assert_eq!(response.request_outcome, Outcome::Success);
        assert!(response.tools_enabled);

        let completion = UserTurnCompletionMetrics::from_turn_context(context, TurnOutcome::Failed);
        assert_eq!(completion.result, ResultKind::Failed);
        assert_eq!(completion.mode, Mode::Tangent);
        assert_eq!(completion.conversation_type, ChatConversationKind::Subagent);

        assert_eq!(TurnOutcome::Cancelled.result_kind(), ResultKind::Cancelled);
        assert_eq!(TurnOutcome::Cancelled.request_outcome(), Outcome::Cancelled);
        assert_eq!(
            TurnOutcome::Failed.completion_reason(true),
            crate::log::CompletionReason::Error
        );
    }

    #[test]
    fn tool_use_records_bundle_legacy_aggregate_and_duration() {
        let tool_use = ToolUseMetrics::from_tool_context(Some("fs_read"), None, false, true, Some(true), Some(true))
            .legacy_event(Some(LegacyEventType::ToolUseSuggested))
            .execution_duration_ms(Some(25.0));

        let records = tool_use_records(tool_use);

        expect_counter_metric(&records, "tool_call_total", 1, &[
            ("tool_origin", "builtin"),
            ("outcome", "success"),
            ("builtin_tool_name", "fs_read"),
        ]);
        expect_counter_metric(&records, "kiro_cli_tool_invocations", 1, &[
            ("tool_origin", "builtin"),
            ("outcome", "success"),
        ]);
        expect_histogram_metric(&records, "kiro_cli_tool_execution_duration_ms", 25.0, &[
            ("tool_origin", "builtin"),
            ("is_success", "true"),
        ]);
    }

    #[test]
    fn tool_use_records_omit_legacy_and_duration_when_absent() {
        let tool_use = ToolUseMetrics::new(ToolInvocation::new(
            Some("external_tool"),
            None,
            true,
            false,
            None,
            None,
        ))
        .execution_duration_ms(Some(25.0));

        let records = tool_use_records(tool_use);

        assert!(records.iter().all(|record| record.name != "tool_call_total"));
        assert!(
            records
                .iter()
                .all(|record| record.name != "kiro_cli_tool_execution_duration_ms")
        );
        expect_counter_metric(&records, "kiro_cli_tool_invocations", 1, &[
            ("tool_origin", "mcp"),
            ("outcome", "denied"),
        ]);
    }

    #[test]
    fn tool_use_records_bucket_error_cancelled_and_invalid_durations() {
        let invalid_tool_use = ToolUseMetrics::new(ToolInvocation::new(
            Some("fs_write"),
            None,
            false,
            true,
            Some(false),
            Some(false),
        ))
        .emit_tool_call_total(true)
        .execution_duration_ms(Some(f64::NAN));

        let records = tool_use_records(invalid_tool_use);

        expect_counter_metric(&records, "tool_call_total", 1, &[
            ("tool_origin", "builtin"),
            ("outcome", "error"),
            ("builtin_tool_name", "fs_write"),
        ]);
        expect_counter_metric(&records, "kiro_cli_tool_invocations", 1, &[
            ("tool_origin", "builtin"),
            ("outcome", "error"),
        ]);
        assert!(
            records
                .iter()
                .all(|record| record.name != "kiro_cli_tool_execution_duration_ms")
        );

        let cancelled_tool_use = ToolUseMetrics::new(ToolInvocation::new(
            Some("fs_write"),
            None,
            false,
            true,
            Some(true),
            None,
        ))
        .execution_duration_ms(Some(0.0));

        let records = tool_use_records(cancelled_tool_use);

        expect_counter_metric(&records, "kiro_cli_tool_invocations", 1, &[
            ("tool_origin", "builtin"),
            ("outcome", "cancelled"),
        ]);
        assert!(
            records
                .iter()
                .all(|record| record.name != "kiro_cli_tool_execution_duration_ms")
        );
    }

    #[test]
    fn user_turn_completion_records_bundle_turn_economics_and_duration() {
        let context = InvocationContext::new(ModelClass::AnthropicSonnet, ClientApplication::ChatCliV3, true);
        let completion = UserTurnCompletionMetrics::new(
            context,
            ResultKind::Failed,
            Mode::Tangent,
            ChatConversationKind::Subagent,
        )
        .emit_user_turn_counter(true)
        .token_usage(TokenUsage {
            uncached_input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        })
        .estimated_cost_usd(Some(0.25))
        .duration_seconds(Some(12.0));

        let records = user_turn_completion_records(completion);

        expect_counter_metric(&records, "kiro_cli_user_turns", 1, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v3"),
            ("result", "failed"),
            ("is_subagent", "true"),
            ("mode", "tangent"),
        ]);
        expect_counter_metric(&records, "kiro_cli_tokens_consumed", 10, &[
            ("model_class", "anthropic_sonnet"),
            ("token_type", "input_uncached"),
            ("client_application", "chat_cli_v3"),
            ("is_subagent", "true"),
        ]);
        expect_counter_metric(&records, "kiro_cli_tokens_consumed", 5, &[
            ("model_class", "anthropic_sonnet"),
            ("token_type", "output"),
            ("client_application", "chat_cli_v3"),
            ("is_subagent", "true"),
        ]);
        expect_float_counter_metric(&records, "kiro_cli_estimated_cost_usd", 0.25, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v3"),
            ("is_subagent", "true"),
        ]);
        expect_gauge_metric(&records, "kiro_cli_pricing_table_active", PRICING_TABLE_VERSION, &[]);
        expect_histogram_metric(&records, "kiro_cli_user_turn_duration_seconds", 12.0, &[
            ("model_class", "anthropic_sonnet"),
            ("chat_conversation_type", "subagent"),
            ("is_subagent", "true"),
            ("mode", "tangent"),
        ]);
    }

    #[test]
    fn user_turn_completion_records_leave_economics_behind_without_turn_counter() {
        let context = InvocationContext::new(ModelClass::AnthropicSonnet, ClientApplication::ChatCliV2, false);
        let completion = UserTurnCompletionMetrics::new(
            context,
            ResultKind::Success,
            Mode::Interactive,
            ChatConversationKind::Interactive,
        )
        .token_usage(TokenUsage {
            uncached_input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        })
        .estimated_cost_usd(Some(0.25))
        .duration_seconds(Some(8.0));

        let records = user_turn_completion_records(completion);

        assert!(records.iter().all(|record| record.name != "kiro_cli_user_turns"
            && record.name != "kiro_cli_tokens_consumed"
            && record.name != "kiro_cli_estimated_cost_usd"
            && record.name != "kiro_cli_pricing_table_active"));
        expect_histogram_metric(&records, "kiro_cli_user_turn_duration_seconds", 8.0, &[
            ("model_class", "anthropic_sonnet"),
            ("chat_conversation_type", "interactive"),
            ("is_subagent", "false"),
            ("mode", "interactive"),
        ]);

        assert!(user_turn_completion_records(completion.duration_seconds(Some(0.0))).is_empty());
    }

    #[test]
    fn typed_dimensions_bucket_unknown_values() {
        assert_eq!(ClientApplication::from_name(Some("raw")).as_str(), "_other_");
        assert_eq!(ClientApplication::from_name(Some("kas")).as_str(), "chat_cli_v3");
        assert_eq!(ClientApplication::from_name(Some("v3")).as_str(), "chat_cli_v3");
        assert_eq!(TelemetryChannel::from_name("legacy_toolkit").as_str(), "legacy_toolkit");
        assert_eq!(TelemetryChannel::from_name("raw").as_str(), "_other_");
        assert_eq!(Partition::from_name("aws-us-gov").as_str(), "aws-us-gov");
        assert_eq!(OsType::from_name("darwin").as_str(), "_other_");
        assert_eq!(InstallSource::from_name("installer").as_str(), "_other_");
        assert_eq!(Mode::from_chat_session(None, Some("/quick-plan")).as_str(), "plan");
        assert_eq!(ExitReason::from_name("signal").as_str(), "_other_");
        assert_eq!(AgentKind::from_name("v3").as_str(), "kas");
        assert_eq!(InstallSource::from_name("toolbox (2.0.0)").as_str(), "internal");
        assert_eq!(SubagentDepthBucket::from_depth(0).as_str(), "_other_");
        assert_eq!(SubagentDepthBucket::from_depth(1).as_str(), "1");
        assert_eq!(SubagentDepthBucket::from_depth(2).as_str(), "2");
        assert_eq!(SubagentDepthBucket::from_depth(3).as_str(), "3+");
        assert_eq!(
            ModelClass::from_model_id(Some("claude-3.7-sonnet")).as_str(),
            "anthropic_sonnet"
        );
        assert_eq!(StatusClass::from_status_code(Some(429)).as_str(), "4xx");
        assert_eq!(
            ErrorKind::from_reason(Some("ThrottlingException"), Some(429)).as_str(),
            "throttling"
        );
        assert_eq!(Upstream::from_name("v3").as_str(), "kas");
        assert_eq!(Upstream::from_name("raw").as_str(), "_other_");
        assert_eq!(
            RetryReason::from_reason(Some("connection reset")).as_str(),
            "connection"
        );
        assert_eq!(
            RetryReason::from_reason(Some("empty response")).as_str(),
            "empty_response"
        );
        assert_eq!(RetryReason::from_name("_other_").as_str(), "_other_");
        assert_eq!(AttemptNumberBucket::from_attempt(0).as_str(), "_other_");
        assert_eq!(AttemptNumberBucket::from_attempt(1).as_str(), "1");
        assert_eq!(AttemptNumberBucket::from_attempt(2).as_str(), "2");
        assert_eq!(AttemptNumberBucket::from_attempt(3).as_str(), "3+");
        assert_eq!(
            PromptSizeBucket::from_context_file_length(Some(40_001)).as_str(),
            "large"
        );
        assert_eq!(
            McpServerClass::from_server_name("amazon-internal").as_str(),
            "internal_amazon"
        );
        assert_eq!(
            Outcome::from_mcp_init_failure(Some("initialize timeout")).as_str(),
            "timeout"
        );
        assert_eq!(
            SessionOutcome::from_goal_terminal_state("reinjection_failed").as_str(),
            "error"
        );
    }

    #[test]
    fn mcp_server_init_record_buckets_metric_dimensions() {
        let input = McpServerInit::from_name("local-server", Some("request timed out"));
        assert_eq!(input.server_class, McpServerClass::UserDefined);
        assert_eq!(input.outcome, Outcome::Timeout);

        let record = mcp_server_init_total_record(input);

        assert_metric_shape(record, "kiro_cli_mcp_server_init_total", MetricValue::Counter(1), &[
            ("mcp_server_class", "user_defined"),
            ("outcome", "timeout"),
        ]);
        assert!(mcp_server_connected_total_record(input).is_none());

        let records = mcp_server_init_records(McpServerInit::from_name("code", None));
        expect_counter_metric(&records, "kiro_cli_mcp_server_init_total", 1, &[
            ("mcp_server_class", "builtin_code"),
            ("outcome", "success"),
        ]);
        expect_counter_metric(&records, "mcp_server_connected_total", 1, &[(
            "mcp_server_class",
            "builtin_code",
        )]);
    }

    #[test]
    fn context_usage_record_is_gauge_and_filters_values() {
        let input = ContextUsageMetric::from_names(42.5, Some("claude-4-sonnet"), Some("kas"), true);
        let record = context_usage_percentage_record(input).expect("valid context usage");

        // Context usage is a point-in-time 0-100 reading, so it is emitted as a
        // gauge (renders the 0-100 range directly) rather than a histogram with
        // ms-scale default buckets.
        assert_metric_shape(
            record,
            "kiro_cli_context_usage_percentage",
            MetricValue::Gauge(42.5),
            &[
                ("model_class", "anthropic_sonnet"),
                ("client_application", "chat_cli_v3"),
                ("is_subagent", "true"),
            ],
        );

        assert!(
            context_usage_percentage_record(ContextUsageMetric::from_names(
                -1.0,
                Some("claude-4-sonnet"),
                Some("kas"),
                true,
            ))
            .is_none()
        );
        assert!(
            context_usage_percentage_record(ContextUsageMetric::from_names(
                f64::NAN,
                Some("claude-4-sonnet"),
                Some("kas"),
                true,
            ))
            .is_none()
        );
        assert!(
            context_usage_percentage_record(ContextUsageMetric::from_names(
                f64::INFINITY,
                Some("claude-4-sonnet"),
                Some("kas"),
                true,
            ))
            .is_none()
        );
        assert!(
            context_usage_percentage_record(ContextUsageMetric::from_names(
                f64::NEG_INFINITY,
                Some("claude-4-sonnet"),
                Some("kas"),
                true,
            ))
            .is_none()
        );
    }

    #[test]
    fn token_economics_surface_filters_empty_values() {
        let context = InvocationContext::new(ModelClass::AnthropicSonnet, ClientApplication::ChatCliV2, false);
        let records = token_economics_records(
            context,
            TokenUsage {
                output_tokens: 7,
                ..Default::default()
            },
            Some(0.000_105),
        );

        let token_records = records
            .iter()
            .filter(|record| record.name == "kiro_cli_tokens_consumed")
            .collect::<Vec<_>>();
        assert_eq!(token_records.len(), 1);
        assert_eq!(token_records[0].value, MetricValue::Counter(7));
        assert_eq!(token_records[0].attributes.len(), 4);
        assert!(
            cache_hit_ratio_from_usage(context, ChatConversationKind::Interactive, TokenUsage::default()).is_none()
        );
    }

    #[test]
    fn lifecycle_enums_serde_as_schema_strings() {
        assert_eq!(serde_json::to_string(&Mode::Plan).unwrap(), "\"plan\"");
        assert_eq!(serde_json::from_str::<Mode>("\"kiro_planner\"").unwrap(), Mode::Plan);
        assert_eq!(serde_json::from_str::<AgentKind>("\"v3\"").unwrap(), AgentKind::Kas);
        assert_eq!(
            serde_json::from_str::<ExitReason>("\"unexpected\"").unwrap(),
            ExitReason::Other
        );
        assert_eq!(
            serde_json::from_str::<InstallSource>("\"toolbox (2.0.0)\"").unwrap(),
            InstallSource::Internal
        );
    }

    #[test]
    fn govcloud_posture_constructors_build_schema_valid_records() {
        assert_metric_shape(
            govcloud_channel_disabled_record(GovcloudChannelDisabled::from_names("legacy_toolkit", "aws-us-gov")),
            "govcloud_channel_disabled_total",
            MetricValue::Counter(1),
            &[
                ("channel", "legacy_toolkit"),
                ("partition", "aws-us-gov"),
                ("reason", "govcloud_disabled"),
            ],
        );

        assert_metric_shape(
            govcloud_channel_leak_record(GovcloudChannelLeak::from_name("legacy_codewhisperer")),
            "govcloud_channel_leak_total",
            MetricValue::Counter(1),
            &[("channel", "legacy_codewhisperer")],
        );
    }

    #[test]
    fn reliability_constructors_build_schema_valid_records() {
        assert_metric_shape(
            crash_total(CrashKind::Panic, OsType::Macos, HostArch::Aarch64),
            "chat_cli.crash.total",
            MetricValue::Counter(1),
            &[("crash_kind", "panic"), ("os_type", "macos"), ("host_arch", "aarch64")],
        );
        assert_metric_shape(
            crash_total_from_names("segfault", "linux", "x86_64"),
            "chat_cli.crash.total",
            MetricValue::Counter(1),
            &[
                ("crash_kind", "segfault"),
                ("os_type", "linux"),
                ("host_arch", "x86_64"),
            ],
        );

        assert_metric_shape(
            startup_failure(FailureStage::DbMigrate, OsType::Linux),
            "chat_cli.startup.failures",
            MetricValue::Counter(1),
            &[("failure_stage", "db_migrate"), ("os_type", "linux")],
        );

        assert_metric_shape(
            startup_duration(0.5, VersionMinorBucket::Current, true, OsType::Macos),
            "chat_cli.startup.duration",
            MetricValue::Histogram(0.5),
            &[
                ("version_minor_bucket", "current"),
                ("cold_start", "true"),
                ("os_type", "macos"),
            ],
        );

        assert_metric_shape(
            agent_loop_iteration_duration(2.0, LoopPhase::ToolExec),
            "chat_cli.agent.loop.iteration_duration",
            MetricValue::Histogram(2.0),
            &[("loop_phase", "tool_exec")],
        );

        assert_metric_shape(
            agent_loop_stuck(StuckPhase::ModelCall, StuckDetection::Watchdog),
            "chat_cli.agent.loop.stuck",
            MetricValue::Counter(1),
            &[("stuck_phase", "model_call"), ("detection", "watchdog")],
        );

        assert_metric_shape(
            upstream_dependency_up(true, Dependency::Bedrock, Partition::Aws),
            "chat_cli.upstream.dependency.up",
            MetricValue::Gauge(1.0),
            &[("dependency", "bedrock"), ("partition", "aws")],
        );
        assert_metric_shape(
            upstream_dependency_up(false, Dependency::OauthIdp, Partition::AwsUsGov),
            "chat_cli.upstream.dependency.up",
            MetricValue::Gauge(0.0),
            &[("dependency", "oauth_idp"), ("partition", "aws-us-gov")],
        );
    }

    #[test]
    fn unknown_reliability_dimension_names_bucket_to_other() {
        assert_eq!(CrashKind::from_name("meltdown").as_str(), "_other_");
        assert_eq!(HostArch::from_name("riscv").as_str(), "_other_");
        assert_eq!(FailureStage::from_name("warp").as_str(), "_other_");
        assert_eq!(LoopPhase::from_name("dream").as_str(), "_other_");
        assert_eq!(StuckPhase::from_name("nap").as_str(), "_other_");
        assert_eq!(StuckDetection::from_name("vibes").as_str(), "_other_");
        assert_eq!(Dependency::from_name("ssm").as_str(), "_other_");
    }

    #[test]
    fn process_health_detail_constructors_build_schema_valid_records() {
        assert_metric_shape(
            process_memory_growth_rate(1024.0, VersionMinorBucket::Current, AgentKind::Kas),
            "chat_cli.process.memory.growth_rate",
            MetricValue::Histogram(1024.0),
            &[("version_minor_bucket", "current"), ("agent_kind", "kas")],
        );
        assert_metric_shape(
            process_fds_open(64.0, VersionMinorBucket::Current, AgentKind::Kas),
            "chat_cli.process.fds.open",
            MetricValue::Gauge(64.0),
            &[("version_minor_bucket", "current"), ("agent_kind", "kas")],
        );
        assert_metric_shape(
            process_threads(12.0, VersionMinorBucket::Current, AgentKind::Kas),
            "chat_cli.process.threads",
            MetricValue::Gauge(12.0),
            &[("version_minor_bucket", "current"), ("agent_kind", "kas")],
        );
    }

    #[test]
    fn quality_outcome_constructors_build_schema_valid_records() {
        let feedback = user_feedback(Sentiment::Positive, FeedbackSurface::Chat);
        assert_metric_shape(feedback.clone(), "user_feedback_total", MetricValue::Counter(1), &[
            ("sentiment", "positive"),
            ("surface", "chat"),
        ]);
        // Pin the exact attribute set (no stray dimensions) per review feedback.
        assert_eq!(feedback.attributes.len(), 2);

        assert_metric_shape(
            user_feedback_from_names("negative", "review"),
            "user_feedback_total",
            MetricValue::Counter(1),
            &[("sentiment", "negative"), ("surface", "review")],
        );

        let regenerated = message_regenerated(ModelClass::AnthropicSonnet);
        assert_metric_shape(
            regenerated.clone(),
            "message_regenerated_total",
            MetricValue::Counter(1),
            &[("model_class", "anthropic_sonnet")],
        );
        assert_eq!(regenerated.attributes.len(), 1);

        assert_metric_shape(
            message_regenerated_from_id(Some("claude-4-sonnet")),
            "message_regenerated_total",
            MetricValue::Counter(1),
            &[("model_class", "anthropic_sonnet")],
        );
    }

    #[test]
    fn quality_outcome_dimensions_round_trip_known_and_unknown() {
        // Known values must round-trip through from_name -> as_str unchanged.
        assert_eq!(Sentiment::from_name("positive").as_str(), "positive");
        assert_eq!(Sentiment::from_name("negative").as_str(), "negative");
        assert_eq!(Sentiment::from_name("neutral").as_str(), "neutral");
        assert_eq!(FeedbackSurface::from_name("chat").as_str(), "chat");
        assert_eq!(FeedbackSurface::from_name("review").as_str(), "review");
        assert_eq!(FeedbackSurface::from_name("settings").as_str(), "settings");
        // Unknown values bucket to _other_.
        assert_eq!(Sentiment::from_name("ecstatic").as_str(), "_other_");
        assert_eq!(FeedbackSurface::from_name("voice").as_str(), "_other_");
    }

    #[test]
    fn security_posture_constructors_build_schema_valid_records() {
        let auth_failure = auth_credential_failure(
            AuthProvider::BuilderId,
            "ExpiredTokenException",
            Operation::Refresh,
            Partition::Aws,
        );
        assert_metric_shape(
            auth_failure.clone(),
            "auth_credential_failure_total",
            MetricValue::Counter(1),
            &[
                ("auth_provider", "builder_id"),
                ("error_code", "expiredtokenexception"),
                ("operation", "refresh"),
                ("partition", "aws"),
            ],
        );
        assert_eq!(auth_failure.attributes.len(), 4);

        assert_metric_shape(
            auth_unexpected_identity(Partition::Aws, Partition::AwsUsGov, Operation::Login),
            "auth_unexpected_identity_total",
            MetricValue::Counter(1),
            &[
                ("expected_partition", "aws"),
                ("actual_partition", "aws-us-gov"),
                ("operation", "login"),
            ],
        );

        assert_metric_shape(
            tls_validation_failure(DestinationClass::PublicInternet, TlsFailureReason::CertExpired),
            "tls_validation_failure_total",
            MetricValue::Counter(1),
            &[
                ("destination_class", "public_internet"),
                ("failure_reason", "cert_expired"),
            ],
        );

        assert_metric_shape(
            tool_egress_destinations(DestinationClass::AwsEndpoint, UrlScheme::Https, true),
            "tool_egress_destinations_total",
            MetricValue::Counter(1),
            &[
                ("destination_class", "aws_endpoint"),
                ("scheme", "https"),
                ("is_allowlisted", "true"),
            ],
        );
    }

    #[test]
    fn security_posture_dimensions_round_trip_known_and_unknown() {
        assert_eq!(AuthProvider::from_name("builder_id").as_str(), "builder_id");
        assert_eq!(AuthProvider::from_name("cognito").as_str(), "cognito");
        assert_eq!(AuthProvider::from_name("sso").as_str(), "sso");
        assert_eq!(TlsFailureReason::from_name("untrusted_root").as_str(), "untrusted_root");
        assert_eq!(DestinationClass::from_name("localhost").as_str(), "localhost");
        assert_eq!(UrlScheme::from_name("wss").as_str(), "wss");
        // Unknown values bucket to _other_.
        assert_eq!(AuthProvider::from_name("oauth").as_str(), "_other_");
        assert_eq!(TlsFailureReason::from_name("meltdown").as_str(), "_other_");
        assert_eq!(DestinationClass::from_name("moon").as_str(), "_other_");
        assert_eq!(UrlScheme::from_name("gopher").as_str(), "_other_");
    }

    #[test]
    fn adoption_gauge_constructors_build_schema_valid_records() {
        assert_metric_shape(
            active_users_daily(1234.0, InstallSource::Brew, ClientApplication::ChatCliV3, false),
            "active_users_daily",
            MetricValue::Gauge(1234.0),
            &[
                ("install_method", "brew"),
                ("client_application", "chat_cli_v3"),
                ("is_internal_amazon", "false"),
            ],
        );
        assert_metric_shape(
            active_users_weekly(5678.0, InstallSource::Internal, ClientApplication::ChatCliV3, true),
            "active_users_weekly",
            MetricValue::Gauge(5678.0),
            &[
                ("install_method", "internal"),
                ("client_application", "chat_cli_v3"),
                ("is_internal_amazon", "true"),
            ],
        );

        let monthly = active_users_monthly(42_000.0);
        assert_metric_shape(
            monthly.clone(),
            "active_users_monthly",
            MetricValue::Gauge(42_000.0),
            &[],
        );
        assert_eq!(monthly.attributes.len(), 0);

        assert_metric_shape(dau_mau_ratio(0.21), "dau_mau_ratio", MetricValue::Gauge(0.21), &[]);
        assert_metric_shape(
            new_users_daily(99.0, InstallSource::Download),
            "new_users_daily",
            MetricValue::Gauge(99.0),
            &[("install_source", "download")],
        );
        assert_metric_shape(
            client_version_seen(7.0, "2.6.1", ReleaseChannel::Stable, OsType::Macos),
            "client_version_seen",
            MetricValue::Gauge(7.0),
            &[
                ("version_full", "2.6.1"),
                ("release_channel", "stable"),
                ("os_type", "macos"),
            ],
        );
        assert_metric_shape(
            version_adoption_pct(63.5, VersionMinorBucket::Current, ReleaseChannel::Stable),
            "version_adoption_pct",
            MetricValue::Gauge(63.5),
            &[("version_minor_bucket", "current"), ("release_channel", "stable")],
        );
        assert_metric_shape(
            stale_version_users(12.0, StalenessBucket::from_age_days(75)),
            "stale_version_users",
            MetricValue::Gauge(12.0),
            &[("staleness_bucket", "60-90")],
        );
        assert_metric_shape(
            feature_unique_users_weekly(8.0, "Tangent"),
            "feature_unique_users_weekly",
            MetricValue::Gauge(8.0),
            &[("feature", "tangent")],
        );
        assert_metric_shape(
            tool_using_sessions_pct(72.0),
            "tool_using_sessions_pct",
            MetricValue::Gauge(72.0),
            &[],
        );
        assert_metric_shape(
            mode_active_users_weekly(15.0, Mode::Plan),
            "mode_active_users_weekly",
            MetricValue::Gauge(15.0),
            &[("mode", "plan")],
        );
        assert_metric_shape(
            upgrade_completed(
                VersionMinorBucket::Older,
                VersionMinorBucket::Current,
                UpgradeTrigger::Auto,
            ),
            "upgrade_completed_total",
            MetricValue::Counter(1),
            &[
                ("from_version_minor_bucket", "older"),
                ("to_version_minor_bucket", "current"),
                ("upgrade_trigger", "auto"),
            ],
        );
    }

    #[test]
    fn adoption_dimensions_round_trip_known_and_unknown() {
        assert_eq!(StalenessBucket::from_age_days(10).as_str(), "<30d");
        assert_eq!(StalenessBucket::from_age_days(45).as_str(), "30-60");
        assert_eq!(StalenessBucket::from_age_days(80).as_str(), "60-90");
        assert_eq!(StalenessBucket::from_age_days(365).as_str(), ">90");
        assert_eq!(StalenessBucket::from_name("30-60").as_str(), "30-60");
        assert_eq!(StalenessBucket::from_name("forever").as_str(), "_other_");
        assert_eq!(UpgradeTrigger::from_name("prompted").as_str(), "prompted");
        assert_eq!(UpgradeTrigger::from_name("sideload").as_str(), "_other_");
    }

    #[test]
    fn rejects_unknown_metrics() {
        let err = counter("missing_metric_total", 1)
            .build()
            .expect_err("metric should fail validation");

        assert_eq!(err, MetricBuildError::UnknownMetric("missing_metric_total".to_string()));
    }

    #[test]
    fn rejects_wrong_metric_kind() {
        let err = histogram("model_invocations_total", 1.0)
            .attribute("model_class", "anthropic_sonnet")
            .build()
            .expect_err("metric should fail validation");

        assert!(matches!(err, MetricBuildError::KindMismatch { .. }));
    }

    #[test]
    fn rejects_attributes_not_declared_for_metric() {
        let err = counter("model_invocations_total", 1)
            .attribute("client_application", "chat_cli_v2")
            .build()
            .expect_err("metric should fail validation");

        assert_eq!(err, MetricBuildError::UnsupportedAttribute {
            metric: "model_invocations_total".to_string(),
            attribute: "client_application".to_string(),
        });
    }

    #[test]
    fn rejects_unbounded_attribute_values() {
        let err = counter("model_invocations_total", 1)
            .attribute("model_class", "raw-model-id")
            .build()
            .expect_err("metric should fail validation");

        assert_eq!(err, MetricBuildError::InvalidAttributeValue {
            metric: "model_invocations_total".to_string(),
            attribute: "model_class".to_string(),
            value: "raw-model-id".to_string(),
        });
    }

    #[test]
    fn rejects_duplicate_attributes() {
        let err = counter("model_invocations_total", 1)
            .attribute("model_class", "anthropic_sonnet")
            .attribute("model_class", "anthropic_haiku")
            .build()
            .expect_err("metric should fail validation");

        assert_eq!(err, MetricBuildError::DuplicateAttribute {
            metric: "model_invocations_total".to_string(),
            attribute: "model_class".to_string(),
        });
    }
}
