pub mod cognito;
pub mod core;
pub mod endpoint;
mod legacy_sink;

use core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    ToolUseEventBuilder,
};
use std::sync::atomic::{
    AtomicU8,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use kiro_telemetry::{
    MetricRecord,
    OtelMode,
    TelemetryConfig as OtelTelemetryConfig,
    metric,
};
pub use kiro_telemetry_host::{
    InstallMethod,
    get_accurate_install_method,
    get_install_method,
};
use kiro_telemetry_legacy::event_to_otel_metric_records;
pub use legacy_sink::TelemetryStage;
use legacy_sink::{
    TelemetryClient,
    resolve_client_id,
};
use uuid::Uuid;

use crate::api_client::ApiClientError;
use crate::auth::builder_id::get_start_url_and_region;
use crate::cli::chat::tools::ToolMetadata;
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
};
use crate::util::env_var::get_cli_client_application;
use crate::util::paths::GlobalPaths;
use crate::util::{
    US_GOV_EAST,
    US_GOV_WEST,
};

const STARTUP_PENDING: u8 = 0;
const STARTUP_SUCCEEDED: u8 = 1;
const STARTUP_FAILED: u8 = 2;

#[derive(thiserror::Error, Debug)]
pub enum TelemetryError {
    #[error("failed to enqueue telemetry event")]
    Send,
    #[error(transparent)]
    Host(#[from] kiro_telemetry_host::TelemetryError),
    #[error(transparent)]
    ApiClient(Box<crate::api_client::ApiClientError>),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

impl From<ApiClientError> for TelemetryError {
    fn from(value: ApiClientError) -> Self {
        Self::ApiClient(Box::new(value))
    }
}

#[derive(Debug)]
struct V1OtelTranslator;

impl kiro_telemetry_host::OtelEventTranslator for V1OtelTranslator {
    fn metric_records(&self, event: &Event) -> Vec<MetricRecord> {
        event_to_otel_metric_records(event)
    }
}

#[derive(Clone, Debug)]
pub struct TelemetryThread {
    host: Arc<Mutex<Option<kiro_telemetry_host::TelemetryThread>>>,
    identity_epochs: Arc<kiro_telemetry::IdentityEpochs>,
    client_id: Uuid,
    enabled: bool,
    startup_state: Arc<AtomicU8>,
}

impl TelemetryThread {
    pub async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        region: Option<&str>,
        telemetry_enabled: bool,
    ) -> Result<Self, TelemetryError> {
        let govcloud_partition = region.and_then(govcloud_partition);
        let client_id = resolve_client_id(env, database, telemetry_enabled)?;
        let persisted_user_id = database.get_telemetry_user_id().unwrap_or_else(|error| {
            tracing::warn!(%error, "Failed to load persisted telemetry identity");
            None
        });
        let otel_config =
            otel_telemetry_config(env, telemetry_enabled, client_id, region).with_user_id(persisted_user_id);
        let legacy_sink =
            Arc::new(TelemetryClient::new(env, fs, database, govcloud_partition, client_id, telemetry_enabled).await?);
        let host = kiro_telemetry_host::TelemetryThread::new(kiro_telemetry_host::HostConfig {
            client_id,
            telemetry_enabled,
            otel_config,
            legacy_sink: Some(legacy_sink),
            otel_translator: Some(Arc::new(V1OtelTranslator)),
            metadata_enricher: None,
            client_application: None,
            engine: None,
            host_role: kiro_telemetry_host::HostRole::UserCli,
            process_identity: None,
            govcloud_partition,
        })
        .await?;
        let identity_epochs = host.identity_epochs();

        Ok(Self {
            host: Arc::new(Mutex::new(Some(host))),
            identity_epochs,
            client_id,
            enabled: telemetry_enabled,
            startup_state: Arc::new(AtomicU8::new(STARTUP_PENDING)),
        })
    }

    pub(crate) fn client_id(&self) -> Uuid {
        self.client_id
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn send(&self, mut event: Event) -> Result<(), TelemetryError> {
        prepare_v1_event(&mut event);
        self.host
            .lock()
            .expect("telemetry host mutex poisoned")
            .as_ref()
            .ok_or(TelemetryError::Send)?
            .send_event(event)?;
        Ok(())
    }

    pub(crate) fn identity_epochs(&self) -> Arc<kiro_telemetry::IdentityEpochs> {
        Arc::clone(&self.identity_epochs)
    }

    pub(crate) fn set_process_identity(&self, engine: metric::Engine, role: metric::ProcessRole) {
        if let Some(host) = self.host.lock().expect("telemetry host mutex poisoned").as_ref() {
            host.set_process_identity(engine, role);
        }
    }

    pub(crate) fn startup_succeeded(&self) -> bool {
        self.startup_state.load(Ordering::Acquire) == STARTUP_SUCCEEDED
    }

    pub async fn finish(&self) -> Result<(), TelemetryError> {
        self.finish_with_timeout(Duration::from_secs(2)).await
    }

    async fn finish_with_timeout(&self, timeout: Duration) -> Result<(), TelemetryError> {
        let host = self.host.lock().expect("telemetry host mutex poisoned").take();
        if let Some(host) = host {
            host.finish_with_timeout(timeout).await?;
        }
        Ok(())
    }

    pub async fn send_user_logged_in(&self, database: &Database) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::UserLoggedIn {});
        set_event_metadata(database, &mut event).await;
        self.send(event)
    }

    pub async fn send_cli_session_started(
        &self,
        database: &Database,
        client_application: metric::ClientApplication,
        session_interface: metric::SessionInterface,
        engine: metric::Engine,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = cli_session_started_event(client_application, session_interface, engine);
        set_event_metadata(database, &mut telemetry_event).await;
        telemetry_event.set_client_application_kind(client_application);

        self.send(telemetry_event)
    }

    pub async fn send_cli_session_completed(
        &self,
        database: &Database,
        exit_reason: metric::ExitReason,
        agent_kind: metric::AgentKind,
        session_interface: metric::SessionInterface,
        engine: metric::Engine,
        run_outcome: metric::RunOutcome,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event =
            cli_session_completed_event(exit_reason, agent_kind, session_interface, engine, run_outcome);
        set_event_metadata(database, &mut telemetry_event).await;

        self.send(telemetry_event)
    }

    pub fn send_startup_duration(
        &self,
        duration_seconds: f64,
        session_interface: metric::SessionInterface,
        engine: metric::Engine,
    ) -> Result<(), TelemetryError> {
        if self
            .startup_state
            .compare_exchange(STARTUP_PENDING, STARTUP_SUCCEEDED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        let mut event = Event::new(EventType::StartupDuration {
            duration_seconds,
            os_type: metric::OsType::from_name(cli_os_type()),
        });
        event.set_session_interface(session_interface);
        event.set_engine(engine);
        self.send(event)
    }

    pub fn send_startup_failure(
        &self,
        session_interface: metric::SessionInterface,
        engine: metric::Engine,
        failure_stage: metric::StartupFailureStage,
    ) -> Result<(), TelemetryError> {
        if self
            .startup_state
            .compare_exchange(STARTUP_PENDING, STARTUP_FAILED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        let mut event = Event::new(EventType::StartupFailure {
            os_type: metric::OsType::from_name(cli_os_type()),
            failure_stage,
        });
        event.set_session_interface(session_interface);
        event.set_engine(engine);
        self.send(event)
    }

    pub fn send_auth_failed(
        &self,
        auth_method: &str,
        oauth_flow: &str,
        error_type: &str,
        error_code: Option<String>,
    ) -> Result<(), TelemetryError> {
        self.send(Event::new(EventType::AuthFailed {
            auth_method: auth_method.to_string(),
            oauth_flow: oauth_flow.to_string(),
            error_type: error_type.to_string(),
            error_code,
        }))
    }

    pub fn send_daily_heartbeat(&self) -> Result<(), TelemetryError> {
        self.send(Event::new(EventType::DailyHeartbeat {
            install_method: Some(install_source().to_string()),
        }))
    }

    pub fn send_chat_session_started(
        &self,
        session_interface: metric::SessionInterface,
        agent_mode: metric::AgentMode,
        trust_posture: metric::TrustPosture,
    ) -> Result<(), TelemetryError> {
        self.send_chat_session_started_for_engine(session_interface, agent_mode, metric::Engine::V1, trust_posture)
    }

    pub(crate) fn send_chat_session_started_for_engine(
        &self,
        session_interface: metric::SessionInterface,
        agent_mode: metric::AgentMode,
        engine: metric::Engine,
        trust_posture: metric::TrustPosture,
    ) -> Result<(), TelemetryError> {
        self.send(chat_session_started_event(
            session_interface,
            agent_mode,
            engine,
            trust_posture,
        ))
    }

    pub async fn send_cli_subcommand_executed(
        &self,
        database: &Database,
        subcommand: String,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::CliSubcommandExecuted { subcommand });
        set_event_metadata(database, &mut telemetry_event).await;

        self.send(telemetry_event)
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
        self.send(telemetry_event)
    }

    pub async fn send_chat_start(
        &self,
        database: &Database,
        conversation_id: String,
        model: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ChatStart { conversation_id, model });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    pub async fn send_chat_end(
        &self,
        database: &Database,
        conversation_id: String,
        model: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::ChatEnd { conversation_id, model });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    pub async fn send_chat_transition(
        &self,
        database: &Database,
        previous: Option<(String, Option<String>)>,
        conversation_id: String,
        model: Option<String>,
    ) -> Result<(), TelemetryError> {
        let mut previous =
            previous.map(|(conversation_id, model)| Event::new(EventType::ChatEnd { conversation_id, model }));
        let mut current = Event::new(EventType::ChatStart { conversation_id, model });
        if let Some(previous) = previous.as_mut() {
            set_event_metadata(database, previous).await;
        }
        set_event_metadata(database, &mut current).await;
        if let Some(previous) = previous {
            self.send(previous)?;
        }
        self.send(current)
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
        self.send(telemetry_event)
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

        self.send(telemetry_event)
    }

    pub async fn send_record_user_turn_completion(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        session_interface: metric::SessionInterface,
        agent_mode: metric::AgentMode,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        self.send_record_user_turn_completion_for_engine(
            database,
            conversation_id,
            result,
            session_interface,
            agent_mode,
            metric::Engine::V1,
            args,
            kiro_telemetry::MetricLogProperties::default(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_record_user_turn_completion_for_engine(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        session_interface: metric::SessionInterface,
        agent_mode: metric::AgentMode,
        engine: metric::Engine,
        args: RecordUserTurnCompletionArgs,
        log_properties: kiro_telemetry::MetricLogProperties,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        telemetry_event.set_session_interface(session_interface);
        telemetry_event.set_engine(engine);
        telemetry_event.metric_context.agent_mode = Some(agent_mode);
        telemetry_event.metric_context.log_properties = log_properties;
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
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
        self.send_metering_event_for_engine(
            database,
            request_id,
            model,
            usage,
            unit,
            unit_plural,
            metric::Engine::V1,
            kiro_telemetry::MetricLogProperties::default(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_metering_event_for_engine(
        &self,
        database: &Database,
        request_id: Option<String>,
        model: Option<String>,
        usage: f64,
        unit: String,
        unit_plural: String,
        engine: metric::Engine,
        log_properties: kiro_telemetry::MetricLogProperties,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::MeteringEvent {
            request_id,
            model,
            usage,
            unit,
            unit_plural,
        });
        telemetry_event.set_engine(engine);
        telemetry_event.metric_context.log_properties = log_properties;
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    pub async fn send_empty_response_retry(
        &self,
        database: &Database,
        model: Option<String>,
        outcome: EmptyResponseRetryOutcome,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::EmptyResponseRetry { model, outcome });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    /// Emits one stall observation plus the observed idle gap for a V1 hard stream stall.
    /// V1 has no soft tier, so every stall is hard-cancelled.
    pub async fn send_stream_stall(
        &self,
        database: &Database,
        model: Option<String>,
        idle: std::time::Duration,
        session_interface: metric::SessionInterface,
    ) -> Result<(), TelemetryError> {
        let mut stall_event = Event::new(EventType::StreamStall {
            model: model.clone(),
            tier: metric::StallTier::Hard,
        });
        stall_event.set_session_interface(session_interface);
        set_event_metadata(database, &mut stall_event).await;
        self.send(stall_event)?;

        let mut episode_event = Event::new(EventType::StreamStallEpisodeEnd {
            model,
            idle_seconds: idle.as_secs_f64(),
            episode_end: metric::StallEpisodeEnd::HardCancelled,
        });
        set_event_metadata(database, &mut episode_event).await;
        self.send(episode_event)
    }

    /// Emits a counter increment when a subagent stage is cancelled by its
    /// overall deadline.
    pub async fn send_subagent_deadline_expired(
        &self,
        database: &Database,
        deadline: std::time::Duration,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::SubagentDeadlineExpired {
            deadline_seconds: deadline.as_secs_f64(),
        });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    /// Emits the hard-cancel → first-event recovery time of a V1 stall retry.
    pub async fn send_stream_stall_recovery(
        &self,
        database: &Database,
        model: Option<String>,
        recovery: std::time::Duration,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::StreamStallRecovery {
            model,
            recovery_seconds: recovery.as_secs_f64(),
        });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    /// Emits the terminal outcome of a V1 stall-continuation retry sequence.
    pub async fn send_stream_stall_retry(
        &self,
        database: &Database,
        model: Option<String>,
        outcome: metric::RetryOutcome,
        attempt_number: u32,
        partial_output: Option<bool>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::StreamStallRetry {
            model,
            outcome,
            attempt_number,
            partial_output,
        });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    pub fn send_subagent_record_user_turn_completion(
        &self,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        telemetry_event.set_session_interface(metric::SessionInterface::InteractiveCli);
        telemetry_event.metric_context.agent_mode = Some(metric::AgentMode::Default);
        self.send(telemetry_event)
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
        self.send(telemetry_event)
    }

    pub async fn send_tool_use_suggested(
        &self,
        database: &Database,
        event: ToolUseEventBuilder,
        execution_context: metric::ExecutionContext,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = tool_use_suggested_event(event, execution_context);
        set_event_metadata(database, &mut telemetry_event).await;

        self.send(telemetry_event)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_mcp_server_init(
        &self,
        database: &Database,
        conversation_id: String,
        server_name: String,
        mcp_server_source: metric::McpServerSource,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
        all_tool_names: Option<String>,
        loaded_tool_names: Option<String>,
        all_tools_count: usize,
        mcp_tools_token_count_estimate: Option<u64>,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(crate::telemetry::EventType::McpServerInit {
            conversation_id,
            server_name,
            mcp_server_source,
            init_failure_reason,
            number_of_tools,
            all_tool_names,
            loaded_tool_names,
            all_tools_count,
            mcp_tools_token_count_estimate,
        });
        set_event_metadata(database, &mut telemetry_event).await;

        self.send(telemetry_event)
    }

    pub async fn send_agent_config_init(
        &self,
        database: &Database,
        conversation_id: String,
        args: AgentConfigInitArgs,
    ) -> Result<(), TelemetryError> {
        let mut telemetry_event = Event::new(crate::telemetry::EventType::AgentConfigInit { conversation_id, args });
        set_event_metadata(database, &mut telemetry_event).await;
        self.send(telemetry_event)
    }

    pub fn send_did_select_profile(
        &self,
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
        profile_count: Option<i64>,
    ) -> Result<(), TelemetryError> {
        self.send(Event::new(EventType::DidSelectProfile {
            source,
            amazonq_profile_region,
            result,
            sso_region,
            profile_count,
        }))
    }

    pub fn send_profile_state(
        &self,
        source: QProfileSwitchIntent,
        amazonq_profile_region: String,
        result: TelemetryResult,
        sso_region: Option<String>,
    ) -> Result<(), TelemetryError> {
        self.send(Event::new(EventType::ProfileState {
            source,
            amazonq_profile_region,
            result,
            sso_region,
        }))
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

        self.send(telemetry_event)
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

        self.send(telemetry_event)
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

        self.send(telemetry_event)
    }
}

pub(crate) fn telemetry_enabled(database: &Database) -> bool {
    !cfg!(test)
        && !crate::util::env_var::is_telemetry_disabled()
        && database.settings.get_bool(Setting::TelemetryEnabled).unwrap_or(true)
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

fn prepare_v1_event(event: &mut Event) {
    let engine = event.engine.unwrap_or(metric::Engine::V1);
    if event.engine.is_none() {
        event.set_engine(engine);
    }
    if event.client_application.is_none() {
        event.set_client_application_kind(match engine {
            metric::Engine::V1 => metric::ClientApplication::ChatCli,
            metric::Engine::V2 => metric::ClientApplication::ChatCliV2,
            metric::Engine::V3 => metric::ClientApplication::ChatCliV3,
            metric::Engine::Unknown => metric::ClientApplication::AcpExternal,
        });
    }
    if event.app_type.is_none() {
        event.app_type = Some(
            match engine {
                metric::Engine::V1 => "V1",
                metric::Engine::V2 => "V2",
                metric::Engine::V3 => "KAS",
                metric::Engine::Unknown => "ACP",
            }
            .to_string(),
        );
    }
    if event.session_interface.is_none() {
        event.set_session_interface(metric::SessionInterface::InteractiveCli);
    }
    if event.metric_context.install_method.is_none() {
        event.metric_context.install_method = Some(metric::InstallSource::from_name(install_source()));
    }

    match &mut event.ty {
        EventType::RecordUserTurnCompletion { args, .. } => {
            event.is_subagent = args.is_subagent;
        },
        EventType::ToolUseSuggested { tool_name, .. } => {
            event.metric_context.canonical_tool_name = tool_name
                .as_deref()
                .and_then(ToolMetadata::get_by_any_alias)
                .map(|metadata| metadata.spec_name.to_string());
        },
        _ => {},
    }
}

fn tool_use_suggested_event(event: ToolUseEventBuilder, execution_context: metric::ExecutionContext) -> Event {
    let mut telemetry_event = Event::new(EventType::ToolUseSuggested {
        conversation_id: event.conversation_id,
        utterance_id: event.utterance_id,
        user_input_id: event.user_input_id,
        tool_use_id: event.tool_use_id,
        tool_name: event.tool_name,
        mcp_server_name: event.mcp_server_name,
        is_accepted: event.is_accepted,
        is_trusted: event.is_trusted,
        approval_path: event.approval_path,
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
        path_scope: event.path_scope,
    });
    telemetry_event.is_subagent = execution_context == metric::ExecutionContext::Subagent;
    telemetry_event
}

fn cli_session_started_event(
    client_application: metric::ClientApplication,
    session_interface: metric::SessionInterface,
    engine: metric::Engine,
) -> Event {
    let mut event = Event::new(EventType::CliSessionStarted {
        os_type: metric::OsType::from_name(cli_os_type()),
        install_source: metric::InstallSource::from_name(install_source()),
    });
    event.set_client_application_kind(client_application);
    event.set_session_interface(session_interface);
    event.set_engine(engine);
    event
}

fn chat_session_started_event(
    session_interface: metric::SessionInterface,
    agent_mode: metric::AgentMode,
    engine: metric::Engine,
    trust_posture: metric::TrustPosture,
) -> Event {
    let mut event = Event::new(EventType::ChatSessionStarted {
        mode: metric::Mode::from_name(agent_mode.as_str()),
        trust_posture: Some(trust_posture),
    });
    event.set_session_interface(session_interface);
    event.set_engine(engine);
    event.metric_context.agent_mode = Some(agent_mode);
    event
}

fn cli_session_completed_event(
    exit_reason: metric::ExitReason,
    agent_kind: metric::AgentKind,
    session_interface: metric::SessionInterface,
    engine: metric::Engine,
    run_outcome: metric::RunOutcome,
) -> Event {
    let mut event = Event::new(EventType::CliSessionCompleted {
        exit_reason,
        agent_kind,
    });
    event.set_session_interface(session_interface);
    event.set_engine(engine);
    event.metric_context.run_outcome = Some(run_outcome);
    event
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

fn otel_telemetry_config(
    env: &Env,
    telemetry_enabled: bool,
    client_id: Uuid,
    region: Option<&str>,
) -> OtelTelemetryConfig {
    // Default to DualWrite (KUTS/OTel + legacy Toolkit) so pre-existing metrics
    // dual-hit both backends without opt-in. An explicit `KIRO_TELEMETRY_OTEL=0`
    // still parses to Off (the user opt-out), and `2` selects OtelOnly.
    let otel_mode = env
        .get(KIRO_TELEMETRY_OTEL)
        .map_or(OtelMode::DualWrite, |value| OtelMode::parse(&value));
    let otlp_endpoint = Some(kiro_telemetry::resolve_otlp_endpoint(
        env.get(KIRO_TELEMETRY_OTLP_ENDPOINT).ok(),
        region,
    ));
    let state_dir = GlobalPaths::database_path_static()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::env::temp_dir().join("kiro-cli"));

    OtelTelemetryConfig::new(telemetry_enabled, otel_mode, otlp_endpoint, state_dir)
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
#[path = "tests.rs"]
mod test;
