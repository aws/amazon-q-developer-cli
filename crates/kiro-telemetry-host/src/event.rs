//! Portable telemetry event types shared across kiro-cli harnesses.
//!
//! These types describe the *shape* of telemetry events that the various
//! kiro-cli surfaces (V2, V3, kiro-bot, ACP) emit. The translation of an
//! [`Event`] into legacy CloudWatch/Toolkit datums or OTel records lives in
//! consumer crates, so this crate does not pull in legacy clients.

use std::time::{
    Duration,
    SystemTime,
};

use kiro_telemetry::{
    LegacyEventType,
    MetricLogProperties,
    metric,
};
use serde::{
    Deserialize,
    Serialize,
};
use strum::{
    Display,
    EnumString,
};
use typeshare::typeshare;

/// A serializable telemetry event that can be sent or queued.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub created_time: Option<SystemTime>,
    pub credential_start_url: Option<String>,
    pub sso_region: Option<String>,
    pub client_application: Option<String>,
    pub app_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<metric::Engine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_interface: Option<metric::SessionInterface>,
    pub acp_client_name: Option<String>,
    pub acp_client_version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_subagent: bool,
    #[serde(skip)]
    pub metric_context: EventMetricContext,
    #[serde(flatten)]
    pub ty: EventType,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventMetricContext {
    pub mode: Option<metric::Mode>,
    pub agent_mode: Option<metric::AgentMode>,
    pub install_method: Option<metric::InstallSource>,
    pub canonical_tool_name: Option<String>,
    pub run_outcome: Option<metric::RunOutcome>,
    pub log_properties: MetricLogProperties,
}

impl Event {
    pub fn new(ty: EventType) -> Self {
        Self {
            ty,
            created_time: Some(SystemTime::now()),
            credential_start_url: None,
            sso_region: None,
            client_application: None,
            app_type: None,
            engine: None,
            session_interface: None,
            acp_client_name: None,
            acp_client_version: None,
            is_subagent: false,
            metric_context: EventMetricContext::default(),
        }
    }

    pub fn set_start_url(&mut self, start_url: String) {
        self.credential_start_url = Some(start_url);
    }

    pub fn set_sso_region(&mut self, sso_region: String) {
        self.sso_region = Some(sso_region);
    }

    pub fn set_client_application(&mut self, client_application: String) {
        self.client_application = Some(client_application);
    }

    pub fn set_client_application_kind(&mut self, client_application: metric::ClientApplication) {
        self.client_application = Some(client_application.as_str().to_string());
    }

    pub fn set_engine(&mut self, engine: metric::Engine) {
        self.engine = Some(engine);
    }

    pub fn set_session_interface(&mut self, session_interface: metric::SessionInterface) {
        self.session_interface = Some(session_interface);
    }

    /// Correlation fields retained as queryable KUTS EMF log properties.
    /// They are deliberately separate from the registered metric attributes.
    pub fn metric_log_properties(&self) -> MetricLogProperties {
        let properties = MetricLogProperties::default();
        let mut properties = match &self.ty {
            EventType::RefreshCredentials { request_id, .. } => properties.with_request_id(request_id.clone()),
            EventType::ChatAddedMessage {
                conversation_id, data, ..
            } => properties
                .with_session_id(conversation_id.clone())
                .with_request_id(data.request_id.clone()),
            EventType::MessageResponseError {
                conversation_id,
                request_id,
                ..
            } => properties
                .with_session_id(conversation_id.clone())
                .with_request_id(request_id.clone()),
            EventType::ChatSlashCommandExecuted { conversation_id, .. }
            | EventType::ChatStart { conversation_id, .. }
            | EventType::ChatEnd { conversation_id, .. }
            | EventType::TangentModeSession { conversation_id, .. }
            | EventType::ToolUseSuggested { conversation_id, .. }
            | EventType::AgentContribution { conversation_id, .. }
            | EventType::McpServerInit { conversation_id, .. }
            | EventType::AgentConfigInit { conversation_id, .. } => properties.with_session_id(conversation_id.clone()),
            EventType::RecordUserTurnCompletion {
                conversation_id, args, ..
            } => properties
                .with_session_id(conversation_id.clone())
                .with_request_id(args.request_ids.iter().rev().find_map(Clone::clone)),
            EventType::SubagentInvocation {
                parent_conversation_id, ..
            } => properties.with_session_id(parent_conversation_id.clone()),
            EventType::VoiceInput { conversation_id, .. } | EventType::GoalCompleted { conversation_id, .. } => {
                properties.with_session_id(conversation_id.clone())
            },
            EventType::ProcessHealthMetric { session_id, .. }
            | EventType::ModeChanged { session_id, .. }
            | EventType::UiModeSessionStart { session_id, .. }
            | EventType::UiModeChanged { session_id, .. }
            | EventType::UiModeDefaultChanged { session_id, .. } => properties.with_session_id(session_id.clone()),
            EventType::MeteringEvent { request_id, .. } => properties.with_request_id(request_id.clone()),
            EventType::UserLoggedIn {}
            | EventType::CliSessionStarted { .. }
            | EventType::CliSessionCompleted { .. }
            | EventType::StartupDuration { .. }
            | EventType::StartupFailure { .. }
            | EventType::AuthFailed { .. }
            | EventType::CliSubcommandExecuted { .. }
            | EventType::ChatSessionStarted { .. }
            | EventType::DidSelectProfile { .. }
            | EventType::ProfileState { .. }
            | EventType::DailyHeartbeat { .. }
            | EventType::ProcessHealth { .. }
            | EventType::ContextUsagePercentage { .. }
            | EventType::ModelInvocation { .. }
            | EventType::EmptyResponseRetry { .. }
            | EventType::AutomaticRetryCompleted { .. } => properties,
        };

        if let Some(session_id) = self.metric_context.log_properties.session_id() {
            properties = properties.with_session_id(session_id.to_string());
        }
        if let Some(request_id) = self.metric_context.log_properties.request_id() {
            properties = properties.with_request_id(request_id.to_string());
        }
        properties
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, Serialize, Deserialize)]
pub enum ChatConversationType {
    // Names are as requested by science
    NotToolUse,
    ToolUse,
}

/// A metadata tag that can be used to annotate a request.
#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, Serialize, Deserialize)]
pub enum MessageMetaTag {
    /// A /compact request
    Compact,
    GenerateAgent,
    /// A /tangent request
    TangentMode,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmptyResponseRetryOutcome {
    Recovered,
    StillEmpty,
}

/// How a mode change was initiated. Add a new variant when adding a new entry point —
/// the wire format is the camelCase variant name. Keeping this as an enum (rather than a
/// free-form string) gives us spell-check at the call site and a single documented set of
/// known values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
#[typeshare]
pub enum ModeChangeSource {
    /// User pressed Shift+Tab to toggle in/out of `kiro_planner`.
    ShiftTab,
    /// User invoked a slash command (`/agent`, `/plan`).
    SlashCommand,
    /// User changed the active UI from the display settings panel.
    SettingsPanel,
}

/// Which input source resolved the UI mode at session start. The wire format is the
/// camelCase variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
#[typeshare]
pub enum UiModeSource {
    /// Resolved from the `KIRO_UI_MODE` env var.
    EnvVar,
    /// Resolved from the persisted `chat.ui.mode` setting.
    Setting,
    /// No env / setting — fell through to the built-in default.
    Default,
}

/// Optional fields to add for a chatAddedMessage telemetry event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChatAddedMessageParams {
    pub message_id: Option<String>,
    pub request_id: Option<String>,
    pub context_file_length: Option<usize>,
    pub reason: Option<String>,
    pub reason_desc: Option<String>,
    pub status_code: Option<u16>,
    pub model: Option<String>,
    pub time_to_first_chunk_ms: Option<f64>,
    pub request_duration_seconds: Option<f64>,
    pub time_between_chunks_ms: Option<Vec<f64>>,
    pub chat_conversation_type: Option<ChatConversationType>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub assistant_response_length: Option<i32>,
    pub message_meta_tags: Vec<MessageMetaTag>,
    pub total_tokens: Option<i32>,
    pub uncached_input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub cache_read_input_tokens: Option<i32>,
    pub cache_write_input_tokens: Option<i32>,
}

/// Optional fields for tangent mode session telemetry event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TangentModeSessionArgs {
    /// Duration of tangent mode session in seconds
    pub duration_seconds: i64,
    /// Whether this is a forget command (true) or tangent mode session (false)
    #[serde(default)]
    pub is_forget: bool,
    /// Number of conversation entries removed (only for forget command)
    #[serde(default)]
    pub entries_removed: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RecordUserTurnCompletionArgs {
    pub request_ids: Vec<Option<String>>,
    pub message_ids: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub reason: Option<String>,
    pub reason_desc: Option<String>,
    pub status_code: Option<u16>,
    pub time_to_first_chunks_ms: Vec<Option<f64>>,
    pub chat_conversation_type: Option<ChatConversationType>,
    pub user_prompt_length: i64,
    pub assistant_response_length: i64,
    #[serde(default)]
    pub total_tokens: Option<i64>,
    #[serde(default)]
    pub uncached_input_tokens: Option<i64>,
    #[serde(default)]
    pub output_tokens: Option<i64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(default)]
    pub cache_write_input_tokens: Option<i64>,
    #[serde(default)]
    pub model_invocation_count: u64,
    pub user_turn_duration_seconds: i64,
    pub follow_up_count: i64,
    pub message_meta_tags: Vec<MessageMetaTag>,
    pub is_subagent: bool,
    #[serde(default)]
    pub emit_user_turn_counter: bool,
    #[serde(skip)]
    pub emit_turn_numeric_metrics: Option<bool>,
    pub parent_tool_use_id: Option<String>,
    /// Number of HTTP-level attempts for the last request in the turn. `None` if not reported
    /// by the transport (mock clients) or if the turn didn't make any transport-level requests.
    /// Pairs with `reason`/`reason_desc` when the turn failed.
    #[serde(default)]
    pub request_attempts: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AgentConfigInitArgs {
    pub agents_loaded_count: i64,
    pub agents_loaded_failed_count: i64,
    pub legacy_profile_migration_executed: bool,
    pub legacy_profile_migrated_count: i64,
    pub launched_agent: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
pub enum EventType {
    UserLoggedIn {},
    CliSessionStarted {
        os_type: metric::OsType,
        install_source: metric::InstallSource,
    },
    CliSessionCompleted {
        exit_reason: metric::ExitReason,
        agent_kind: metric::AgentKind,
    },
    StartupDuration {
        duration_seconds: f64,
        os_type: metric::OsType,
    },
    StartupFailure {
        os_type: metric::OsType,
        failure_stage: metric::StartupFailureStage,
    },
    AuthFailed {
        auth_method: String,
        oauth_flow: String,
        error_type: String,
        error_code: Option<String>,
    },
    RefreshCredentials {
        request_id: String,
        result: TelemetryResult,
        reason: Option<String>,
        oauth_flow: String,
    },
    CliSubcommandExecuted {
        subcommand: String,
    },
    ChatSlashCommandExecuted {
        conversation_id: String,
        command: String,
        subcommand: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
    },
    ChatStart {
        conversation_id: String,
        model: Option<String>,
    },
    ChatSessionStarted {
        #[serde(default, deserialize_with = "deserialize_mode_or_default")]
        mode: metric::Mode,
    },
    ChatEnd {
        conversation_id: String,
        model: Option<String>,
    },
    ChatAddedMessage {
        conversation_id: String,
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    },
    RecordUserTurnCompletion {
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    },
    TangentModeSession {
        conversation_id: String,
        result: TelemetryResult,
        args: TangentModeSessionArgs,
    },
    ToolUseSuggested {
        conversation_id: String,
        utterance_id: Option<String>,
        user_input_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp_server_name: Option<String>,
        is_accepted: bool,
        is_trusted: bool,
        is_success: Option<bool>,
        reason_desc: Option<String>,
        is_valid: Option<bool>,
        is_custom_tool: bool,
        input_token_size: Option<usize>,
        output_token_size: Option<usize>,
        custom_tool_call_latency: Option<usize>,
        model: Option<String>,
        execution_duration: Option<Duration>,
        turn_duration: Option<Duration>,
        aws_service_name: Option<String>,
        aws_operation_name: Option<String>,
    },
    AgentContribution {
        conversation_id: String,
        utterance_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        lines_by_agent: Option<isize>,
        lines_by_user: Option<isize>,
    },
    McpServerInit {
        conversation_id: String,
        server_name: String,
        #[serde(default)]
        mcp_server_source: metric::McpServerSource,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
    },
    AgentConfigInit {
        conversation_id: String,
        args: AgentConfigInitArgs,
    },
    DidSelectProfile {
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
        profile_count: Option<i64>,
    },
    ProfileState {
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
    },
    MessageResponseError {
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        conversation_id: String,
        request_id: Option<String>,
        message_id: Option<String>,
        context_file_length: Option<usize>,
        #[serde(default)]
        model: Option<String>,
    },
    DailyHeartbeat {
        #[serde(default)]
        install_method: Option<String>,
    },
    SubagentInvocation {
        parent_conversation_id: String,
        subagent_name: String,
        builtin_tool_uses: u32,
        mcp_tool_uses: u32,
        parent_tool_use_id: String,
    },
    VoiceInput {
        conversation_id: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        backend: String,
        input_method: String,
        recording_duration_ms: Option<i64>,
        transcription_duration_ms: Option<i64>,
        text_length: Option<i64>,
        model_size: Option<String>,
        auto_submit: Option<bool>,
    },
    ProcessHealthMetric {
        #[serde(
            default,
            deserialize_with = "deserialize_agent_kind_or_default",
            skip_serializing_if = "is_default_agent_kind"
        )]
        agent_kind: metric::AgentKind,
        rss_mb: f64,
        heap_used_mb: f64,
        peak_rss_mb: f64,
        cpu_user_pct: f64,
        cpu_system_pct: f64,
        last_render_ms: f64,
        max_render_ms: f64,
        renders_per_min: i64,
        full_redraws_per_min: i64,
        yoga_node_count: i64,
        event_loop_p99_ms: Option<f64>,
        input_latency_p95_ms: Option<f64>,
        session_duration_sec: i64,
        cpu_cores: i64,
        total_memory_mb: i64,
        terminal: String,
        session_id: Option<String>,
        version: String,
        platform: String,
    },
    ProcessHealth {
        rss_bytes: f64,
        peak_rss_bytes: f64,
        cpu_utilization: f64,
    },
    /// Emitted when the active agent (= ACP session mode) changes. Caller is responsible
    /// for skipping no-op changes (`from_mode == to_mode`). `session_id` carries the ACP
    /// session id that becomes `amazonqConversationId` on the metric.
    ModeChanged {
        from_mode: String,
        to_mode: String,
        source: ModeChangeSource,
        session_id: Option<String>,
    },
    /// Emitted exactly once per session, immediately after the TUI resolves the UI mode.
    /// `ui_mode_default` is the persisted default (or `"unset"`) regardless of which
    /// source actually won — separating the steady-state count from the per-launch outcome.
    UiModeSessionStart {
        ui_mode: String,
        ui_mode_source: UiModeSource,
        ui_mode_default: String,
        session_id: Option<String>,
    },
    /// Emitted when the user toggles between `lite` and `tui` mid-session via `/lite` or
    /// `/tui`. Caller is responsible for skipping no-op changes (`from == to`).
    UiModeChanged {
        from: String,
        to: String,
        source: ModeChangeSource,
        session_id: Option<String>,
    },
    /// Emitted when `/settings default-ui` writes a new value to the persisted
    /// `chat.ui.mode` setting. Caller is responsible for skipping no-ops.
    UiModeDefaultChanged {
        from: String,
        to: String,
        session_id: Option<String>,
    },
    GoalCompleted {
        conversation_id: Option<String>,
        terminal_state: String,
        iterations: i64,
        max_iterations: i64,
        duration_sec: i64,
    },
    MeteringEvent {
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
    },
    ContextUsagePercentage {
        model: Option<String>,
        percentage: f64,
    },
    ModelInvocation {
        model: Option<String>,
    },
    EmptyResponseRetry {
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    },
    AutomaticRetryCompleted {
        retry_reason: metric::RetryReason,
        additional_attempts: u32,
        outcome: metric::RetryOutcome,
    },
}

fn deserialize_mode_or_default<'de, D>(deserializer: D) -> Result<metric::Mode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<String> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(value.as_deref().map(metric::Mode::from_name).unwrap_or_default())
}

fn deserialize_agent_kind_or_default<'de, D>(deserializer: D) -> Result<metric::AgentKind, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<String> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(value.as_deref().map(metric::AgentKind::from_name).unwrap_or_default())
}

fn is_default_agent_kind(agent_kind: &metric::AgentKind) -> bool {
    *agent_kind == metric::AgentKind::default()
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, Serialize, Deserialize)]
pub enum TelemetryResult {
    Succeeded,
    Failed,
    Cancelled,
}

/// 'user' -> users change the profile through Q CLI user profile command
/// 'auth' -> users change the profile through dashboard
/// 'update' -> CLI auto select the profile on users' behalf as there is only 1 profile
/// 'reload' -> CLI will try to reload previous selected profile upon CLI is running
#[derive(Debug, Copy, Clone, PartialEq, Eq, EnumString, Display, Serialize, Deserialize)]
pub enum QProfileSwitchIntent {
    User,
    Auth,
    Update,
    Reload,
}

impl EventType {
    pub fn legacy_event_type(&self) -> Option<LegacyEventType> {
        match self {
            Self::UserLoggedIn {} => Some(LegacyEventType::UserLoggedIn),
            Self::CliSessionStarted { .. } => None,
            Self::CliSessionCompleted { .. } => None,
            Self::StartupDuration { .. } => None,
            Self::StartupFailure { .. } => None,
            Self::AuthFailed { .. } => Some(LegacyEventType::AuthFailed),
            Self::RefreshCredentials { .. } => Some(LegacyEventType::RefreshCredentials),
            Self::CliSubcommandExecuted { .. } => Some(LegacyEventType::CliSubcommandExecuted),
            Self::ChatSlashCommandExecuted { .. } => Some(LegacyEventType::ChatSlashCommandExecuted),
            Self::ChatStart { .. } => Some(LegacyEventType::ChatStart),
            Self::ChatEnd { .. } => Some(LegacyEventType::ChatEnd),
            Self::ChatAddedMessage { .. } => Some(LegacyEventType::ChatAddedMessage),
            Self::RecordUserTurnCompletion { .. } => Some(LegacyEventType::RecordUserTurnCompletion),
            Self::TangentModeSession { .. } => Some(LegacyEventType::TangentModeSession),
            Self::ToolUseSuggested { .. } => Some(LegacyEventType::ToolUseSuggested),
            Self::AgentContribution { .. } => Some(LegacyEventType::AgentContribution),
            Self::McpServerInit { .. } => Some(LegacyEventType::McpServerInit),
            Self::AgentConfigInit { .. } => Some(LegacyEventType::AgentConfigInit),
            Self::DidSelectProfile { .. } => Some(LegacyEventType::DidSelectProfile),
            Self::ProfileState { .. } => Some(LegacyEventType::ProfileState),
            Self::MessageResponseError { .. } => Some(LegacyEventType::MessageResponseError),
            Self::DailyHeartbeat { .. } => Some(LegacyEventType::DailyHeartbeat),
            Self::SubagentInvocation { .. } => Some(LegacyEventType::SubagentInvocation),
            Self::VoiceInput { .. } => Some(LegacyEventType::VoiceInput),
            Self::ProcessHealthMetric { .. } => Some(LegacyEventType::ProcessHealthMetric),
            Self::ProcessHealth { .. } => None,
            Self::ModeChanged { .. } => Some(LegacyEventType::ModeChanged),
            Self::UiModeSessionStart { .. } => None,
            Self::UiModeChanged { .. } => None,
            Self::UiModeDefaultChanged { .. } => None,
            Self::GoalCompleted { .. } => Some(LegacyEventType::GoalCompleted),
            Self::MeteringEvent { .. } => None,
            Self::ContextUsagePercentage { .. } => None,
            Self::EmptyResponseRetry { .. } => None,
            Self::AutomaticRetryCompleted { .. } => None,
            Self::ModelInvocation { .. } => None,
            Self::ChatSessionStarted { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips a camelCase wire-format enum through both serde JSON and
    /// strum's FromStr/Display, so a `rename_all`/variant drift fails the test.
    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str::<$ty>(&quoted).unwrap());
            assert_eq!($variant, $text.parse::<$ty>().unwrap());
            assert_eq!($text, $variant.to_string());
        };
    }

    #[test]
    fn test_ui_mode_source_ser_deser() {
        test_ser_deser!(UiModeSource, UiModeSource::EnvVar, "envVar");
        test_ser_deser!(UiModeSource, UiModeSource::Setting, "setting");
        test_ser_deser!(UiModeSource, UiModeSource::Default, "default");
    }

    #[test]
    fn test_mode_change_source_ser_deser() {
        test_ser_deser!(ModeChangeSource, ModeChangeSource::ShiftTab, "shiftTab");
        test_ser_deser!(ModeChangeSource, ModeChangeSource::SlashCommand, "slashCommand");
        test_ser_deser!(ModeChangeSource, ModeChangeSource::SettingsPanel, "settingsPanel");
    }

    #[test]
    fn chat_message_exposes_session_and_request_log_properties() {
        let event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "session-123".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                request_id: Some("request-456".to_string()),
                ..Default::default()
            },
        });

        let properties = event.metric_log_properties();
        assert_eq!(properties.session_id(), Some("session-123"));
        assert_eq!(properties.request_id(), Some("request-456"));
    }

    #[test]
    fn event_log_properties_support_session_only_request_only_and_empty_events() {
        let session = Event::new(EventType::ChatStart {
            conversation_id: "session-123".to_string(),
            model: None,
        })
        .metric_log_properties();
        assert_eq!(session.session_id(), Some("session-123"));
        assert_eq!(session.request_id(), None);

        let request = Event::new(EventType::MeteringEvent {
            request_id: Some("request-456".to_string()),
            model: None,
            usage: 1.0,
            unit: "credit".to_string(),
            unit_plural: "credits".to_string(),
        })
        .metric_log_properties();
        assert_eq!(request.session_id(), None);
        assert_eq!(request.request_id(), Some("request-456"));

        let mut v3_turn = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "v3-session-123".to_string(),
            result: TelemetryResult::Succeeded,
            args: RecordUserTurnCompletionArgs {
                request_ids: vec![None, Some("v3-request-456".to_string())],
                ..Default::default()
            },
        });
        v3_turn.set_engine(metric::Engine::V3);
        let v3_properties = v3_turn.metric_log_properties();
        assert_eq!(v3_properties.session_id(), Some("v3-session-123"));
        assert_eq!(v3_properties.request_id(), Some("v3-request-456"));

        let mut kuts_only_v3_turn = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id: "v3-session-789".to_string(),
            result: TelemetryResult::Succeeded,
            args: RecordUserTurnCompletionArgs::default(),
        });
        kuts_only_v3_turn.metric_context.log_properties = MetricLogProperties::default()
            .with_session_id("v3-session-789".to_string())
            .with_request_id("kuts-request-789".to_string());
        let kuts_only_properties = kuts_only_v3_turn.metric_log_properties();
        assert_eq!(kuts_only_properties.session_id(), Some("v3-session-789"));
        assert_eq!(kuts_only_properties.request_id(), Some("kuts-request-789"));

        let empty = Event::new(EventType::UserLoggedIn {}).metric_log_properties();
        assert_eq!(empty, MetricLogProperties::default());
    }
}
