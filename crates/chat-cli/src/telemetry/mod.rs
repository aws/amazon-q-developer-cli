pub mod cognito;
pub mod core;
pub mod definitions;
pub mod endpoint;

use core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    ToolUseEventBuilder,
};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

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
    MetricRecord,
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
    get_accurate_install_method,
    get_install_method,
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

#[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
const PRODUCT: &str = "CodeWhisperer";
#[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
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
    /// Construct a V1 `TelemetryThread`.
    ///
    /// V1 keeps its own private internal pipeline (it has a distinct
    /// `core::Event` shape from the host crate); PR I rewires V1 to consume
    /// `HostConfig.legacy_sink`. For PR D the `_host_config` parameter is
    /// metadata-only — V1 still derives govcloud partition and constructs its
    /// local `TelemetryClient` from `(env, fs, database, region)`.
    pub async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        region: Option<&str>,
        _host_config: kiro_telemetry_host::HostConfig,
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
        self.finish_with_timeout(Duration::from_millis(1000)).await
    }

    async fn finish_with_timeout(self, timeout: Duration) -> Result<(), TelemetryError> {
        drop(self.tx);
        if let Some(handle) = self.handle {
            match tokio::time::timeout(timeout, handle).await {
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

    pub async fn send_cli_session_started(
        &self,
        database: &Database,
        client_application: metric::ClientApplication,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = cli_session_started_event(client_application);
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
    }

    pub async fn send_cli_session_completed(
        &self,
        database: &Database,
        exit_reason: metric::ExitReason,
        agent_kind: metric::AgentKind,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = cli_session_completed_event(exit_reason, agent_kind);
        set_event_metadata(database, &mut telemetry_event).await;

        Ok(self.tx.send(telemetry_event)?)
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
        Ok(self.tx.send(Event::new(EventType::DailyHeartbeat {}))?)
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
        model: Option<String>,
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
            model,
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
}

/// Build a host-level [`kiro_telemetry_host::HostConfig`] for V1.
///
/// In PR D this is metadata-only (V1's local `TelemetryClient` still owns the
/// real send paths); PR I rewires V1 to actually consume the host
/// `legacy_sink`.
pub async fn build_v1_host_config(
    env: &Env,
    database: &mut Database,
    region: Option<&str>,
) -> Result<kiro_telemetry_host::HostConfig, TelemetryError> {
    let telemetry_enabled = !cfg!(test)
        && !crate::util::env_var::is_telemetry_disabled()
        && database.settings.get_bool(Setting::TelemetryEnabled).unwrap_or(true);

    let client_id = if telemetry_enabled {
        match crate::util::env_var::get_telemetry_client_id(env) {
            Ok(id) => Uuid::from_str(&id)
                .unwrap_or_else(|_| database.get_client_id().ok().flatten().unwrap_or_else(Uuid::new_v4)),
            Err(_) => database.get_client_id().ok().flatten().unwrap_or_else(Uuid::new_v4),
        }
    } else {
        uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff")
    };

    Ok(kiro_telemetry_host::HostConfig {
        client_id,
        telemetry_enabled,
        otel_config: kiro_telemetry::TelemetryConfig::new(
            telemetry_enabled,
            kiro_telemetry::OtelMode::Off,
            None,
            std::env::temp_dir().join("kiro-cli"),
        ),
        legacy_sink: None,
        otel_translator: None,
        metadata_enricher: None,
        client_application: get_cli_client_application().map(|s| metric::ClientApplication::from_name(Some(&s))),
        host_role: kiro_telemetry_host::HostRole::UserCli,
        govcloud_partition: region.and_then(govcloud_partition),
        consent_settings_path: None,
    })
}

async fn set_event_metadata(database: &Database, event: &mut Event) {
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

fn cli_session_started_event(client_application: metric::ClientApplication) -> Event {
    let mut event = Event::new(EventType::CliSessionStarted {
        os_type: metric::OsType::from_name(cli_os_type()),
        install_source: metric::InstallSource::from_name(install_source()),
    });
    event.set_client_application_kind(client_application);
    event
}

fn cli_session_completed_event(exit_reason: metric::ExitReason, agent_kind: metric::AgentKind) -> Event {
    Event::new(EventType::CliSessionCompleted {
        exit_reason,
        agent_kind,
    })
}

fn cli_os_type() -> &'static str {
    match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        _ => "_other_",
    }
}

fn install_source() -> &'static str {
    match get_install_method() {
        InstallMethod::Brew => "brew",
        InstallMethod::Toolbox(_) => "internal",
        InstallMethod::Unknown => "unknown",
    }
}

fn govcloud_partition(region: &str) -> Option<&'static str> {
    match region {
        US_GOV_EAST | US_GOV_WEST => Some("aws-us-gov"),
        _ => None,
    }
}

fn govcloud_channel_disabled_record(channel: &str, partition: &str) -> MetricRecord {
    metric::govcloud_channel_disabled(
        metric::TelemetryChannel::from_name(channel),
        metric::Partition::from_name(partition),
        metric::PostureReason::GovcloudDisabled,
    )
}

fn govcloud_channel_leak_record(channel: &str) -> MetricRecord {
    metric::govcloud_channel_leak(metric::TelemetryChannel::from_name(channel))
}

fn should_build_toolkit_telemetry_client(telemetry_enabled: bool, govcloud_partition: Option<&str>) -> bool {
    telemetry_enabled && govcloud_partition.is_none() && cfg!(feature = "legacy_toolkit_sink")
}

fn should_build_codewhisperer_telemetry_client() -> bool {
    cfg!(feature = "legacy_codewhisperer_sink")
}

#[derive(Debug)]
struct TelemetryClient {
    #[cfg_attr(not(feature = "legacy_toolkit_sink"), allow(dead_code))]
    client_id: Uuid,
    #[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
    telemetry_enabled: bool,
    otel_providers: OtelProviders,
    otel_telemetry_client: Arc<OtelTelemetryClient>,
    #[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
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
        let otel_config = otel_telemetry_config(env, telemetry_enabled, client_id)
            .with_user_id(database.get_telemetry_user_id().ok().flatten());
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

        let records = event.otel_metric_records();
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

        let Some(record) = event.otel_log_record() else {
            return;
        };

        if let Err(err) = self.otel_telemetry_client.emit_log(record) {
            trace!(%err, "failed to emit OTel legacy log record");
        }
    }

    fn emit_govcloud_channel_disabled(&self, channel: &str, partition: &str) {
        if let Err(err) = self
            .otel_telemetry_client
            .emit(govcloud_channel_disabled_record(channel, partition))
        {
            trace!(%err, channel, partition, "failed to emit GovCloud disabled-channel counter");
        }
    }

    fn emit_govcloud_channel_leak(&self, channel: &str) {
        if let Err(err) = self.otel_telemetry_client.emit(govcloud_channel_leak_record(channel)) {
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

    #[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
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

    #[cfg_attr(not(feature = "legacy_toolkit_sink"), allow(dead_code))]
    async fn send_telemetry_toolkit_metric(&self, event: Event) {
        let Some(toolkit_telemetry_client) = self.toolkit_telemetry_client.clone() else {
            trace!("not sending toolkit metric - client does not exist");
            return;
        };
        let client_id = self.client_id;
        self.emit_redaction_metric_records(&event);
        let Some(metric_datum) = event.into_metric_datum() else {
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

    #[cfg_attr(not(feature = "legacy_toolkit_sink"), allow(dead_code))]
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

    #[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
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

/// Default OTLP endpoint (KUTS) when `KIRO_TELEMETRY_OTLP_ENDPOINT` is not overridden.
///
/// `pub(crate)` so the launcher (`crate::launch`) can resolve the same endpoint
/// it forwards to the TUI child from the single source of truth the host's own
/// OTel pipeline uses — no dependency on the collector crate.
pub(crate) const DEFAULT_OTLP_ENDPOINT: &str = "https://prod.us-east-1.telemetry-v2.kiro.dev";

fn otel_telemetry_config(env: &Env, telemetry_enabled: bool, client_id: Uuid) -> OtelTelemetryConfig {
    // Default to DualWrite (KUTS/OTel + legacy Toolkit) so pre-existing metrics
    // dual-hit both backends without opt-in. An explicit `KIRO_TELEMETRY_OTEL=0`
    // still parses to Off (the user opt-out), and `2` selects OtelOnly.
    let otel_mode = env
        .get(KIRO_TELEMETRY_OTEL)
        .map_or(OtelMode::DualWrite, |value| OtelMode::parse(&value));
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

pub trait ReasonCode: std::error::Error {
    fn reason_code(&self) -> String;
}

/// Returns a generic error reason + reason description pair.
pub fn get_error_reason<E>(error: &E) -> (String, String)
where
    E: ReasonCode + 'static,
{
    let mut err_chain = eyre::Chain::new(error);
    let reason_desc = if err_chain.len() > 1 {
        format!(
            "'{}' caused by: {}",
            error,
            err_chain.next_back().map_or("UNKNOWN".to_string(), |e| e.to_string())
        )
    } else {
        error.to_string()
    };

    (error.reason_code(), reason_desc)
}

#[cfg(test)]
mod test {
    use uuid::uuid;

    use super::*;

    #[tokio::test]
    async fn client_context() {
        let mut database = Database::new_default().await.unwrap();
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
        // Exports stay disabled here because telemetry_enabled is false under
        // cfg!(test), independent of the (now DualWrite) default OTel mode.
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
    fn cli_session_started_event_sets_launch_dimensions() {
        let event = cli_session_started_event(metric::ClientApplication::ChatCliV3);

        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v3"));
        match event.ty {
            EventType::CliSessionStarted {
                os_type,
                install_source,
            } => {
                assert!(matches!(
                    os_type,
                    metric::OsType::Linux | metric::OsType::Macos | metric::OsType::Windows | metric::OsType::Other
                ));
                assert!(matches!(
                    install_source,
                    metric::InstallSource::Brew | metric::InstallSource::Internal | metric::InstallSource::Unknown
                ));
            },
            _ => panic!("expected CLI session-started event"),
        }
    }

    #[test]
    fn cli_session_completed_event_sets_exit_dimensions() {
        let event = cli_session_completed_event(metric::ExitReason::Clean, metric::AgentKind::Kas);

        match event.ty {
            EventType::CliSessionCompleted {
                exit_reason,
                agent_kind,
            } => {
                assert_eq!(exit_reason, metric::ExitReason::Clean);
                assert_eq!(agent_kind, metric::AgentKind::Kas);
            },
            _ => panic!("expected CLI session-completed event"),
        }
    }

    #[test]
    fn govcloud_partition_detects_gov_regions() {
        assert_eq!(govcloud_partition(US_GOV_EAST), Some("aws-us-gov"));
        assert_eq!(govcloud_partition(US_GOV_WEST), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-east-1"), None);
    }

    #[test]
    fn govcloud_disabled_record_shape() {
        let record = govcloud_channel_disabled_record("legacy_toolkit", "aws-us-gov");

        assert_eq!(record.name, "kiro_cli_govcloud_channel_disabled_total");
        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert!(
            record
                .attributes
                .iter()
                .any(|attribute| attribute.key == "channel" && attribute.value == "legacy_toolkit")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attribute| attribute.key == "partition" && attribute.value == "aws-us-gov")
        );
        assert!(
            record
                .attributes
                .iter()
                .any(|attribute| attribute.key == "reason" && attribute.value == "govcloud_disabled")
        );
    }

    #[test]
    fn govcloud_leak_record_shape() {
        let record = govcloud_channel_leak_record("legacy_toolkit");

        assert_eq!(record.name, "kiro_cli_govcloud_channel_leak_total");
        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert!(
            record
                .attributes
                .iter()
                .any(|attribute| attribute.key == "channel" && attribute.value == "legacy_toolkit")
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
    async fn cloned_telemetry_thread_can_finish_before_original() {
        let mut database = Database::new_default().await.unwrap();
        let env = Env::from_slice(&[(KIRO_TELEMETRY_OTEL, "0")]);
        let host_config = build_v1_host_config(&env, &mut database, None).await.unwrap();
        let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, host_config)
            .await
            .unwrap();
        let clone = thread.clone();

        clone.finish().await.unwrap();
        thread.finish().await.unwrap();
    }

    #[tokio::test]
    async fn telemetry_thread_finish_returns_after_timeout_when_worker_is_stuck() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let thread = TelemetryThread {
            handle: Some(tokio::spawn(async {
                std::future::pending::<()>().await;
            })),
            tx: TelemetrySender::Strong(tx),
        };

        thread.finish_with_timeout(Duration::from_millis(1)).await.unwrap();
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_send() {
        let mut database = Database::new_default().await.unwrap();
        let env = Env::new();
        let host_config = build_v1_host_config(&env, &mut database, None).await.unwrap();
        let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, host_config)
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
        let mut database = Database::new_default().await.unwrap();
        let env = Env::new();
        let host_config = build_v1_host_config(&env, &mut database, None).await.unwrap();
        let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, host_config)
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
        let mut database = Database::new_default().await.unwrap();
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
}
