//! Portable telemetry event types shared across kiro-cli harnesses.
//!
//! These types describe the *shape* of telemetry events that the various
//! kiro-cli surfaces (V2, V3, kiro-bot, ACP) emit. The translation of an
//! [`Event`] into legacy CloudWatch/Toolkit datums or OTel records lives in
//! the consumer crates (e.g. `chat_cli_v2`) — this crate intentionally does
//! not pull in the legacy clients.

use std::time::{
    Duration,
    SystemTime,
};

use kiro_telemetry::{
    EventClass,
    FieldClass,
    LegacyEventType,
    MetricRecord,
    PiiRedactor,
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
    pub acp_client_name: Option<String>,
    pub acp_client_version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_subagent: bool,
    #[serde(flatten)]
    pub ty: EventType,
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
            acp_client_name: None,
            acp_client_version: None,
            is_subagent: false,
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

impl From<EmptyResponseRetryOutcome> for metric::Outcome {
    fn from(value: EmptyResponseRetryOutcome) -> Self {
        match value {
            EmptyResponseRetryOutcome::Recovered => Self::Recovered,
            EmptyResponseRetryOutcome::StillEmpty => Self::StillEmpty,
        }
    }
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
    pub estimated_cost_usd: Option<f64>,
    pub user_turn_duration_seconds: i64,
    pub follow_up_count: i64,
    pub message_meta_tags: Vec<MessageMetaTag>,
    pub is_subagent: bool,
    #[serde(default)]
    pub emit_user_turn_counter: bool,
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
    /// Emitted when the active agent (= ACP session mode) changes. Caller is responsible
    /// for skipping no-op changes (`from_mode == to_mode`). `session_id` carries the ACP
    /// session id that becomes `amazonqConversationId` on the metric.
    ModeChanged {
        from_mode: String,
        to_mode: String,
        source: ModeChangeSource,
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
    RetryAttempt {
        upstream: metric::Upstream,
        retry_reason: metric::RetryReason,
        attempt: u32,
    },
    RetryExhausted {
        upstream: metric::Upstream,
        final_error_kind: metric::ErrorKind,
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

impl From<TelemetryResult> for metric::TurnOutcome {
    fn from(value: TelemetryResult) -> Self {
        match value {
            TelemetryResult::Succeeded => Self::Succeeded,
            TelemetryResult::Failed => Self::Failed,
            TelemetryResult::Cancelled => Self::Cancelled,
        }
    }
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
            Self::ModeChanged { .. } => Some(LegacyEventType::ModeChanged),
            Self::GoalCompleted { .. } => Some(LegacyEventType::GoalCompleted),
            Self::MeteringEvent { .. } => None,
            Self::ContextUsagePercentage { .. } => None,
            Self::EmptyResponseRetry { .. } => None,
            Self::RetryAttempt { .. } => None,
            Self::RetryExhausted { .. } => None,
            Self::ModelInvocation { .. } => None,
            Self::ChatSessionStarted { .. } => None,
        }
    }

    pub(crate) fn redaction_metric_records(&self, channel: metric::TelemetryChannel) -> Vec<MetricRecord> {
        match self {
            Self::ChatAddedMessage {
                data: ChatAddedMessageParams { reason_desc, .. },
                ..
            }
            | Self::RecordUserTurnCompletion {
                args: RecordUserTurnCompletionArgs { reason_desc, .. },
                ..
            }
            | Self::ToolUseSuggested { reason_desc, .. }
            | Self::MessageResponseError { reason_desc, .. }
            | Self::VoiceInput { reason_desc, .. } => {
                redaction_records_for_fields(channel, &[(FieldClass::Other, reason_desc.as_deref())])
            },
            Self::McpServerInit {
                init_failure_reason,
                all_tool_names,
                loaded_tool_names,
                ..
            } => redaction_records_for_fields(channel, &[
                (FieldClass::Other, init_failure_reason.as_deref()),
                (FieldClass::Context, all_tool_names.as_deref()),
                (FieldClass::Context, loaded_tool_names.as_deref()),
            ]),
            Self::AuthFailed { error_type, .. } => {
                redaction_records_for_fields(channel, &[(FieldClass::Other, Some(error_type.as_str()))])
            },
            _ => Vec::new(),
        }
    }
}

impl Event {
    pub fn redaction_metric_records(&self, channel: metric::TelemetryChannel) -> Vec<MetricRecord> {
        self.ty.redaction_metric_records(channel)
    }
}

fn redaction_records_for_fields(
    channel: metric::TelemetryChannel,
    fields: &[(FieldClass, Option<&str>)],
) -> Vec<MetricRecord> {
    let mut records = Vec::new();
    for (field_class, value) in fields {
        let Some(value) = value else {
            continue;
        };
        records.extend(
            PiiRedactor
                .redact(*field_class, value)
                .metric_records(EventClass::LegacyEvent, channel),
        );
    }
    records
}
