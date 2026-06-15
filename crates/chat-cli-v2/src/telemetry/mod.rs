pub mod cognito;
pub mod core;
pub mod endpoint;
pub mod observer;

use core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    ToolUseEventBuilder,
};
use std::str::FromStr;
use std::sync::Arc;

use amzn_codewhisperer_client::types::{
    ChatAddMessageEvent,
    ChatInteractWithMessageEvent,
    ChatMessageInteractionType,
    IdeCategory,
    OperatingSystem,
    TelemetryEvent,
    UserContext,
};
use amzn_toolkit_telemetry_client::config::{
    BehaviorVersion,
    Region,
};
use amzn_toolkit_telemetry_client::error::DisplayErrorContext;
use amzn_toolkit_telemetry_client::types::AwsProduct;
use amzn_toolkit_telemetry_client::{
    Client as ToolkitTelemetryClient,
    Config,
};
use aws_credential_types::provider::SharedCredentialsProvider;
use cognito::CognitoProvider;
use endpoint::StaticEndpoint;
use kiro_telemetry::{
    OtelLogsSink,
    OtelMetricsSink,
    OtelMode,
    OtelProviders,
    TelemetryClient as OtelTelemetryClient,
    TelemetryConfig as OtelTelemetryConfig,
    consent_file_integrity_records,
    init_otel,
    metric,
};
pub use kiro_telemetry_host::{
    InstallMethod,
    get_install_method,
};
#[cfg(test)]
use kiro_telemetry_legacy::event_to_otel_metric_record;
use kiro_telemetry_legacy::{
    event_to_metric_datum,
    event_to_otel_log_record,
    event_to_otel_metric_records,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::error::Elapsed;
use tracing::{
    debug,
    error,
    trace,
};
use uuid::{
    Uuid,
    uuid,
};

use crate::api_client::{
    ApiClient,
    ApiClientError,
};
use crate::auth::builder_id::get_start_url_and_region;
use crate::aws_common::app_name;
use crate::cli::RootSubcommand;
use crate::constants::{
    BREW_CASK_NAME,
    CLI_NAME,
};
use crate::database::settings::Setting;
use crate::database::{
    Database,
    DatabaseError,
};
use crate::os::{
    Env,
    Fs,
};
use crate::telemetry::core::Event;
pub use crate::telemetry::core::{
    EmptyResponseRetryOutcome,
    EventType,
    QProfileSwitchIntent,
    TelemetryResult,
};
use crate::util::consts::env_var::{
    KIRO_TELEMETRY_OTEL,
    KIRO_TELEMETRY_OTLP_ENDPOINT,
    KIRO_TELEMETRY_OTLP_LOGS_ENABLED,
};
use crate::util::env_var::get_cli_client_application;
use crate::util::paths::GlobalPaths;
use crate::util::system_info::os_version;
use crate::util::{
    US_GOV_EAST,
    US_GOV_WEST,
};

#[derive(thiserror::Error, Debug)]
pub enum TelemetryError {
    #[error(transparent)]
    Client(Box<amzn_toolkit_telemetry_client::operation::post_metrics::PostMetricsError>),
    #[error(transparent)]
    Send(Box<mpsc::error::SendError<Event>>),
    #[error(transparent)]
    ApiClient(Box<crate::api_client::ApiClientError>),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Timeout(#[from] Elapsed),
}

impl From<amzn_toolkit_telemetry_client::operation::post_metrics::PostMetricsError> for TelemetryError {
    fn from(value: amzn_toolkit_telemetry_client::operation::post_metrics::PostMetricsError) -> Self {
        Self::Client(Box::new(value))
    }
}

impl From<Box<mpsc::error::SendError<Event>>> for TelemetryError {
    fn from(value: Box<mpsc::error::SendError<Event>>) -> Self {
        Self::Send(value)
    }
}

impl From<ApiClientError> for TelemetryError {
    fn from(value: ApiClientError) -> Self {
        Self::ApiClient(Box::new(value))
    }
}

const PRODUCT: &str = "CodeWhisperer";
const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A IDE toolkit telemetry stage
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TelemetryStage {
    pub endpoint: &'static str,
    pub cognito_pool_id: &'static str,
    pub region: Region,
}

impl TelemetryStage {
    #[cfg(test)]
    const BETA: Self = Self::new(
        "https://7zftft3lj2.execute-api.us-east-1.amazonaws.com/Beta",
        "us-east-1:db7bfc9f-8ecd-4fbb-bea7-280c16069a99",
        "us-east-1",
    );
    const EXTERNAL_PROD: Self = Self::new(
        "https://client-telemetry.us-east-1.amazonaws.com",
        "us-east-1:820fd6d1-95c0-4ca4-bffb-3f01d32da842",
        "us-east-1",
    );

    const fn new(endpoint: &'static str, cognito_pool_id: &'static str, region: &'static str) -> Self {
        Self {
            endpoint,
            cognito_pool_id,
            region: Region::from_static(region),
        }
    }
}

#[derive(Debug)]
enum TelemetrySender {
    Strong(mpsc::UnboundedSender<Event>),
    Weak(mpsc::WeakUnboundedSender<Event>),
}

impl TelemetrySender {
    fn send(&self, ev: Event) -> Result<(), Box<mpsc::error::SendError<Event>>> {
        match self {
            Self::Strong(sender) => sender.send(ev).map_err(Box::new),
            Self::Weak(sender) => {
                if let Some(sender) = sender.upgrade() {
                    sender.send(ev).map_err(Box::new)
                } else {
                    tracing::error!(
                        "Attempted to send telemetry after telemetry thread has been dropped. Event attempted {:?}",
                        ev
                    );
                    Ok(())
                }
            },
        }
    }
}

impl Clone for TelemetrySender {
    fn clone(&self) -> Self {
        match self {
            Self::Strong(sender) => Self::Weak(sender.downgrade()),
            Self::Weak(sender) => Self::Weak(sender.clone()),
        }
    }
}

#[derive(Debug)]
pub struct TelemetryThread {
    handle: Option<JoinHandle<()>>,
    tx: TelemetrySender,
}

impl Clone for TelemetryThread {
    fn clone(&self) -> Self {
        Self {
            handle: None,
            tx: self.tx.clone(),
        }
    }
}

impl TelemetryThread {
    pub async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        region: Option<&str>,
    ) -> Result<Self, TelemetryError> {
        // govcloud does not have the infrastructure to support toolkit telemetry
        let govcloud_partition = region.and_then(govcloud_partition);
        let telemetry_client = TelemetryClient::new(env, fs, database, govcloud_partition).await?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tx = TelemetrySender::Strong(tx);

        let handle = if let Some(partition) = govcloud_partition {
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    trace!("TelemetryThread received new telemetry event: {:?}", event);
                    trace!("Dropping toolkit telemetry");
                    telemetry_client
                        .send_event_with_legacy_toolkit_disabled(event, partition)
                        .await;
                }
                telemetry_client.flush_otel();
            })
        } else {
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    trace!("TelemetryThread received new telemetry event: {:?}", event);
                    telemetry_client.send_event(event).await;
                }
                telemetry_client.flush_otel();
            })
        };

        Ok(Self {
            handle: Some(handle),
            tx,
        })
    }

    pub async fn finish(self) -> Result<(), TelemetryError> {
        drop(self.tx);
        if let Some(handle) = self.handle {
            match tokio::time::timeout(std::time::Duration::from_millis(1000), handle).await {
                Ok(result) => {
                    if let Err(e) = result {
                        return Err(TelemetryError::Join(e));
                    }
                },
                Err(_) => {
                    // Ignore timeout errors
                },
            }
        }

        Ok(())
    }

    pub fn send_user_logged_in(&self) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::UserLoggedIn {}))?)
    }

    /// Send a pre-built telemetry event directly (used by TelemetryObserver).
    pub fn send_event(&self, event: Event) -> Result<(), TelemetryError> {
        Ok(self.tx.send(event)?)
    }

    pub fn send_auth_failed(
        &self,
        auth_method: &str,
        oauth_flow: &str,
        error_type: &str,
        error_code: Option<String>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::AuthFailed {
            auth_method: auth_method.to_string(),
            oauth_flow: oauth_flow.to_string(),
            error_type: error_type.to_string(),
            error_code,
        }))?)
    }

    pub fn send_daily_heartbeat(&self) -> Result<(), TelemetryError> {
        self.send_daily_heartbeat_with_client_application(get_cli_client_application())
    }

    fn send_daily_heartbeat_with_client_application(
        &self,
        client_application: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::DailyHeartbeat {
            install_method: Some(get_install_method(BREW_CASK_NAME, CLI_NAME).to_string()),
        });
        if let Some(client_app) = client_application {
            event.set_client_application(client_app);
        } else {
            event.set_client_application_kind(metric::ClientApplication::ChatCliV2);
        }
        Ok(self.tx.send(event)?)
    }

    pub async fn send_cli_subcommand_executed(
        &self,
        database: &Database,
        subcommand: &RootSubcommand,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::CliSubcommandExecuted {
            subcommand: subcommand.telemetry_name(),
        });
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_chat_slash_command_executed(
        &self,
        database: &Database,
        conversation_id: String,
        command: String,
        subcommand: Option<String>,
        result: TelemetryResult,
        reason: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ChatSlashCommandExecuted {
            conversation_id,
            command,
            subcommand,
            result,
            reason,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    #[allow(clippy::too_many_arguments)] // TODO: Should make a parameters struct.
    pub async fn send_agent_contribution_metric(
        &self,
        database: &Database,
        conversation_id: String,
        utterance_id: Option<String>,
        tool_use_id: Option<String>,
        tool_name: Option<String>,
        lines_by_agent: Option<isize>,
        lines_by_user: Option<isize>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::AgentContribution {
            conversation_id,
            utterance_id,
            tool_use_id,
            tool_name,
            lines_by_agent,
            lines_by_user,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    #[allow(clippy::too_many_arguments)] // TODO: Should make a parameters struct.
    pub async fn send_chat_added_message(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ChatAddedMessage {
            conversation_id,
            result,
            data,
        });
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_record_user_turn_completion(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_metering_event(
        &self,
        database: &Database,
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::MeteringEvent {
            request_id,
            model,
            usage,
            unit,
            unit_plural,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_empty_response_retry(
        &self,
        database: &Database,
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::EmptyResponseRetry { model, outcome });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    pub fn send_subagent_record_user_turn_completion(
        &self,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        let telemetry_event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_tangent_mode_session(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: TangentModeSessionArgs,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::TangentModeSession {
            conversation_id,
            result,
            args,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_tool_use_suggested(
        &self,
        database: &Database,
        event: ToolUseEventBuilder,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ToolUseSuggested {
            conversation_id: event.conversation_id,
            utterance_id: event.utterance_id,
            user_input_id: event.user_input_id,
            tool_use_id: event.tool_use_id,
            tool_name: event.tool_name,
            mcp_server_name: event.mcp_server_name,
            is_accepted: event.is_accepted,
            is_trusted: event.is_trusted,
            is_success: event.is_success,
            reason_desc: event.reason_desc,
            is_valid: event.is_valid,
            is_custom_tool: event.is_custom_tool,
            input_token_size: event.input_token_size,
            output_token_size: event.output_token_size,
            custom_tool_call_latency: event.custom_tool_call_latency,
            model: event.model,
            execution_duration: event.execution_duration,
            turn_duration: event.turn_duration,
            aws_service_name: event.aws_service_name,
            aws_operation_name: event.aws_operation_name,
        });
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_mcp_server_init(
        &self,
        database: &Database,
        conversation_id: String,
        server_name: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(crate::telemetry::EventType::McpServerInit {
            conversation_id,
            server_name,
            init_failure_reason,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            all_tools_count,
        });
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_agent_config_init(
        &self,
        database: &Database,
        conversation_id: String,
        args: AgentConfigInitArgs,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(crate::telemetry::EventType::AgentConfigInit { conversation_id, args });
        set_event_metadata(database, &mut telemetry_event).await;
        Ok(self.tx.send(telemetry_event)?)
    }

    pub fn send_did_select_profile(
        &self,
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
        profile_count: Option<i64>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::DidSelectProfile {
            source,
            amazonq_profile_region,
            result,
            sso_region,
            profile_count,
        }))?)
    }

    pub fn send_profile_state(
        &self,
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ProfileState {
            source,
            amazonq_profile_region,
            result,
            sso_region,
        }))?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_response_error(
        &self,
        database: &Database,
        conversation_id: String,
        context_file_length: Option<usize>,
        result: TelemetryResult,
        reason: Option<String>,
        reason_desc: Option<String>,
        status_code: Option<u16>,
        request_id: Option<String>,
        message_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::MessageResponseError {
            result,
            reason,
            reason_desc,
            status_code,
            conversation_id,
            context_file_length,
            request_id,
            message_id,
            model: None,
        });
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
    }

    pub fn send_subagent_invocation(
        &self,
        parent_conversation_id: String,
        subagent_name: String,
        builtin_tool_uses: u32,
        mcp_tool_uses: u32,
        parent_tool_use_id: String,
    ) -> Result<(), TelemetryError> {
        let telemetry_event = Event::new(EventType::SubagentInvocation {
            parent_conversation_id,
            subagent_name,
            builtin_tool_uses,
            mcp_tool_uses,
            parent_tool_use_id,
        });

        Ok(self.tx.send(telemetry_event)?)
    }

    #[cfg(feature = "voice")]
    #[allow(clippy::too_many_arguments)]
    pub fn send_voice_input(
        &self,
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
    ) -> Result<(), TelemetryError> {
        let telemetry_event = Event::new(EventType::VoiceInput {
            conversation_id,
            result,
            reason,
            reason_desc,
            backend,
            input_method,
            recording_duration_ms,
            transcription_duration_ms,
            text_length,
            model_size,
            auto_submit,
        });

        Ok(self.tx.send(telemetry_event)?)
    }

    /// Emit a `kirocli_goalCompleted` event when a goal loop reaches a
    /// terminal state (the agent calls goal-complete, the iteration cap is
    /// hit, or the user clears the goal).
    ///
    /// Used to track average iterations to completion and the breakdown of
    /// terminal states.
    #[allow(clippy::too_many_arguments)]
    pub fn send_goal_completed(
        &self,
        conversation_id: Option<String>,
        terminal_state: String,
        iterations: i64,
        max_iterations: i64,
        duration_sec: i64,
    ) -> Result<(), TelemetryError> {
        let telemetry_event = Event::new(EventType::GoalCompleted {
            conversation_id,
            terminal_state,
            iterations,
            max_iterations,
            duration_sec,
        });
        Ok(self.tx.send(telemetry_event)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_process_health_snapshot(
        &self,
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
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::ProcessHealthMetric {
            agent_kind,
            rss_mb,
            heap_used_mb,
            peak_rss_mb,
            cpu_user_pct,
            cpu_system_pct,
            last_render_ms,
            max_render_ms,
            renders_per_min,
            full_redraws_per_min,
            yoga_node_count,
            event_loop_p99_ms,
            input_latency_p95_ms,
            session_duration_sec,
            cpu_cores,
            total_memory_mb,
            terminal,
            session_id,
            version,
            platform,
        });
        Ok(self.tx.send(event)?)
    }

    /// Emit a single `modeChanged` telemetry event. Caller is responsible for skipping no-op
    /// changes (`from_mode == to_mode`); we just record what we're handed.
    pub fn send_mode_changed(
        &self,
        from_mode: String,
        to_mode: String,
        source: crate::agent::acp::schema::ModeChangeSource,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::ModeChanged {
            from_mode,
            to_mode,
            source,
            session_id,
        });
        Ok(self.tx.send(event)?)
    }

    /// Emit `uiModeSessionStart`. Fires exactly once per session, after the TUI resolves
    /// the UI mode at startup.
    pub fn send_ui_mode_session_start(
        &self,
        ui_mode: String,
        ui_mode_source: crate::agent::acp::schema::UiModeSource,
        ui_mode_default: String,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::UiModeSessionStart {
            ui_mode,
            ui_mode_source,
            ui_mode_default,
            session_id,
        });
        Ok(self.tx.send(event)?)
    }

    /// Emit `uiModeChanged`. Caller is responsible for skipping no-op changes.
    pub fn send_ui_mode_changed(
        &self,
        from: String,
        to: String,
        source: crate::agent::acp::schema::ModeChangeSource,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::UiModeChanged {
            from,
            to,
            source,
            session_id,
        });
        Ok(self.tx.send(event)?)
    }

    /// Emit `uiModeDefaultChanged`. Caller is responsible for skipping no-op writes.
    pub fn send_ui_mode_default_changed(
        &self,
        from: String,
        to: String,
        session_id: Option<String>,
    ) -> Result<(), TelemetryError> {
        let event = Event::new(EventType::UiModeDefaultChanged { from, to, session_id });
        Ok(self.tx.send(event)?)
    }
}

pub(crate) async fn set_event_metadata(database: &Database, event: &mut Event) {
    let (start_url, region) = get_start_url_and_region(database).await;
    if let Some(start_url) = start_url {
        event.set_start_url(start_url);
    }
    if let Some(region) = region {
        event.set_sso_region(region);
    }

    // Set the client application from environment variable
    if let Some(client_app) = get_cli_client_application() {
        event.set_client_application(client_app);
    }
}

fn govcloud_partition(region: &str) -> Option<&'static str> {
    match region {
        US_GOV_EAST | US_GOV_WEST => Some("aws-us-gov"),
        _ => None,
    }
}

fn should_build_toolkit_telemetry_client(telemetry_enabled: bool, govcloud_partition: Option<&str>) -> bool {
    telemetry_enabled && govcloud_partition.is_none() && cfg!(feature = "legacy_toolkit_sink")
}

fn should_build_codewhisperer_telemetry_client() -> bool {
    cfg!(feature = "legacy_codewhisperer_sink")
}

#[derive(Debug)]
struct TelemetryClient {
    client_id: Uuid,
    telemetry_enabled: bool,
    otel_providers: OtelProviders,
    otel_telemetry_client: Arc<OtelTelemetryClient>,
    codewhisperer_client: Option<ApiClient>,
    toolkit_telemetry_client: Option<ToolkitTelemetryClient>,
}

impl TelemetryClient {
    async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        govcloud_partition: Option<&str>,
    ) -> Result<Self, TelemetryError> {
        let telemetry_enabled = !cfg!(test)
            && !crate::util::env_var::is_telemetry_disabled()
            && database.settings.get_bool(Setting::TelemetryEnabled).unwrap_or(true);

        // GovCloud must not construct the legacy commercial Toolkit telemetry client.
        let toolkit_telemetry_client = if should_build_toolkit_telemetry_client(telemetry_enabled, govcloud_partition) {
            Some(ToolkitTelemetryClient::from_conf(
                Config::builder()
                    .http_client(crate::aws_common::http_client::client())
                    .behavior_version(BehaviorVersion::v2026_01_12())
                    .endpoint_resolver(StaticEndpoint(TelemetryStage::EXTERNAL_PROD.endpoint))
                    .app_name(app_name())
                    .region(TelemetryStage::EXTERNAL_PROD.region.clone())
                    .credentials_provider(SharedCredentialsProvider::new(CognitoProvider::new(
                        TelemetryStage::EXTERNAL_PROD,
                    )))
                    .build(),
            ))
        } else {
            None
        };

        fn client_id(env: &Env, database: &mut Database, telemetry_enabled: bool) -> Result<Uuid, TelemetryError> {
            if !telemetry_enabled {
                return Ok(uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff"));
            }

            if let Ok(client_id) = crate::util::env_var::get_telemetry_client_id(env)
                && let Ok(uuid) = Uuid::from_str(&client_id)
            {
                return Ok(uuid);
            }

            Ok(match database.get_client_id()? {
                Some(uuid) => uuid,
                None => {
                    let uuid = database
                        .settings
                        .get_string(Setting::OldClientId)
                        .and_then(|id| Uuid::try_parse(&id).ok())
                        .unwrap_or_else(Uuid::new_v4);

                    if let Err(err) = database.set_client_id(uuid) {
                        error!(%err, "Failed to set client id in state");
                    }

                    uuid
                },
            })
        }

        // cw telemetry is only available with bearer token auth.
        let codewhisperer_client = if should_build_codewhisperer_telemetry_client() {
            Some(ApiClient::new(env, fs, database, None).await?)
        } else {
            None
        };
        let client_id = client_id(env, database, telemetry_enabled)?;
        let otel_config = otel_telemetry_config(env, telemetry_enabled, client_id);
        let otel_providers = init_otel(&otel_config);
        let mut otel_telemetry_client = OtelTelemetryClient::new(otel_config.clone())
            .with_sink(std::sync::Arc::new(OtelMetricsSink::new(kiro_telemetry::meter())));
        if otel_config.otlp_logs_enabled() {
            otel_telemetry_client =
                otel_telemetry_client.with_sink(std::sync::Arc::new(OtelLogsSink::from_providers(&otel_providers)));
        }
        let otel_telemetry_client = Arc::new(otel_telemetry_client);

        let client = Self {
            client_id,
            telemetry_enabled,
            otel_providers,
            otel_telemetry_client,
            toolkit_telemetry_client,
            codewhisperer_client,
        };
        client.emit_consent_record_integrity();
        Ok(client)
    }

    /// Sends a telemetry event to both the CW and toolkit API's. If the clients do not exist, then
    /// telemetry is not sent.
    ///
    /// See [TelemetryClient::new] for which conditions the clients are created for.
    async fn send_event(&self, event: Event) {
        let legacy_event_type = event.ty.legacy_event_type();
        if self.otel_exports_enabled() {
            if let Some(legacy_event_type) = legacy_event_type {
                trace!(
                    legacy_event_type = legacy_event_type.as_str(),
                    "OTel telemetry configured for legacy event"
                );
            } else {
                trace!("OTel telemetry configured for native event");
            }
        }
        self.emit_otel_metric_record(&event);
        self.emit_otel_log_record(&event);
        #[cfg(feature = "legacy_codewhisperer_sink")]
        self.send_cw_telemetry_event(&event).await;
        #[cfg(not(feature = "legacy_codewhisperer_sink"))]
        trace!("legacy CodeWhisperer telemetry sink disabled by cargo feature");
        #[cfg(feature = "legacy_toolkit_sink")]
        self.send_telemetry_toolkit_metric(event).await;
        #[cfg(not(feature = "legacy_toolkit_sink"))]
        {
            let _ = event;
            trace!("legacy Toolkit telemetry sink disabled by cargo feature");
        }
    }

    async fn send_event_with_legacy_toolkit_disabled(&self, event: Event, partition: &str) {
        let legacy_event_type = event.ty.legacy_event_type();
        if self.otel_exports_enabled() {
            if let Some(legacy_event_type) = legacy_event_type {
                trace!(
                    legacy_event_type = legacy_event_type.as_str(),
                    "OTel telemetry configured for GovCloud legacy event"
                );
            } else {
                trace!("OTel telemetry configured for GovCloud native event");
            }
        }
        if self.toolkit_telemetry_client.is_some() {
            self.emit_govcloud_channel_leak("legacy_toolkit");
        }
        self.emit_govcloud_channel_disabled("legacy_toolkit", partition);
        self.emit_otel_metric_record(&event);
        self.emit_otel_log_record(&event);
        #[cfg(feature = "legacy_codewhisperer_sink")]
        self.send_cw_telemetry_event(&event).await;
        #[cfg(not(feature = "legacy_codewhisperer_sink"))]
        trace!("legacy CodeWhisperer telemetry sink disabled by cargo feature");
    }

    fn otel_exports_enabled(&self) -> bool {
        self.otel_telemetry_client.config().exports_enabled()
    }

    fn flush_otel(&self) {
        if let Err(err) = self.otel_providers.force_flush() {
            trace!(%err, "failed to flush no-op OTel provider");
        }
    }

    fn emit_otel_metric_record(&self, event: &Event) {
        if !self.otel_exports_enabled() {
            return;
        }

        let records = event_to_otel_metric_records(event);
        if records.is_empty() {
            if let Some(legacy_event_type) = event.ty.legacy_event_type() {
                trace!(
                    legacy_event_type = legacy_event_type.as_str(),
                    "legacy event maps to OTel log or derived target; metric record not emitted"
                );
            } else {
                trace!("native event maps to OTel log or derived target; metric record not emitted");
            }
            return;
        }

        for record in records {
            if let Err(err) = self.otel_telemetry_client.emit(record) {
                trace!(%err, "failed to emit OTel legacy metric record");
            }
        }
    }

    fn emit_otel_log_record(&self, event: &Event) {
        if !self.otel_exports_enabled() {
            return;
        }

        let Some(record) = event_to_otel_log_record(event) else {
            return;
        };

        if let Err(err) = self.otel_telemetry_client.emit_log(record) {
            trace!(%err, "failed to emit OTel legacy log record");
        }
    }

    fn emit_govcloud_channel_disabled(&self, channel: &str, partition: &str) {
        if let Err(err) = self
            .otel_telemetry_client
            .emit(metric::govcloud_channel_disabled_record(
                metric::GovcloudChannelDisabled::from_names(channel, partition),
            ))
        {
            trace!(%err, channel, partition, "failed to emit GovCloud disabled-channel counter");
        }
    }

    fn emit_govcloud_channel_leak(&self, channel: &str) {
        if let Err(err) = self.otel_telemetry_client.emit(metric::govcloud_channel_leak_record(
            metric::GovcloudChannelLeak::from_name(channel),
        )) {
            trace!(%err, channel, "failed to emit GovCloud channel leak counter");
        }
    }

    fn emit_consent_record_integrity(&self) {
        if !self.otel_exports_enabled() {
            return;
        }

        let Ok(settings_path) = GlobalPaths::settings_path() else {
            trace!("failed to resolve settings path for consent integrity telemetry");
            return;
        };

        for record in consent_file_integrity_records(settings_path) {
            if let Err(err) = self.otel_telemetry_client.emit(record) {
                trace!(%err, "failed to emit consent integrity accounting");
            }
        }
    }

    async fn send_cw_telemetry_event(&self, event: &Event) {
        let Some(codewhisperer_client) = self.codewhisperer_client.clone() else {
            trace!("not sending cw metric - client does not exist");
            return;
        };

        match &event.ty {
            EventType::ChatAddedMessage {
                conversation_id,
                data:
                    ChatAddedMessageParams {
                        message_id,
                        model,
                        time_to_first_chunk_ms,
                        time_between_chunks_ms,
                        assistant_response_length,
                        ..
                    },
                ..
            } => {
                let user_context = self.user_context().unwrap();
                // Short-Term fix for Validation errors -
                // chatAddMessageEvent.timeBetweenChunks' : Member must have length less than or equal to 100
                let time_between_chunks_truncated = time_between_chunks_ms
                    .as_ref()
                    .map(|chunks| chunks.iter().take(100).cloned().collect());

                let chat_add_message_event = match ChatAddMessageEvent::builder()
                    .conversation_id(conversation_id)
                    .message_id(message_id.clone().unwrap_or("not_set".to_string()))
                    .set_time_to_first_chunk_milliseconds(*time_to_first_chunk_ms)
                    .set_time_between_chunks(time_between_chunks_truncated)
                    .set_response_length(*assistant_response_length)
                    .build()
                {
                    Ok(event) => event,
                    Err(err) => {
                        error!(err =% DisplayErrorContext(err), "Failed to send cw telemetry event");
                        return;
                    },
                };

                let event = TelemetryEvent::ChatAddMessageEvent(chat_add_message_event);
                debug!(
                    ?event,
                    ?user_context,
                    telemetry_enabled = self.telemetry_enabled,
                    "Sending cw telemetry event"
                );
                if let Err(err) = codewhisperer_client
                    .send_telemetry_event(event, user_context, self.telemetry_enabled, model.to_owned())
                    .await
                {
                    error!(err =% DisplayErrorContext(err), "Failed to send cw telemetry event");
                }
            },
            EventType::AgentContribution {
                conversation_id,
                utterance_id,
                lines_by_agent,
                ..
            } => {
                let user_context = self.user_context().unwrap();

                let builder = ChatInteractWithMessageEvent::builder()
                    .conversation_id(conversation_id)
                    .message_id(utterance_id.clone().unwrap_or("not_set".to_string()))
                    .accepted_line_count(lines_by_agent.map_or(0, |lines| lines as i32))
                    .interaction_type(ChatMessageInteractionType::AgenticCodeAccepted);

                let chat_interact_event = match builder.build() {
                    Ok(event) => event,
                    Err(err) => {
                        error!(err =% DisplayErrorContext(err), "Failed to build ChatInteractWithMessageEvent");
                        return;
                    },
                };

                let event = TelemetryEvent::ChatInteractWithMessageEvent(chat_interact_event);
                debug!(
                    ?event,
                    ?user_context,
                    telemetry_enabled = self.telemetry_enabled,
                    "Sending cw telemetry event"
                );
                if let Err(err) = codewhisperer_client
                    .send_telemetry_event(event, user_context, self.telemetry_enabled, None)
                    .await
                {
                    error!(err =% DisplayErrorContext(err), "Failed to send cw telemetry event");
                }
            },
            _ => {
                // No CW telemetry event for other event types
            },
        }
    }

    async fn send_telemetry_toolkit_metric(&self, event: Event) {
        let Some(toolkit_telemetry_client) = self.toolkit_telemetry_client.clone() else {
            trace!("not sending toolkit metric - client does not exist");
            return;
        };
        let client_id = self.client_id;
        self.emit_redaction_metric_records(&event);
        let Some(metric_datum) = event_to_metric_datum(event) else {
            trace!("not sending toolkit metric - metric datum does not exist");
            return;
        };

        let product = AwsProduct::CodewhispererTerminal;
        let metric_name = metric_datum.metric_name().to_owned();

        debug!(?client_id, ?product, ?metric_datum, "Sending toolkit telemetry event");
        if let Err(err) = toolkit_telemetry_client
            .post_metrics()
            .aws_product(product)
            .aws_product_version(env!("CARGO_PKG_VERSION"))
            .client_id(client_id)
            .os(std::env::consts::OS)
            .os_architecture(std::env::consts::ARCH)
            .os_version(os_version().map(|v| v.to_string()).unwrap_or_default())
            .metric_data(metric_datum)
            .send()
            .await
            .map_err(DisplayErrorContext)
        {
            error!(%err, ?metric_name, "Failed to post toolkit metric");
        }
    }

    fn emit_redaction_metric_records(&self, event: &Event) {
        if !self.otel_exports_enabled() {
            return;
        }

        for record in event.redaction_metric_records(kiro_telemetry::metric::TelemetryChannel::LegacyToolkit) {
            if let Err(err) = self.otel_telemetry_client.emit(record) {
                trace!(%err, "failed to emit telemetry redaction accounting");
            }
        }
    }

    fn user_context(&self) -> Option<UserContext> {
        let operating_system = match std::env::consts::OS {
            "linux" => OperatingSystem::Linux,
            "macos" => OperatingSystem::Mac,
            "windows" => OperatingSystem::Windows,
            os => {
                error!(%os, "Unsupported operating system");
                return None;
            },
        };

        match UserContext::builder()
            .client_id(self.client_id.hyphenated().to_string())
            .operating_system(operating_system)
            .product(PRODUCT)
            .ide_category(IdeCategory::Cli)
            .ide_version(PRODUCT_VERSION)
            .build()
        {
            Ok(user_context) => Some(user_context),
            Err(err) => {
                error!(%err, "Failed to build user context");
                None
            },
        }
    }
}

/// Default OTLP collector endpoint when `KIRO_TELEMETRY_OTLP_ENDPOINT` is not overridden.
const DEFAULT_OTLP_ENDPOINT: &str = "https://prod.us-east-1.telemetry-v2.kiro.dev";

fn otel_telemetry_config(env: &Env, telemetry_enabled: bool, client_id: Uuid) -> OtelTelemetryConfig {
    let otel_mode = env
        .get(KIRO_TELEMETRY_OTEL)
        .map_or(OtelMode::Off, |value| OtelMode::parse(&value));
    let otlp_endpoint = env
        .get(KIRO_TELEMETRY_OTLP_ENDPOINT)
        .ok()
        .or_else(|| Some(DEFAULT_OTLP_ENDPOINT.to_string()));
    let state_dir = GlobalPaths::database_path_static()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::env::temp_dir().join("kiro-cli"));

    let otlp_logs_enabled = env
        .get(KIRO_TELEMETRY_OTLP_LOGS_ENABLED)
        .is_ok_and(|value| value.trim() != "0");

    OtelTelemetryConfig::new(telemetry_enabled, otel_mode, otlp_endpoint, state_dir)
        .with_otlp_logs_enabled(otlp_logs_enabled)
        .with_machine_id(client_id.hyphenated().to_string())
}

pub use kiro_telemetry_host::ReasonCode;
// Re-exported for parity with the `chat-cli` (V1) crate API even though no V2 caller currently
// uses it. The `bin` target sees this as an unused re-export, so we silence the lint.
#[allow(unused_imports)]
pub use kiro_telemetry_host::get_error_reason;

#[cfg(test)]
mod test {
    use kiro_telemetry::testing::{
        InMemoryTelemetry,
        expect_log,
        expect_log_attrs,
        expect_metric,
        expect_metric_attrs,
        in_memory_telemetry,
    };
    use kiro_telemetry::{
        log as telemetry_log,
        metric,
    };
    use uuid::uuid;

    use super::*;

    #[tokio::test]
    async fn client_context() {
        let mut database = Database::new().await.unwrap();
        let client = TelemetryClient::new(&Env::new(), &Fs::new(), &mut database, None)
            .await
            .unwrap();
        let context = client.user_context().unwrap();

        assert_eq!(context.ide_category, IdeCategory::Cli);
        assert!(matches!(
            context.operating_system,
            OperatingSystem::Linux | OperatingSystem::Mac | OperatingSystem::Windows
        ));
        assert_eq!(context.product, PRODUCT);
        assert_eq!(
            context.client_id,
            Some(uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff").hyphenated().to_string())
        );
        assert_eq!(context.ide_version.as_deref(), Some(PRODUCT_VERSION));
        assert!(!client.otel_exports_enabled());
    }

    #[test]
    fn otel_config_parses_new_env_controls() {
        let env = Env::from_slice(&[
            (KIRO_TELEMETRY_OTEL, "1"),
            (
                KIRO_TELEMETRY_OTLP_ENDPOINT,
                "https://prod.us-east-1.telemetry-v2.kiro.dev",
            ),
            (KIRO_TELEMETRY_OTLP_LOGS_ENABLED, "0"),
        ]);
        let client_id = uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e");
        let config = otel_telemetry_config(&env, true, client_id);

        assert_eq!(config.otel_mode, OtelMode::DualWrite);
        assert!(config.exports_enabled());
        assert!(!config.otlp_logs_enabled());
        assert_eq!(config.machine_id, client_id.hyphenated().to_string());
        assert_eq!(config.deployment_environment, "prod");
        assert_eq!(
            config.otlp_endpoint.as_deref(),
            Some("https://prod.us-east-1.telemetry-v2.kiro.dev")
        );
    }

    #[test]
    fn otel_config_defaults_kuts_logs_off() {
        let env = Env::from_slice(&[(KIRO_TELEMETRY_OTEL, "2")]);
        let config = otel_telemetry_config(&env, true, uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"));

        assert!(config.exports_enabled());
        assert!(!config.otlp_logs_enabled());
    }

    #[test]
    fn govcloud_partition_detects_gov_regions() {
        assert_eq!(govcloud_partition(US_GOV_EAST), Some("aws-us-gov"));
        assert_eq!(govcloud_partition(US_GOV_WEST), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-east-1"), None);
    }

    #[test]
    fn govcloud_disabled_record_shape() {
        let record = metric::govcloud_channel_disabled_record(metric::GovcloudChannelDisabled::from_names(
            "legacy_toolkit",
            "aws-us-gov",
        ));

        expect_metric(
            std::slice::from_ref(&record),
            metric::govcloud_channel_disabled_from_names("legacy_toolkit", "aws-us-gov"),
        );
    }

    #[test]
    fn govcloud_leak_record_shape() {
        let record = metric::govcloud_channel_leak_record(metric::GovcloudChannelLeak::from_name("legacy_toolkit"));

        expect_metric(
            std::slice::from_ref(&record),
            metric::govcloud_channel_leak_from_name("legacy_toolkit"),
        );
    }

    #[test]
    fn legacy_sink_feature_flags_control_client_construction() {
        assert_eq!(
            should_build_toolkit_telemetry_client(true, None),
            cfg!(feature = "legacy_toolkit_sink")
        );
        assert!(!should_build_toolkit_telemetry_client(false, None));
        assert!(!should_build_toolkit_telemetry_client(true, Some("aws-us-gov")));
        assert_eq!(
            should_build_codewhisperer_telemetry_client(),
            cfg!(feature = "legacy_codewhisperer_sink")
        );
    }

    #[tokio::test]
    async fn send_event_emits_otel_metrics_to_configured_sink() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let InMemoryTelemetry {
            providers,
            client: otel_telemetry_client,
            sink,
        } = in_memory_telemetry(
            OtelTelemetryConfig::new(true, OtelMode::DualWrite, None, tempdir.path().to_path_buf())
                .with_otlp_logs_enabled(true),
        );
        let client = TelemetryClient {
            client_id: Uuid::nil(),
            telemetry_enabled: true,
            otel_providers: providers,
            otel_telemetry_client,
            codewhisperer_client: None,
            toolkit_telemetry_client: None,
        };
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id: "conversation".to_string(),
            result: TelemetryResult::Succeeded,
            data: ChatAddedMessageParams {
                model: Some("claude-4-sonnet".to_string()),
                time_to_first_chunk_ms: Some(250.0),
                request_duration_seconds: Some(1.5),
                output_tokens: Some(7),
                ..Default::default()
            },
        });
        event.client_application = Some("chat_cli_v2".to_string());

        client.send_event(event).await;
        client
            .send_event(Event::new(EventType::McpServerInit {
                conversation_id: "conversation".to_string(),
                server_name: "local-server".to_string(),
                init_failure_reason: None,
                number_of_tools: 2,
                all_tool_names: Some("read,write".to_string()),
                loaded_tool_names: Some("read,write".to_string()),
                all_tools_count: 2,
            }))
            .await;
        client
            .send_event(Event::new(EventType::GoalCompleted {
                conversation_id: Some("conversation".to_string()),
                terminal_state: "completed".to_string(),
                iterations: 2,
                max_iterations: 4,
                duration_sec: 12,
            }))
            .await;
        client
            .send_event(Event::new(EventType::ChatSlashCommandExecuted {
                conversation_id: "conversation".to_string(),
                command: "/model".to_string(),
                subcommand: Some("list".to_string()),
                result: TelemetryResult::Succeeded,
                reason: None,
            }))
            .await;
        client
            .send_event(Event::new(EventType::CliSubcommandExecuted {
                subcommand: "version".to_string(),
            }))
            .await;

        let records = sink.records();
        let turn_record = expect_metric(
            &records,
            metric::user_turns(
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                metric::ResultKind::Success,
                false,
                metric::Mode::Interactive,
            ),
        );
        expect_metric_attrs(turn_record, &[
            ("model_class", "anthropic_sonnet"),
            ("client_application", "chat_cli_v2"),
            ("result", "success"),
            ("mode", "interactive"),
        ]);
        let invocation_record = expect_metric(&records, metric::model_invocation(metric::ModelClass::AnthropicSonnet));
        expect_metric_attrs(invocation_record, &[("model_class", "anthropic_sonnet")]);
        expect_metric(
            &records,
            metric::time_to_first_chunk_ms(
                250.0,
                metric::ModelClass::AnthropicSonnet,
                metric::ClientApplication::ChatCliV2,
                false,
            ),
        );
        expect_metric(
            &records,
            metric::bedrock_stream_ttft(
                0.25,
                metric::ModelClass::AnthropicSonnet,
                metric::PromptSizeBucket::Other,
                false,
            ),
        );
        expect_metric(
            &records,
            metric::bedrock_stream_duration(
                1.5,
                metric::ModelClass::AnthropicSonnet,
                telemetry_log::CompletionReason::Stop,
            ),
        );
        expect_metric(
            &records,
            metric::bedrock_request_duration(
                1.5,
                metric::ModelClass::AnthropicSonnet,
                metric::Operation::Stream,
                metric::Outcome::Success,
            ),
        );
        expect_metric(
            &records,
            metric::tokens_consumed(
                7,
                metric::ModelClass::AnthropicSonnet,
                metric::TokenType::Output,
                metric::ClientApplication::ChatCliV2,
                false,
            ),
        );
        let mcp_record = expect_metric(
            &records,
            metric::mcp_server_init_total(metric::McpServerClass::UserDefined, metric::Outcome::Success),
        );
        expect_metric_attrs(mcp_record, &[
            ("mcp_server_class", "user_defined"),
            ("outcome", "success"),
        ]);
        expect_metric(
            &records,
            metric::mcp_server_connected_total(metric::McpServerClass::UserDefined),
        );
        expect_metric(&records, metric::session_outcome(metric::SessionOutcome::TaskCompleted));
        let slash_record = expect_metric(&records, metric::slash_command_invoked("/model"));
        expect_metric_attrs(slash_record, &[("command", "/model")]);
        let feature_record = expect_metric(&records, metric::feature_used("version"));
        expect_metric_attrs(feature_record, &[("feature", "version")]);

        let log_records = sink.log_records();
        let mcp_log = expect_log(
            &log_records,
            telemetry_log::mcp_server_init(
                "local-server",
                metric::McpServerClass::UserDefined,
                metric::Outcome::Success,
            ),
        );
        expect_log_attrs(mcp_log, &[
            ("mcp_server_name", "local-server"),
            ("mcp_server_class", "user_defined"),
            ("outcome", "success"),
        ]);
    }

    #[tokio::test]
    async fn govcloud_disabled_send_path_emits_posture_metric() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let InMemoryTelemetry {
            providers,
            client: otel_telemetry_client,
            sink,
        } = in_memory_telemetry(OtelTelemetryConfig::new(
            true,
            OtelMode::DualWrite,
            None,
            tempdir.path().to_path_buf(),
        ));
        let client = TelemetryClient {
            client_id: Uuid::nil(),
            telemetry_enabled: true,
            otel_providers: providers,
            otel_telemetry_client,
            codewhisperer_client: None,
            toolkit_telemetry_client: None,
        };

        client
            .send_event_with_legacy_toolkit_disabled(
                Event::new(EventType::ModelInvocation {
                    model: Some("claude-4-sonnet".to_string()),
                }),
                "aws-us-gov",
            )
            .await;

        let records = sink.records();
        expect_metric(
            &records,
            metric::govcloud_channel_disabled_record(metric::GovcloudChannelDisabled::from_names(
                "legacy_toolkit",
                "aws-us-gov",
            )),
        );
        expect_metric(
            &records,
            metric::model_invocation_record(metric::ModelInvocation::from_id(Some("claude-4-sonnet"))),
        );
    }

    #[tokio::test]
    async fn daily_heartbeat_send_path_defaults_to_v2_client_application() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let thread = TelemetryThread {
            handle: None,
            tx: TelemetrySender::Strong(tx),
        };

        thread
            .send_daily_heartbeat_with_client_application(None)
            .expect("daily heartbeat should be sent");

        let event = rx.recv().await.expect("daily heartbeat event should be queued");

        assert_eq!(
            event.client_application.as_deref(),
            Some(metric::ClientApplication::ChatCliV2.as_str())
        );
        let record = event_to_otel_metric_record(&event).expect("daily heartbeat metric");
        expect_metric(
            std::slice::from_ref(&record),
            metric::daily_heartbeat_record(metric::DailyHeartbeat::from_names(
                Some(metric::ClientApplication::ChatCliV2.as_str()),
                event_install_method(&event).as_deref(),
            )),
        );
    }

    fn event_install_method(event: &Event) -> Option<String> {
        match &event.ty {
            EventType::DailyHeartbeat { install_method } => install_method.clone(),
            other => panic!("expected daily heartbeat event, got {other:?}"),
        }
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_send() {
        let mut database = Database::new().await.unwrap();
        let thread = TelemetryThread::new(&Env::new(), &Fs::new(), &mut database, None)
            .await
            .unwrap();
        thread.send_user_logged_in().ok();
        drop(thread);

        assert!(!logs_contain("ERROR"));
        assert!(!logs_contain("error"));
        assert!(!logs_contain("WARN"));
        assert!(!logs_contain("warn"));
        assert!(!logs_contain("Failed to post metric"));
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_all_telemetry() {
        let mut database = Database::new().await.unwrap();
        let thread = TelemetryThread::new(&Env::new(), &Fs::new(), &mut database, None)
            .await
            .unwrap();

        thread.send_user_logged_in().ok();
        thread
            .send_cli_subcommand_executed(&database, &RootSubcommand::Version { changelog: None })
            .await
            .ok();
        thread
            .send_chat_added_message(
                &database,
                "conv_id".to_owned(),
                TelemetryResult::Succeeded,
                ChatAddedMessageParams {
                    message_id: Some("message_id".to_owned()),
                    context_file_length: Some(123),
                    ..Default::default()
                },
            )
            .await
            .ok();

        drop(thread);

        assert!(!logs_contain("ERROR"));
        assert!(!logs_contain("error"));
        assert!(!logs_contain("WARN"));
        assert!(!logs_contain("warn"));
        assert!(!logs_contain("Failed to post metric"));
    }

    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_without_optout() {
        let mut database = Database::new().await.unwrap();
        let client = TelemetryClient::new(&Env::new(), &Fs::new(), &mut database, None)
            .await
            .unwrap();
        client
            .codewhisperer_client
            .as_ref()
            .expect("cw telemetry client should exist")
            .send_telemetry_event(
                TelemetryEvent::ChatAddMessageEvent(
                    ChatAddMessageEvent::builder()
                        .conversation_id("debug".to_owned())
                        .message_id("debug".to_owned())
                        .build()
                        .unwrap(),
                ),
                client.user_context().unwrap(),
                false,
                Some("model".to_owned()),
            )
            .await
            .unwrap();
    }

    #[test]
    fn test_process_health_metric_into_datum() {
        use crate::telemetry::core::{
            Event,
            EventType,
        };

        let event = Event::new(EventType::ProcessHealthMetric {
            agent_kind: metric::AgentKind::V2,
            rss_mb: 142.0,
            heap_used_mb: 87.0,
            peak_rss_mb: 205.0,
            cpu_user_pct: 3.2,
            cpu_system_pct: 1.1,
            last_render_ms: 4.5,
            max_render_ms: 12.3,
            renders_per_min: 45,
            full_redraws_per_min: 2,
            yoga_node_count: 128,
            event_loop_p99_ms: Some(8.7),
            input_latency_p95_ms: Some(15.2),
            session_duration_sec: 300,
            cpu_cores: 10,
            total_memory_mb: 32768,
            terminal: "iTerm.app".to_string(),
            session_id: Some("test-session-123".to_string()),
            version: "2.4.0".to_string(),
            platform: "darwin".to_string(),
        });

        let datum = event_to_metric_datum(event);
        assert!(datum.is_some());

        let datum = datum.unwrap();
        assert_eq!(datum.metric_name(), "codewhispererterminal_processHealthSnapshot");
        assert!(datum.passive());
        assert_eq!(datum.value(), 1.0);

        let metadata = datum.metadata();
        assert!(!metadata.is_empty());

        // Check key fields are present
        let keys: Vec<&str> = metadata.iter().filter_map(|m| m.key()).collect();
        assert!(keys.contains(&"codewhispererterminal_tuiVersion"));
        assert!(keys.contains(&"codewhispererterminal_platform"));
        assert!(keys.contains(&"codewhispererterminal_rssMb"));
        assert!(keys.contains(&"codewhispererterminal_heapUsedMb"));
        assert!(keys.contains(&"codewhispererterminal_cpuUserPct"));
        assert!(keys.contains(&"codewhispererterminal_eventLoopP99Ms"));
        assert!(keys.contains(&"codewhispererterminal_inputLatencyP95Ms"));

        // Check values
        let find_value =
            |key: &str| -> Option<&str> { metadata.iter().find(|m| m.key() == Some(key)).and_then(|m| m.value()) };
        assert_eq!(find_value("codewhispererterminal_tuiVersion"), Some("2.4.0"));
        assert_eq!(find_value("codewhispererterminal_platform"), Some("darwin"));
        assert_eq!(find_value("codewhispererterminal_rssMb"), Some("142"));
        assert_eq!(find_value("codewhispererterminal_rendersPerMin"), Some("45"));
    }

    #[test]
    fn test_process_health_metric_optional_fields_none() {
        use crate::telemetry::core::{
            Event,
            EventType,
        };

        let event = Event::new(EventType::ProcessHealthMetric {
            agent_kind: metric::AgentKind::V2,
            rss_mb: 100.0,
            heap_used_mb: 50.0,
            peak_rss_mb: 150.0,
            cpu_user_pct: 0.0,
            cpu_system_pct: 0.0,
            last_render_ms: 0.0,
            max_render_ms: 0.0,
            renders_per_min: 0,
            full_redraws_per_min: 0,
            yoga_node_count: 0,
            event_loop_p99_ms: None,
            input_latency_p95_ms: None,
            session_duration_sec: 60,
            cpu_cores: 4,
            total_memory_mb: 16384,
            terminal: "unknown".to_string(),
            session_id: None,
            version: "1.0.0".to_string(),
            platform: "linux".to_string(),
        });

        let datum = event_to_metric_datum(event);
        assert!(datum.is_some());

        let datum = datum.unwrap();
        let metadata = datum.metadata();
        let find_value =
            |key: &str| -> Option<&str> { metadata.iter().find(|m| m.key() == Some(key)).and_then(|m| m.value()) };
        // Optional fields should be empty string when None
        assert_eq!(find_value("codewhispererterminal_eventLoopP99Ms"), Some(""));
        assert_eq!(find_value("codewhispererterminal_inputLatencyP95Ms"), Some(""));
    }
}
