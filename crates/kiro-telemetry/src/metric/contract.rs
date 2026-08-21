pub use kiro_telemetry_schema::TurnFailureReason;
use kiro_telemetry_schema::{
    SlashCommandMetricName,
    TopLevelCommandMetricName,
};

use super::{
    CloudSessionEvent,
    Engine,
    ErrorKind,
    OsType,
    ProcessRole,
    RenderKind,
    UiMode,
    counter,
    counter_f64,
    gauge,
    histogram,
    model_attr,
    version_attr,
};
use crate::MetricRecord;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionInterface {
    #[default]
    InteractiveCli,
    NoninteractiveCli,
    ExternalAcp,
}

impl SessionInterface {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InteractiveCli => "interactive_cli",
            Self::NoninteractiveCli => "noninteractive_cli",
            Self::ExternalAcp => "external_acp",
        }
    }
}

/// Bounded label for the ACP client driving the session, so Crew (`kirocrew`) is
/// isolatable within `external_acp` on cross-surface dashboards without unbounded
/// cardinality from the raw client name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpClient {
    /// Kiro CLI's own TUI/host.
    Cli,
    /// KiroCrew supervisor (client name `kirocrew`).
    Kirocrew,
    /// Any other identified ACP client.
    Other,
    /// No client name was reported.
    #[default]
    Unknown,
}

impl AcpClient {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Kirocrew => "kirocrew",
            Self::Other => "other",
            Self::Unknown => "unknown",
        }
    }

    /// Maps a raw ACP `clientInfo.name` to the bounded label. `None` (no client
    /// info, e.g. V1's direct emitters) maps to `Cli`.
    ///
    /// The first-party ACP client names — the built-in TUI (`kiro-tui`) and the
    /// one-shot client (`kiro-cli-non-interactive`) — map to `Cli`. These literals
    /// mirror `kiro-telemetry-observer`'s `ClientName` constants; they are duplicated
    /// (not imported) because that crate depends on this one, so importing would
    /// cycle. KiroCrew maps to `Kirocrew`; anything else identified is `Other`.
    pub fn from_client_name(name: Option<&str>) -> Self {
        match name {
            None => Self::Cli,
            Some(n) => match n.trim().to_ascii_lowercase().as_str() {
                "kirocrew" | "kiro-crew" | "kiro_crew" => Self::Kirocrew,
                "kiro-tui" | "kiro-cli-non-interactive" | "kiro-cli" | "kiro_cli" | "cli" => Self::Cli,
                "" => Self::Unknown,
                _ => Self::Other,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    #[default]
    Default,
    Plan,
    Spec,
    Autonomous,
    Custom,
}

impl AgentMode {
    pub fn from_id(value: Option<&str>) -> Self {
        let normalized = value
            .unwrap_or_default()
            .trim()
            .trim_start_matches('/')
            .to_ascii_lowercase()
            .replace('-', "_");
        match normalized.as_str() {
            "" | "default" | "kiro" | "kiro_default" | "vibe" | "interactive" => Self::Default,
            "plan" | "quick_plan" | "kiro_planner" | "planner" => Self::Plan,
            "spec" | "kiro_spec" => Self::Spec,
            "autonomous" => Self::Autonomous,
            _ => Self::Custom,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Plan => "plan",
            Self::Spec => "spec",
            Self::Autonomous => "autonomous",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuthMethod {
    BuilderId,
    IdentityCenter,
    Social,
    ExternalIdp,
    #[default]
    Unknown,
}

impl AuthMethod {
    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "builder_id" | "builderid" => Self::BuilderId,
            "identity_center" | "idc" | "sso" => Self::IdentityCenter,
            "social" | "google" | "github" => Self::Social,
            "external_idp" | "external" => Self::ExternalIdp,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuilderId => "builder_id",
            Self::IdentityCenter => "identity_center",
            Self::Social => "social",
            Self::ExternalIdp => "external_idp",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuthFlow {
    Pkce,
    DeviceCode,
    NotApplicable,
    #[default]
    Unknown,
}

impl AuthFlow {
    pub fn from_name(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "pkce" | "browser" => Self::Pkce,
            "device" | "device_code" => Self::DeviceCode,
            "not_applicable" | "none" => Self::NotApplicable,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pkce => "pkce",
            Self::DeviceCode => "device_code",
            Self::NotApplicable => "not_applicable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecutionContext {
    #[default]
    Main,
    Subagent,
}

impl ExecutionContext {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Subagent => "subagent",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMetricOrigin {
    Builtin,
    Mcp,
    #[default]
    Unknown,
}

impl ToolMetricOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Mcp => "mcp",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMetricOutcome {
    Success,
    Error,
    Denied,
    Cancelled,
    #[default]
    Unknown,
}

impl ToolMetricOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}

/// Permission posture when a chat session is created or loaded.
/// Changes made later in the session require a separate transition signal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustPosture {
    PromptOnDemand,
    TrustAllTools,
    #[default]
    Unknown,
}

impl TrustPosture {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PromptOnDemand => "prompt_on_demand",
            Self::TrustAllTools => "trust_all_tools",
            Self::Unknown => "unknown",
        }
    }

    pub const fn from_trust_all_tools(trust_all_tools: bool) -> Self {
        if trust_all_tools {
            Self::TrustAllTools
        } else {
            Self::PromptOnDemand
        }
    }
}

/// How authorization for one attempted tool execution was obtained.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPath {
    AutoAllowed,
    UserApproved,
    Denied,
    #[default]
    Unknown,
}

impl ApprovalPath {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutoAllowed => "auto_allowed",
            Self::UserApproved => "user_approved",
            Self::Denied => "denied",
            Self::Unknown => "unknown",
        }
    }
}

/// Coarse location of the resolved target of a filesystem tool.
///
/// Never carries a path. The point is to distinguish edits inside the project
/// the user is working in, which are cheap to undo, from edits that reach
/// outside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathScope {
    Workspace,
    OutsideWorkspace,
    Home,
    System,
    NotApplicable,
    #[default]
    Unknown,
}

impl PathScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::OutsideWorkspace => "outside_workspace",
            Self::Home => "home",
            Self::System => "system",
            Self::NotApplicable => "not_applicable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToolMetric<'a> {
    pub engine: Engine,
    pub origin: ToolMetricOrigin,
    pub builtin_tool_name: Option<&'a str>,
    pub outcome: ToolMetricOutcome,
    pub execution_context: ExecutionContext,
    pub approval_path: ApprovalPath,
    pub path_scope: PathScope,
}

impl<'a> ToolMetric<'a> {
    pub const fn new(
        engine: Engine,
        origin: ToolMetricOrigin,
        outcome: ToolMetricOutcome,
        execution_context: ExecutionContext,
    ) -> Self {
        Self {
            engine,
            origin,
            builtin_tool_name: None,
            outcome,
            execution_context,
            approval_path: ApprovalPath::Unknown,
            path_scope: PathScope::NotApplicable,
        }
    }

    pub const fn builtin_tool_name(mut self, name: Option<&'a str>) -> Self {
        self.builtin_tool_name = name;
        self
    }

    pub const fn approval_path(mut self, approval_path: ApprovalPath) -> Self {
        self.approval_path = approval_path;
        self
    }

    pub const fn path_scope(mut self, path_scope: PathScope) -> Self {
        self.path_scope = path_scope;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelRequestOutcome {
    Success,
    Failure,
    Cancelled,
    #[default]
    Unknown,
}

impl ModelRequestOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunOutcome {
    Success,
    UserInterrupt,
    Failure,
    #[default]
    Unknown,
}

impl RunOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::UserInterrupt => "user_interrupt",
            Self::Failure => "failure",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupFailureStage {
    RuntimeSetup,
    AgentLaunch,
    ProtocolInit,
    InterfaceInit,
    #[default]
    Unknown,
}

impl StartupFailureStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeSetup => "runtime_setup",
            Self::AgentLaunch => "agent_launch",
            Self::ProtocolInit => "protocol_init",
            Self::InterfaceInit => "interface_init",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum RetryOutcome {
    Recovered,
    Exhausted,
    Cancelled,
    #[default]
    Unknown,
}

impl RetryOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recovered => "recovered",
            Self::Exhausted => "exhausted",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}

/// Closed transient-failure class for the `transient_class` dimension. `Unknown`
/// is the deserialization fallback, so an out-of-set label degrades to a valid
/// schema value instead of failing validation at emit time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransientErrorClass {
    Throttle,
    ServerError,
    Network,
    #[default]
    #[serde(other)]
    Unknown,
}

impl TransientErrorClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Throttle => "throttle",
            Self::ServerError => "server_error",
            Self::Network => "network",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StallTier {
    Soft,
    Hard,
}

impl StallTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Hard => "hard",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StallEpisodeEnd {
    Resumed,
    HardCancelled,
    /// The warned stream ended in an error or EOF: the episode is over, but not
    /// because the stream recovered. Exists so every soft stall gets a matching
    /// episode end and the two series stay reconcilable.
    Failed,
    /// A cancel abandoned the warned stream before it recovered or died on its
    /// own — typically the user answering the stall notice with Ctrl+C, which is
    /// distinct threshold-tuning signal from a backend failure.
    Cancelled,
}

impl StallEpisodeEnd {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resumed => "resumed",
            Self::HardCancelled => "hard_cancelled",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Bounded ordinal of a stall-continuation attempt; the retry budget is 2, so any
/// out-of-range value maps to `unknown` rather than widening the enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum AttemptNumberBucket {
    First,
    Second,
    #[default]
    Unknown,
}

impl AttemptNumberBucket {
    pub const fn from_attempt(attempt: u32) -> Self {
        match attempt {
            1 => Self::First,
            2 => Self::Second,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::First => "1",
            Self::Second => "2",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum PartialOutput {
    Yes,
    No,
    #[default]
    Unknown,
}

impl PartialOutput {
    pub const fn from_flag(had_output: Option<bool>) -> Self {
        match had_output {
            Some(true) => Self::Yes,
            Some(false) => Self::No,
            None => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerSource {
    Registry,
    Global,
    Workspace,
    Agent,
    AcpInjected,
    #[default]
    Unknown,
}

impl McpServerSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Registry => "registry",
            Self::Global => "global",
            Self::Workspace => "workspace",
            Self::Agent => "agent",
            Self::AcpInjected => "acp_injected",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpInitOutcome {
    Success,
    Failure,
    #[default]
    Unknown,
}

impl McpInitOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GoalOutcome {
    Completed,
    Cancelled,
    IterationLimit,
    DispatchFailure,
    ReinjectionFailure,
    AgentError,
    #[default]
    Unknown,
}

impl GoalOutcome {
    pub fn from_terminal_state(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "completed" | "complete" => Self::Completed,
            "cancelled" | "canceled" => Self::Cancelled,
            "exhausted" | "iteration_limit" => Self::IterationLimit,
            "dispatch_failure" | "dispatch_failed" => Self::DispatchFailure,
            "reinjection_failure" | "reinjection_failed" => Self::ReinjectionFailure,
            "agent_error" | "failed" | "error" => Self::AgentError,
            _ => Self::Unknown,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::IterationLimit => "iteration_limit",
            Self::DispatchFailure => "dispatch_failure",
            Self::ReinjectionFailure => "reinjection_failure",
            Self::AgentError => "agent_error",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuthFailureReason {
    AuthorizationDenied,
    InvalidOrExpiredCredential,
    Network,
    Timeout,
    ServiceError,
    Configuration,
    Storage,
    IdentityMismatch,
    #[default]
    Unknown,
}

impl AuthFailureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthorizationDenied => "authorization_denied",
            Self::InvalidOrExpiredCredential => "invalid_or_expired_credential",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::ServiceError => "service_error",
            Self::Configuration => "configuration",
            Self::Storage => "storage",
            Self::IdentityMismatch => "identity_mismatch",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExportDropReason {
    Oversize,
    InvalidRecord,
    EncodingFailure,
    RetryExhausted,
    PermanentRejection,
    #[default]
    Unknown,
}

impl ExportDropReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Oversize => "oversize",
            Self::InvalidRecord => "invalid_record",
            Self::EncodingFailure => "encoding_failure",
            Self::RetryExhausted => "retry_exhausted",
            Self::PermanentRejection => "permanent_rejection",
            Self::Unknown => "unknown",
        }
    }
}

fn with_common_product_dimensions(
    builder: super::MetricBuilder,
    session_interface: SessionInterface,
    engine: Engine,
    os_type: Option<OsType>,
) -> super::MetricBuilder {
    let builder = builder
        .attribute("version_full", version_attr())
        .attribute("session_interface", session_interface.as_str())
        .attribute("agent_engine", engine.as_str());
    match os_type {
        Some(os_type) => builder.attribute("os_type", os_type.as_str()),
        None => builder,
    }
}

fn with_tool_dimensions(builder: super::MetricBuilder, metric: ToolMetric<'_>) -> super::MetricBuilder {
    let builder = builder
        .attribute("version_full", version_attr())
        .attribute("agent_engine", metric.engine.as_str())
        .attribute("tool_origin", metric.origin.as_str())
        .attribute("tool_outcome", metric.outcome.as_str())
        .attribute("approval_path", metric.approval_path.as_str())
        .attribute("path_scope", metric.path_scope.as_str())
        .attribute("execution_context", metric.execution_context.as_str());
    if metric.origin == ToolMetricOrigin::Builtin {
        builder.attribute(
            "builtin_tool_name",
            canonical_builtin_tool_name(metric.builtin_tool_name),
        )
    } else {
        builder
    }
}

fn canonical_builtin_tool_name(value: Option<&str>) -> &'static str {
    let Some(value) = value else {
        return "unknown";
    };
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "fs_read" | "fsread" | "read" => "fs_read",
        "fs_write" | "fswrite" | "write" => "fs_write",
        "execute_bash" | "execute_cmd" | "executecmd" | "shell" => "execute_bash",
        "summary" => "summary",
        "grep" => "grep",
        "glob" => "glob",
        "use_aws" | "aws" => "use_aws",
        "web_fetch" | "webfetch" => "web_fetch",
        "web_search" | "websearch" => "web_search",
        "code" => "code",
        "agent_crew" | "subagent" | "use_subagent" => "use_subagent",
        "session" | "session_management" | "sessionmanagement" | "sessions" => "session",
        "switch_to_execution" | "switchtoexecution" => "switch_to_execution",
        "introspect" => "introspect",
        "knowledge" => "knowledge",
        "tool_search" | "toolsearch" => "tool_search",
        "task" | "todo" | "todo_list" => "task",
        "goal" => "goal",
        _ => "unknown",
    }
}

pub fn record_run_started(session_interface: SessionInterface, engine: Engine, os_type: OsType) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_run_started_total", 1),
        session_interface,
        engine,
        Some(os_type),
    )
    .expect_valid()
}

pub fn record_login_success(auth_method: AuthMethod, auth_flow: AuthFlow) -> MetricRecord {
    counter("kiro_cli_login_success_total", 1)
        .attribute("version_full", version_attr())
        .attribute("auth_method", auth_method.as_str())
        .attribute("auth_flow", auth_flow.as_str())
        .expect_valid()
}

pub fn record_chat_session_started(
    session_interface: SessionInterface,
    agent_mode: AgentMode,
    engine: Engine,
    trust_posture: TrustPosture,
) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_chat_session_started_total", 1),
        session_interface,
        engine,
        None,
    )
    .attribute("agent_mode", agent_mode.as_str())
    .attribute("trust_posture", trust_posture.as_str())
    .expect_valid()
}

pub fn record_cloud_session_lifecycle(event: CloudSessionEvent) -> MetricRecord {
    counter("kiro_cli_cloud_session_lifecycle_total", 1)
        .attribute("version_full", version_attr())
        .attribute("cloud_event", event.as_str())
        .expect_valid()
}

pub fn record_cloud_session_ready(seconds: f64) -> Option<MetricRecord> {
    positive(seconds).then(|| {
        histogram("kiro_cli_cloud_session_ready_seconds", seconds)
            .attribute("version_full", version_attr())
            .expect_valid()
    })
}

pub fn record_autonomous_mode(event: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_autonomous_mode_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "autonomous_event",
            bounded_name(event, &["enabled", "disabled", "switch_failed", "reverted"]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_cloud_repo_attach(event: &str, repo_count_bucket: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_cloud_repo_attach_total", 1)
        .attribute("version_full", version_attr())
        .attribute("repo_attach_event", bounded_name(event, &["opened", "submitted"]))
        .attribute(
            "repo_count_bucket",
            bounded_name(repo_count_bucket, &["none", "1", "2", "3_5", "6_plus"]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_cloud_error(operation: &str, error_kind: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_cloud_error_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "cloud_op",
            bounded_name(operation, &[
                "session_new",
                "session_load",
                "turn_stream",
                "source_providers_list",
                "source_providers_resources",
                "list_sessions",
                "delete_session",
            ]),
        )
        .attribute(
            "cloud_error_kind",
            bounded_name(error_kind, &[
                "throttling",
                "auth",
                "version_skew",
                "not_found",
                "network",
                "timeout",
                "stream_truncated",
                "server_error",
                "other",
            ]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_cloud_attach(kind: &str, size_bucket: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_cloud_attach_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "attach_kind",
            bounded_name(kind, &["image", "document", "text", "binary"]),
        )
        .attribute(
            "attach_size_bucket",
            bounded_name(size_bucket, &["under_64k", "under_1m", "under_5m", "over_5m"]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_config_panel(category: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_config_panel_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "config_category",
            bounded_name(category, &[
                "menu", "agents", "mcp", "powers", "steering", "skills", "hooks", "env",
            ]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_cloud_config_diagnostic(severity: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_cloud_config_diagnostic_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "diagnostic_severity",
            bounded_name(severity, &["error", "warning", "info"]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_cloud_config_source(surface: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_cloud_config_source_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "config_surface",
            bounded_name(surface, &["mcp", "steering", "hooks", "powers"]),
        )
        .attribute("agent_engine", engine.as_str())
        .expect_valid()
}

pub fn record_ui_mode_session_started(ui_mode: UiMode) -> MetricRecord {
    counter("kiro_cli_ui_mode_session_started_total", 1)
        .attribute("version_full", version_attr())
        .attribute("ui_mode", ui_mode.as_str())
        .expect_valid()
}

/// A session-dashboard visit ended. The TUI is the production emitter; this
/// constructor is the typed contract that keeps the schema honest.
pub fn record_session_dashboard(outcome: &str, via: &str, entry: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_session_dashboard_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute(
            "outcome",
            bounded_or(outcome, &["resumed", "resumed_cross_workspace", "closed"], "_other_"),
        )
        .attribute("via", bounded_or(via, &["search", "browse", "_none_"], "_other_"))
        .attribute("entry", bounded_or(entry, &["slash", "ctrl_e", "boot"], "_other_"))
        .expect_valid()
}

pub fn record_daily_heartbeat(
    release_channel: super::ReleaseChannel,
    os_type: OsType,
    install_method: super::InstallSource,
) -> MetricRecord {
    counter("kiro_cli_daily_heartbeat", 1)
        .attribute("version_full", version_attr())
        .attribute("release_channel", release_channel.as_str())
        .attribute("os_type", os_type.as_str())
        .attribute("install_method", install_method.as_str())
        .expect_valid()
}

pub fn record_slash_command(command: &str, engine: Engine) -> MetricRecord {
    counter("kiro_cli_slash_command_invoked_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("slash_command", SlashCommandMetricName::from_name(command).as_str())
        .expect_valid()
}

pub fn record_top_level_command(command: &str) -> MetricRecord {
    counter("kiro_cli_top_level_command_invoked_total", 1)
        .attribute("version_full", version_attr())
        .attribute(
            "top_level_command",
            TopLevelCommandMetricName::from_name(command).as_str(),
        )
        .expect_valid()
}

pub fn record_tool_call(metric: ToolMetric<'_>) -> MetricRecord {
    with_tool_dimensions(counter("kiro_cli_tool_call_total", 1), metric).expect_valid()
}

pub fn record_model_invocation(engine: Engine, model: Option<&str>) -> MetricRecord {
    model_invocations_record(engine, model, 1)
}

pub fn record_model_invocations(engine: Engine, model: Option<&str>, count: u64) -> Option<MetricRecord> {
    (count > 0).then(|| model_invocations_record(engine, model, count))
}

fn model_invocations_record(engine: Engine, model: Option<&str>, count: u64) -> MetricRecord {
    counter("kiro_cli_model_invocations_total", count)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("model", model_attr(model))
        .expect_valid()
}

pub fn record_model_time_to_first_content_ms(
    milliseconds: f64,
    engine: Engine,
    model: Option<&str>,
) -> Option<MetricRecord> {
    positive(milliseconds).then(|| {
        histogram("kiro_cli_model_time_to_first_content_ms", milliseconds)
            .attribute("version_full", version_attr())
            .attribute("agent_engine", engine.as_str())
            .attribute("model", model_attr(model))
            .expect_valid()
    })
}

pub fn record_time_to_first_visible_response_ms(
    milliseconds: f64,
    session_interface: SessionInterface,
    agent_mode: AgentMode,
    engine: Engine,
) -> Option<MetricRecord> {
    positive(milliseconds).then(|| {
        with_common_product_dimensions(
            histogram("kiro_cli_time_to_first_visible_response_ms", milliseconds),
            session_interface,
            engine,
            None,
        )
        .attribute("agent_mode", agent_mode.as_str())
        .expect_valid()
    })
}

pub fn record_model_request_duration_seconds(
    seconds: f64,
    engine: Engine,
    model: Option<&str>,
    outcome: ModelRequestOutcome,
) -> Option<MetricRecord> {
    positive(seconds).then(|| {
        histogram("kiro_cli_model_request_duration_seconds", seconds)
            .attribute("version_full", version_attr())
            .attribute("agent_engine", engine.as_str())
            .attribute("model", model_attr(model))
            .attribute("model_request_outcome", outcome.as_str())
            .expect_valid()
    })
}

pub fn record_user_turn_duration_seconds(
    seconds: f64,
    session_interface: SessionInterface,
    agent_mode: AgentMode,
    engine: Engine,
) -> Option<MetricRecord> {
    positive(seconds).then(|| {
        with_common_product_dimensions(
            histogram("kiro_cli_user_turn_duration_seconds", seconds),
            session_interface,
            engine,
            None,
        )
        .attribute("agent_mode", agent_mode.as_str())
        .expect_valid()
    })
}

pub fn record_run_outcome(
    session_interface: SessionInterface,
    engine: Engine,
    os_type: OsType,
    outcome: RunOutcome,
) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_run_outcome_total", 1),
        session_interface,
        engine,
        Some(os_type),
    )
    .attribute("run_outcome", outcome.as_str())
    .expect_valid()
}

pub fn record_crash(
    engine: Engine,
    os_type: OsType,
    process_role: ProcessRole,
    crash_kind: super::CrashKind,
) -> MetricRecord {
    record_crash_for_version(version_attr(), engine, os_type, process_role, crash_kind)
}

pub fn record_crash_for_version(
    version_full: &str,
    engine: Engine,
    os_type: OsType,
    process_role: ProcessRole,
    crash_kind: super::CrashKind,
) -> MetricRecord {
    counter("kiro_cli_crash_total", 1)
        .attribute("version_full", version_full)
        .attribute("agent_engine", engine.as_str())
        .attribute("os_type", os_type.as_str())
        .attribute("process_role", process_role.as_str())
        .attribute("crash_kind", crash_kind.as_str())
        .expect_valid()
}

pub fn record_startup_duration_seconds(
    seconds: f64,
    session_interface: SessionInterface,
    engine: Engine,
    os_type: OsType,
) -> Option<MetricRecord> {
    positive(seconds).then(|| {
        with_common_product_dimensions(
            histogram("kiro_cli_startup_duration_seconds", seconds),
            session_interface,
            engine,
            Some(os_type),
        )
        .expect_valid()
    })
}

pub fn record_model_request_failure(engine: Engine, model: Option<&str>, error_kind: ErrorKind) -> MetricRecord {
    counter("kiro_cli_model_request_failure_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("model", model_attr(model))
        .attribute("error_kind", error_kind.as_str())
        .expect_valid()
}

pub fn record_stream_stall(
    session_interface: SessionInterface,
    acp_client: AcpClient,
    engine: Engine,
    model: Option<&str>,
    tier: StallTier,
) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_stream_stall_total", 1),
        session_interface,
        engine,
        None,
    )
    .attribute("model", model_attr(model))
    .attribute("stall_tier", tier.as_str())
    .attribute("acp_client", acp_client.as_str())
    .expect_valid()
}

pub fn record_stream_stall_idle_seconds(
    idle_seconds: f64,
    session_interface: SessionInterface,
    acp_client: AcpClient,
    engine: Engine,
    model: Option<&str>,
    episode_end: StallEpisodeEnd,
) -> MetricRecord {
    histogram("kiro_cli_stream_stall_idle_seconds", idle_seconds)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("model", model_attr(model))
        .attribute("stall_episode_end", episode_end.as_str())
        .attribute("session_interface", session_interface.as_str())
        .attribute("acp_client", acp_client.as_str())
        .expect_valid()
}

pub fn record_stream_stall_retry(
    session_interface: SessionInterface,
    acp_client: AcpClient,
    engine: Engine,
    model: Option<&str>,
    outcome: RetryOutcome,
    attempt_bucket: AttemptNumberBucket,
    partial_output: PartialOutput,
) -> MetricRecord {
    counter("kiro_cli_stream_stall_retry_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("model", model_attr(model))
        .attribute("retry_outcome", outcome.as_str())
        .attribute("attempt_number_bucket", attempt_bucket.as_str())
        .attribute("partial_output", partial_output.as_str())
        .attribute("session_interface", session_interface.as_str())
        .attribute("acp_client", acp_client.as_str())
        .expect_valid()
}

pub fn record_stream_stall_recovery_seconds(
    recovery_seconds: f64,
    session_interface: SessionInterface,
    acp_client: AcpClient,
    engine: Engine,
    model: Option<&str>,
) -> MetricRecord {
    histogram("kiro_cli_stream_stall_recovery_seconds", recovery_seconds)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("model", model_attr(model))
        .attribute("session_interface", session_interface.as_str())
        .attribute("acp_client", acp_client.as_str())
        .expect_valid()
}

pub fn record_subagent_deadline_expired(deadline_seconds: f64, engine: Engine) -> MetricRecord {
    counter("kiro_cli_subagent_deadline_expired_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("deadline_seconds_bucket", deadline_bucket(deadline_seconds))
        .expect_valid()
}

/// An agent-layer transient-failure retry (throttle/5xx/network) was issued after a
/// backoff wait. Carries the same surface dimensions as the stall family so retry
/// rate can be split by surface and joined against those series.
pub fn record_transient_retry(
    session_interface: SessionInterface,
    acp_client: AcpClient,
    engine: Engine,
    model: Option<&str>,
    class: TransientErrorClass,
    attempt_bucket: AttemptNumberBucket,
    partial_output: PartialOutput,
) -> MetricRecord {
    counter("kiro_cli_transient_retry_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("model", model_attr(model))
        .attribute("transient_class", class.as_str())
        .attribute("attempt_number_bucket", attempt_bucket.as_str())
        .attribute("partial_output", partial_output.as_str())
        .attribute("session_interface", session_interface.as_str())
        .attribute("acp_client", acp_client.as_str())
        .expect_valid()
}

/// Coarse dimension-safe bucket for the configured deadline, so the counter can
/// distinguish default from user-tuned deadlines without unbounded cardinality.
fn deadline_bucket(deadline_seconds: f64) -> &'static str {
    if deadline_seconds <= 0.0 {
        "unknown"
    } else if deadline_seconds < 3600.0 {
        "under_default"
    } else if deadline_seconds == 3600.0 {
        "default"
    } else {
        "over_default"
    }
}

fn process_dimensions(
    builder: super::MetricBuilder,
    os_type: Option<OsType>,
    engine: Engine,
    process_role: ProcessRole,
) -> super::MetricBuilder {
    let builder = builder
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("process_role", process_role.as_str());
    match os_type {
        Some(os_type) => builder.attribute("os_type", os_type.as_str()),
        None => builder,
    }
}

pub fn record_process_memory_rss_bytes(
    bytes: f64,
    os_type: OsType,
    engine: Engine,
    process_role: ProcessRole,
) -> Option<MetricRecord> {
    non_negative(bytes).then(|| {
        process_dimensions(
            gauge("kiro_cli_process_memory_rss_bytes", bytes),
            Some(os_type),
            engine,
            process_role,
        )
        .expect_valid()
    })
}

pub fn record_process_cpu_utilization_ratio(
    ratio: f64,
    os_type: OsType,
    engine: Engine,
    process_role: ProcessRole,
) -> Option<MetricRecord> {
    non_negative(ratio).then(|| {
        process_dimensions(
            histogram("kiro_cli_process_cpu_utilization_ratio", ratio),
            Some(os_type),
            engine,
            process_role,
        )
        .expect_valid()
    })
}

pub fn record_process_open_file_descriptor_count(
    count: f64,
    os_type: OsType,
    engine: Engine,
    process_role: ProcessRole,
) -> Option<MetricRecord> {
    non_negative(count).then(|| {
        process_dimensions(
            gauge("kiro_cli_process_open_file_descriptor_count", count),
            Some(os_type),
            engine,
            process_role,
        )
        .expect_valid()
    })
}

pub fn record_process_handle_count(count: f64, engine: Engine, process_role: ProcessRole) -> Option<MetricRecord> {
    non_negative(count).then(|| {
        process_dimensions(
            gauge("kiro_cli_process_handle_count", count),
            None,
            engine,
            process_role,
        )
        .expect_valid()
    })
}

pub fn record_process_thread_count(
    count: f64,
    os_type: OsType,
    engine: Engine,
    process_role: ProcessRole,
) -> Option<MetricRecord> {
    non_negative(count).then(|| {
        process_dimensions(
            gauge("kiro_cli_process_thread_count", count),
            Some(os_type),
            engine,
            process_role,
        )
        .expect_valid()
    })
}

pub fn record_tokens_consumed(
    value: u64,
    engine: Engine,
    model: Option<&str>,
    token_type: super::TokenType,
) -> Option<MetricRecord> {
    (value > 0 && !matches!(token_type, super::TokenType::InputCacheWrite | super::TokenType::Other)).then(|| {
        counter("kiro_cli_tokens_consumed", value)
            .attribute("version_full", version_attr())
            .attribute("agent_engine", engine.as_str())
            .attribute("model", model_attr(model))
            .attribute("token_type", token_type.as_str())
            .expect_valid()
    })
}

pub fn record_credits_consumed(value: f64, model: Option<&str>) -> Option<MetricRecord> {
    positive(value).then(|| {
        counter_f64("kiro_cli_credits_consumed", value)
            .attribute("version_full", version_attr())
            .attribute("model", model_attr(model))
            .expect_valid()
    })
}

pub fn record_tool_execution_duration_ms(milliseconds: f64, metric: ToolMetric<'_>) -> Option<MetricRecord> {
    positive(milliseconds).then(|| {
        with_tool_dimensions(histogram("kiro_cli_tool_execution_duration_ms", milliseconds), metric).expect_valid()
    })
}

pub fn record_mcp_server_init(
    engine: Engine,
    source: McpServerSource,
    outcome: McpInitOutcome,
    server_name: Option<&str>,
) -> MetricRecord {
    counter("kiro_cli_mcp_server_init_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("mcp_server_source", source.as_str())
        .attribute("mcp_init_outcome", outcome.as_str())
        .optional_attribute("mcp_server_name", server_name.filter(|name| !name.trim().is_empty()))
        .expect_valid()
}

pub fn record_mcp_tools_token_count_estimate(
    estimate: u64,
    engine: Engine,
    source: McpServerSource,
    server_name: Option<&str>,
) -> MetricRecord {
    histogram("kiro_cli_mcp_tools_token_count_estimate", estimate as f64)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("mcp_server_source", source.as_str())
        .optional_attribute("mcp_server_name", server_name.filter(|name| !name.trim().is_empty()))
        .expect_valid()
}

pub fn record_user_turn(session_interface: SessionInterface, agent_mode: AgentMode, engine: Engine) -> MetricRecord {
    record_user_turn_for_acp_client(session_interface, agent_mode, engine, None)
}

pub fn record_acp_method_invocation(
    session_interface: SessionInterface,
    engine: Engine,
    method: &str,
    acp_client_name: Option<&str>,
) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_acp_method_invocations_total", 1),
        session_interface,
        engine,
        None,
    )
    .attribute("acp_method", method)
    .optional_attribute("acp_client_name", acp_client_name)
    .expect_valid()
}

/// Terminal outcome of one inbound ACP extension request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpMethodOutcome {
    /// The handler produced a JSON-RPC result.
    Success,
    /// Rejected as client-attributable because the request parameters were invalid.
    Error,
    /// Failed with a server-attributable (internal) error.
    Fault,
    #[default]
    Unknown,
}

impl AcpMethodOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Fault => "fault",
            Self::Unknown => "unknown",
        }
    }
}

/// Shared product + ACP dimensions for the outcome counter and duration
/// histogram, so both series carry an identical dimension set.
fn with_acp_method_dimensions(
    builder: super::MetricBuilder,
    session_interface: SessionInterface,
    engine: Engine,
    method: &str,
    acp_client_name: Option<&str>,
    outcome: AcpMethodOutcome,
) -> super::MetricBuilder {
    with_common_product_dimensions(builder, session_interface, engine, None)
        .attribute("acp_method", method)
        .optional_attribute("acp_client_name", acp_client_name)
        .attribute("acp_method_outcome", outcome.as_str())
}

/// Terminal outcome of one inbound ACP extension request. A single counter partitioned by
/// `acp_method_outcome` gives a bounded
/// success/error/fault/unknown breakdown.
pub fn record_acp_method_outcome(
    session_interface: SessionInterface,
    engine: Engine,
    method: &str,
    acp_client_name: Option<&str>,
    outcome: AcpMethodOutcome,
) -> MetricRecord {
    with_acp_method_dimensions(
        counter("kiro_cli_acp_method_outcome_total", 1),
        session_interface,
        engine,
        method,
        acp_client_name,
        outcome,
    )
    .expect_valid()
}

/// End-to-end latency of one completed inbound ACP extension request, split by
/// terminal outcome. Non-finite or non-positive samples are dropped (the
/// outcome counter still records them).
pub fn record_acp_method_duration_ms(
    milliseconds: f64,
    session_interface: SessionInterface,
    engine: Engine,
    method: &str,
    acp_client_name: Option<&str>,
    outcome: AcpMethodOutcome,
) -> Option<MetricRecord> {
    positive(milliseconds).then(|| {
        with_acp_method_dimensions(
            histogram("kiro_cli_acp_method_duration_ms", milliseconds),
            session_interface,
            engine,
            method,
            acp_client_name,
            outcome,
        )
        .expect_valid()
    })
}

pub fn record_user_turn_for_acp_client(
    session_interface: SessionInterface,
    agent_mode: AgentMode,
    engine: Engine,
    acp_client_name: Option<&str>,
) -> MetricRecord {
    with_common_product_dimensions(counter("kiro_cli_user_turns", 1), session_interface, engine, None)
        .attribute("agent_mode", agent_mode.as_str())
        .optional_attribute("acp_client_name", acp_client_name)
        .expect_valid()
}

pub fn record_goal_outcome(engine: Engine, outcome: GoalOutcome) -> MetricRecord {
    counter("kiro_cli_goal_outcome_total", 1)
        .attribute("version_full", version_attr())
        .attribute("agent_engine", engine.as_str())
        .attribute("goal_outcome", outcome.as_str())
        .expect_valid()
}

pub fn record_telemetry_export_dropped(count: u64, drop_reason: ExportDropReason) -> Option<MetricRecord> {
    record_telemetry_export_dropped_for_version(count, env!("CARGO_PKG_VERSION"), drop_reason)
}

pub(crate) fn record_telemetry_export_dropped_for_version(
    count: u64,
    version_full: &str,
    drop_reason: ExportDropReason,
) -> Option<MetricRecord> {
    (count > 0).then(|| {
        counter("kiro_cli_telemetry_export_dropped_total", count)
            .attribute("version_full", version_full)
            .attribute("drop_reason", drop_reason.as_str())
            .expect_valid()
    })
}

pub fn record_turn_failure(
    session_interface: SessionInterface,
    agent_mode: AgentMode,
    engine: Engine,
    failure_reason: TurnFailureReason,
) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_turn_failure_total", 1),
        session_interface,
        engine,
        None,
    )
    .attribute("agent_mode", agent_mode.as_str())
    .attribute("turn_failure_reason", failure_reason.as_str())
    .expect_valid()
}

pub fn record_turn_cancelled(
    session_interface: SessionInterface,
    agent_mode: AgentMode,
    engine: Engine,
) -> MetricRecord {
    with_common_product_dimensions(
        counter("kiro_cli_turn_cancelled_total", 1),
        session_interface,
        engine,
        None,
    )
    .attribute("agent_mode", agent_mode.as_str())
    .expect_valid()
}

pub fn record_process_peak_rss_bytes(
    bytes: f64,
    os_type: OsType,
    engine: Engine,
    process_role: ProcessRole,
) -> Option<MetricRecord> {
    non_negative(bytes).then(|| {
        process_dimensions(
            histogram("kiro_cli_process_peak_rss_bytes", bytes),
            Some(os_type),
            engine,
            process_role,
        )
        .expect_valid()
    })
}

fn tui_dimensions(builder: super::MetricBuilder, os_type: OsType, engine: Engine) -> super::MetricBuilder {
    builder
        .attribute("version_full", version_attr())
        .attribute("os_type", os_type.as_str())
        .attribute("agent_engine", engine.as_str())
}

pub fn record_tui_heap_used_bytes(bytes: f64, os_type: OsType, engine: Engine) -> Option<MetricRecord> {
    non_negative(bytes)
        .then(|| tui_dimensions(gauge("kiro_cli_tui_heap_used_bytes", bytes), os_type, engine).expect_valid())
}

pub fn record_tui_event_loop_delay_p99_seconds(seconds: f64, os_type: OsType, engine: Engine) -> Option<MetricRecord> {
    non_negative(seconds).then(|| {
        tui_dimensions(
            histogram("kiro_cli_tui_event_loop_delay_p99_seconds", seconds),
            os_type,
            engine,
        )
        .expect_valid()
    })
}

pub fn record_tui_input_to_render_p95_seconds(seconds: f64, os_type: OsType, engine: Engine) -> Option<MetricRecord> {
    non_negative(seconds).then(|| {
        tui_dimensions(
            histogram("kiro_cli_tui_input_to_render_p95_seconds", seconds),
            os_type,
            engine,
        )
        .expect_valid()
    })
}

pub fn record_tui_render_duration_seconds(
    seconds: f64,
    os_type: OsType,
    engine: Engine,
    render_kind: RenderKind,
) -> Option<MetricRecord> {
    non_negative(seconds).then(|| {
        tui_dimensions(
            histogram("kiro_cli_tui_render_duration_seconds", seconds),
            os_type,
            engine,
        )
        .attribute("render_kind", render_kind.as_str())
        .expect_valid()
    })
}

pub fn record_auth_failure(
    auth_method: AuthMethod,
    auth_flow: AuthFlow,
    auth_operation: super::AuthOperation,
    failure_reason: AuthFailureReason,
) -> MetricRecord {
    counter("kiro_cli_auth_failure_total", 1)
        .attribute("version_full", version_attr())
        .attribute("auth_method", auth_method.as_str())
        .attribute("auth_flow", auth_flow.as_str())
        .attribute("auth_operation", auth_operation.as_str())
        .attribute("auth_failure_reason", failure_reason.as_str())
        .expect_valid()
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn bounded_name<'a>(value: &'a str, allowed: &[&str]) -> &'a str {
    bounded_or(value, allowed, "unknown")
}

fn bounded_or<'a>(value: &'a str, allowed: &[&str], fallback: &'a str) -> &'a str {
    if allowed.contains(&value) { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::{
        AcpClient,
        AgentMode,
        ApprovalPath,
        PathScope,
        TrustPosture,
    };

    #[test]
    fn acp_client_maps_first_party_names_to_cli() {
        // The built-in TUI and the one-shot client are first-party => Cli, not Other.
        // Regression guard: mismapping these leaves the `cli` bucket dead in V2.
        assert_eq!(AcpClient::from_client_name(Some("kiro-tui")), AcpClient::Cli);
        assert_eq!(
            AcpClient::from_client_name(Some("kiro-cli-non-interactive")),
            AcpClient::Cli
        );
        assert_eq!(AcpClient::from_client_name(None), AcpClient::Cli);
    }

    #[test]
    fn acp_client_maps_crew_and_other() {
        assert_eq!(AcpClient::from_client_name(Some("kirocrew")), AcpClient::Kirocrew);
        assert_eq!(AcpClient::from_client_name(Some("some-external-ide")), AcpClient::Other);
        assert_eq!(AcpClient::from_client_name(Some("")), AcpClient::Unknown);
    }

    #[test]
    fn built_in_default_agent_uses_default_bucket() {
        assert_eq!(AgentMode::from_id(Some("kiro_default")), AgentMode::Default);
    }

    fn assert_schema_values(attribute_name: &str, actual: &[&str]) {
        let attribute = kiro_telemetry_schema::registry()
            .attribute(attribute_name)
            .unwrap_or_else(|| panic!("missing {attribute_name} in types.yaml"));
        assert_eq!(attribute.allowed_values.len(), actual.len());
        for value in actual {
            assert!(
                attribute.accepts_value(value),
                "{value:?} is missing from {attribute_name} in types.yaml"
            );
        }
    }

    #[test]
    fn every_variant_maps_to_a_declared_schema_value() {
        assert_schema_values("approval_path", &[
            ApprovalPath::AutoAllowed.as_str(),
            ApprovalPath::UserApproved.as_str(),
            ApprovalPath::Denied.as_str(),
            ApprovalPath::Unknown.as_str(),
        ]);
    }

    #[test]
    fn trust_all_tools_is_reported_as_such() {
        assert_eq!(TrustPosture::from_trust_all_tools(true), TrustPosture::TrustAllTools);
        assert_eq!(TrustPosture::from_trust_all_tools(false), TrustPosture::PromptOnDemand);
    }

    #[test]
    fn every_trust_posture_variant_maps_to_a_declared_schema_value() {
        assert_schema_values("trust_posture", &[
            TrustPosture::PromptOnDemand.as_str(),
            TrustPosture::TrustAllTools.as_str(),
            TrustPosture::Unknown.as_str(),
        ]);
    }

    #[test]
    fn every_path_scope_variant_maps_to_a_declared_schema_value() {
        assert_schema_values("path_scope", &[
            PathScope::Workspace.as_str(),
            PathScope::OutsideWorkspace.as_str(),
            PathScope::Home.as_str(),
            PathScope::System.as_str(),
            PathScope::NotApplicable.as_str(),
            PathScope::Unknown.as_str(),
        ]);
    }
}
