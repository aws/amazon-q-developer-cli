pub mod cognito;
pub mod core;
pub mod endpoint;

use core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    RecordUserTurnCompletionArgs,
    TangentModeSessionArgs,
    ToolUseEventBuilder,
};
use std::future::Future;
use std::str::FromStr;
use std::sync::atomic::{
    AtomicU8,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
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
use amzn_toolkit_telemetry_client::config::endpoint::{
    Endpoint,
    EndpointFuture,
    Params,
    ResolveEndpoint,
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
    OtelMetricsSink,
    OtelMode,
    OtelProviders,
    TelemetryClient as OtelTelemetryClient,
    TelemetryConfig as OtelTelemetryConfig,
    init_otel,
    metric,
};
pub use kiro_telemetry_host::{
    InstallMethod,
    get_accurate_install_method,
    get_install_method,
};
use kiro_telemetry_legacy::{
    event_to_metric_datum,
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
use crate::util::system_info::os_version;
use crate::util::{
    US_GOV_EAST,
    US_GOV_WEST,
};

const KIRO_TELEMETRY_TOOLKIT_ENDPOINT: &str = "KIRO_TELEMETRY_TOOLKIT_ENDPOINT";
const STARTUP_PENDING: u8 = 0;
const STARTUP_SUCCEEDED: u8 = 1;
const STARTUP_FAILED: u8 = 2;

#[derive(thiserror::Error, Debug)]
pub enum TelemetryError {
    #[error(transparent)]
    Client(Box<amzn_toolkit_telemetry_client::operation::post_metrics::PostMetricsError>),
    #[error("failed to enqueue telemetry event")]
    Send,
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
struct ToolkitTelemetryEndpoint(String);

impl ResolveEndpoint for ToolkitTelemetryEndpoint {
    fn resolve_endpoint<'a>(&'a self, _params: &'a Params) -> EndpointFuture<'a> {
        EndpointFuture::ready(Ok(Endpoint::builder().url(self.0.clone()).build()))
    }
}

#[derive(Debug, Default)]
struct TelemetryRuntime {
    otel_handle: Option<JoinHandle<()>>,
    legacy_handle: Option<JoinHandle<()>>,
    otel_providers: Option<OtelProviders>,
    tx: Option<mpsc::UnboundedSender<Event>>,
}

#[derive(Clone, Debug)]
pub struct TelemetryThread {
    #[cfg_attr(not(test), allow(dead_code))]
    enabled: bool,
    runtime: Arc<Mutex<TelemetryRuntime>>,
    client_id: Uuid,
    process_identity: Arc<Mutex<Option<kiro_telemetry_host::ProcessIdentity>>>,
    run_receipt_store: Option<kiro_telemetry_host::RunReceiptStore>,
    run_receipt: Arc<Mutex<Option<kiro_telemetry_host::RunReceipt>>>,
    startup_state: Arc<AtomicU8>,
}

async fn await_shutdown_branches<OtelShutdown, LegacyShutdown>(
    otel_shutdown: OtelShutdown,
    legacy_shutdown: LegacyShutdown,
) -> Result<(), TelemetryError>
where
    OtelShutdown: Future<Output = Result<(), TelemetryError>>,
    LegacyShutdown: Future<Output = Result<(), TelemetryError>>,
{
    let (otel_result, legacy_result) = tokio::join!(otel_shutdown, legacy_shutdown);
    otel_result.and(legacy_result)
}

async fn await_worker_until(
    handle: Option<JoinHandle<()>>,
    deadline: tokio::time::Instant,
    operation: &'static str,
) -> Result<(), TelemetryError> {
    let Some(mut handle) = handle else {
        return Ok(());
    };
    match tokio::time::timeout_at(deadline, &mut handle).await {
        Ok(result) => result.map_err(TelemetryError::Join),
        Err(_) => {
            handle.abort();
            let _ = handle.await;
            trace!(operation, "telemetry shutdown deadline elapsed");
            Ok(())
        },
    }
}

async fn force_flush_otel_until(providers: Option<OtelProviders>, deadline: tokio::time::Instant) {
    let Some(providers) = providers else {
        return;
    };
    let (flush_tx, flush_rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = flush_tx.send(providers.force_flush().map_err(|err| err.to_string()));
    });
    match tokio::time::timeout_at(deadline, flush_rx).await {
        Ok(Ok(Ok(()))) => {},
        Ok(Ok(Err(err))) => trace!(%err, "failed to force-flush OTel providers"),
        Ok(Err(_)) => trace!("OTel force-flush worker exited without a result"),
        Err(_) => trace!("timed out force-flushing OTel providers"),
    }
}

async fn finish_otel_until(
    handle: Option<JoinHandle<()>>,
    providers: Option<OtelProviders>,
    deadline: tokio::time::Instant,
) -> Result<(), TelemetryError> {
    let worker_result = await_worker_until(handle, deadline, "draining V1 OTel queue").await;
    force_flush_otel_until(providers, deadline).await;
    worker_result
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
        let telemetry_client = Arc::new(TelemetryClient::new(env, fs, database, region, telemetry_enabled).await?);
        let client_id = telemetry_client.client_id;
        let receipt_store = kiro_telemetry_host::RunReceiptStore::new(telemetry_client.otel_state_dir());
        let run_receipt_store = telemetry_client.otel_exports_enabled().then_some(receipt_store.clone());
        if telemetry_client.otel_exports_enabled() {
            let recovery = receipt_store.recover();
            let records = recovery.records().collect::<Vec<_>>();
            telemetry_client.emit_otel_records(records.iter().cloned());
            if records.is_empty() || telemetry_client.flush_otel() {
                recovery.acknowledge();
            }
        } else {
            receipt_store.clear_unlocked();
        }
        let run_receipt = Arc::new(Mutex::new(None));
        let process_identity = Arc::new(Mutex::new(None));
        let worker_process_identity = Arc::clone(&process_identity);
        let mut process_sampler = telemetry_client
            .otel_exports_enabled()
            .then(kiro_telemetry_host::ProcessSampler::new)
            .flatten();
        let mut process_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (legacy_tx, mut legacy_rx) = mpsc::unbounded_channel();
        let otel_providers = telemetry_client.otel_providers.clone();

        let legacy_client = Arc::clone(&telemetry_client);
        let legacy_handle = tokio::spawn(async move {
            while let Some(event) = legacy_rx.recv().await {
                legacy_client.send_legacy_event(event, govcloud_partition).await;
            }
        });
        let otel_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    event = rx.recv() => {
                        let Some(event) = event else {
                            break;
                        };
                        trace!("TelemetryThread received new telemetry event: {:?}", event);
                        telemetry_client.emit_otel_event(&event);
                        if legacy_tx.send(event).is_err() {
                            trace!("legacy telemetry worker stopped before event delivery");
                        }
                    },
                    _ = process_interval.tick(), if process_sampler.is_some() => {
                        let identity = *worker_process_identity
                            .lock()
                            .expect("process identity mutex poisoned");
                        if let (Some(identity), Some(sampler)) = (identity, process_sampler.as_mut()) {
                            telemetry_client.emit_otel_records(sampler.sample(identity));
                        }
                    },
                }
            }
            let identity = *worker_process_identity.lock().expect("process identity mutex poisoned");
            if let (Some(identity), Some(sampler)) = (identity, process_sampler.as_mut()) {
                telemetry_client.emit_otel_records(sampler.final_sample(identity));
            }
        });

        Ok(Self {
            enabled: telemetry_enabled,
            runtime: Arc::new(Mutex::new(TelemetryRuntime {
                otel_handle: Some(otel_handle),
                legacy_handle: Some(legacy_handle),
                otel_providers: Some(otel_providers),
                tx: Some(tx),
            })),
            client_id,
            process_identity,
            run_receipt_store,
            run_receipt,
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
        let sender = self.runtime.lock().unwrap().tx.clone().ok_or(TelemetryError::Send)?;
        sender.send(event).map_err(|_error| TelemetryError::Send)
    }

    pub(crate) fn set_process_identity(&self, engine: metric::Engine, role: metric::ProcessRole) {
        let identity = kiro_telemetry_host::ProcessIdentity::new(engine, role);
        *self.process_identity.lock().expect("process identity mutex poisoned") = Some(identity);
        if let Some(store) = self.run_receipt_store.as_ref() {
            let mut receipt = self.run_receipt.lock().expect("run receipt mutex poisoned");
            if receipt.is_none() {
                *receipt = store.start(identity).ok();
            }
        }
    }

    pub(crate) fn startup_succeeded(&self) -> bool {
        self.startup_state.load(Ordering::Acquire) == STARTUP_SUCCEEDED
    }

    pub async fn finish(&self) -> Result<(), TelemetryError> {
        self.finish_with_timeout(Duration::from_secs(2)).await
    }

    async fn finish_with_timeout(&self, timeout: Duration) -> Result<(), TelemetryError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let (otel_handle, legacy_handle, otel_providers) = {
            let mut runtime = self.runtime.lock().unwrap();
            runtime.tx.take();
            (
                runtime.otel_handle.take(),
                runtime.legacy_handle.take(),
                runtime.otel_providers.take(),
            )
        };
        let result = await_shutdown_branches(
            finish_otel_until(otel_handle, otel_providers, deadline),
            await_worker_until(legacy_handle, deadline, "draining V1 legacy telemetry queue"),
        )
        .await;
        if let Some(receipt) = self.run_receipt.lock().expect("run receipt mutex poisoned").take() {
            receipt.complete();
        }
        result
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
    ) -> Result<(), TelemetryError> {
        self.send_chat_session_started_for_engine(session_interface, agent_mode, metric::Engine::V1)
    }

    pub(crate) fn send_chat_session_started_for_engine(
        &self,
        session_interface: metric::SessionInterface,
        agent_mode: metric::AgentMode,
        engine: metric::Engine,
    ) -> Result<(), TelemetryError> {
        self.send(chat_session_started_event(session_interface, agent_mode, engine))
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
) -> Event {
    let mut event = Event::new(EventType::ChatSessionStarted {
        mode: metric::Mode::from_name(agent_mode.as_str()),
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

fn govcloud_channel_leak_record(channel: &str) -> MetricRecord {
    let channel = match channel {
        "legacy_toolkit" => metric::TelemetryChannelName::LegacyToolkit,
        "legacy_codewhisperer" => metric::TelemetryChannelName::LegacyCodewhisperer,
        "otel" => metric::TelemetryChannelName::Otel,
        "kuts" => metric::TelemetryChannelName::Kuts,
        _ => metric::TelemetryChannelName::Unknown,
    };
    metric::record_prohibited_telemetry_channel_enabled(channel)
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
        region: Option<&str>,
        telemetry_enabled: bool,
    ) -> Result<Self, TelemetryError> {
        let govcloud_partition = region.and_then(govcloud_partition);

        // GovCloud must not construct the legacy commercial Toolkit telemetry client.
        let toolkit_telemetry_client = if should_build_toolkit_telemetry_client(telemetry_enabled, govcloud_partition) {
            let config = Config::builder()
                .http_client(crate::aws_common::http_client::client())
                .behavior_version(BehaviorVersion::v2026_01_12())
                .app_name(app_name())
                .region(TelemetryStage::EXTERNAL_PROD.region.clone())
                .credentials_provider(SharedCredentialsProvider::new(CognitoProvider::new(
                    TelemetryStage::EXTERNAL_PROD,
                )));
            let config = match env.get(KIRO_TELEMETRY_TOOLKIT_ENDPOINT) {
                Ok(endpoint) => config.endpoint_resolver(ToolkitTelemetryEndpoint(endpoint)),
                Err(_) => config.endpoint_resolver(StaticEndpoint(TelemetryStage::EXTERNAL_PROD.endpoint)),
            };
            Some(ToolkitTelemetryClient::from_conf(config.build()))
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
        let otel_config = otel_telemetry_config(env, telemetry_enabled, client_id, region)
            .with_user_id(database.get_telemetry_user_id().ok().flatten());
        let otel_providers = init_otel(&otel_config);
        let otel_telemetry_client = OtelTelemetryClient::new(otel_config.clone())
            .with_sink(std::sync::Arc::new(OtelMetricsSink::new(kiro_telemetry::meter())));
        if govcloud_partition.is_some()
            && toolkit_telemetry_client.is_some()
            && let Err(err) = otel_telemetry_client.emit(govcloud_channel_leak_record("legacy_toolkit"))
        {
            trace!(%err, "failed to emit prohibited GovCloud telemetry-channel counter");
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
        Ok(client)
    }

    fn emit_otel_event(&self, event: &Event) {
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
        self.emit_otel_metric_record(event);
    }

    async fn send_legacy_event(&self, event: Event, govcloud_partition: Option<&str>) {
        #[cfg(feature = "legacy_codewhisperer_sink")]
        self.send_cw_telemetry_event(&event).await;
        #[cfg(not(feature = "legacy_codewhisperer_sink"))]
        trace!("legacy CodeWhisperer telemetry sink disabled by cargo feature");

        if govcloud_partition.is_some() {
            trace!("legacy Toolkit telemetry disabled in GovCloud");
            return;
        }

        #[cfg(feature = "legacy_toolkit_sink")]
        self.send_telemetry_toolkit_metric(event).await;
        #[cfg(not(feature = "legacy_toolkit_sink"))]
        {
            let _ = event;
            trace!("legacy Toolkit telemetry sink disabled by cargo feature");
        }
    }

    fn otel_exports_enabled(&self) -> bool {
        self.otel_telemetry_client.config().exports_enabled()
    }

    fn flush_otel(&self) -> bool {
        match self.otel_providers.force_flush() {
            Ok(()) => true,
            Err(err) => {
                trace!(%err, "failed to flush OTel provider");
                false
            },
        }
    }

    fn otel_state_dir(&self) -> std::path::PathBuf {
        self.otel_telemetry_client.config().state_dir.clone()
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

        let properties = event.metric_log_properties();
        for record in records {
            if let Err(err) = self.otel_telemetry_client.emit_with_log_properties(record, &properties) {
                trace!(%err, "failed to emit OTel legacy metric record");
            }
        }
    }

    fn emit_otel_records(&self, records: impl IntoIterator<Item = MetricRecord>) {
        if !self.otel_exports_enabled() {
            return;
        }
        for record in records {
            if let Err(err) = self.otel_telemetry_client.emit(record) {
                trace!(%err, "failed to emit OTel legacy metric record");
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
mod test {
    use uuid::uuid;

    use super::*;

    #[tokio::test]
    async fn client_context() {
        let mut database = Database::new_default().await.unwrap();
        let client = TelemetryClient::new(&Env::new(), &Fs::new(), &mut database, None, false)
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
    fn otel_config_parses_env_controls() {
        let env = Env::from_slice(&[
            (KIRO_TELEMETRY_OTEL, "1"),
            (
                KIRO_TELEMETRY_OTLP_ENDPOINT,
                "https://prod.us-east-1.telemetry-v2.kiro.dev",
            ),
        ]);
        let client_id = uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e");
        let config = otel_telemetry_config(&env, true, client_id, Some("eu-central-1"));

        assert_eq!(config.otel_mode, OtelMode::DualWrite);
        assert!(config.exports_enabled());
        assert_eq!(config.machine_id, client_id.hyphenated().to_string());
        assert_eq!(config.deployment_environment, "prod");
        assert_eq!(
            config.otlp_endpoint.as_deref(),
            Some("https://prod.us-east-1.telemetry-v2.kiro.dev")
        );
    }

    #[test]
    fn otel_config_defaults_to_regional_kuts_endpoint() {
        let env = Env::from_slice(&[(KIRO_TELEMETRY_OTEL, "2")]);
        let config = otel_telemetry_config(
            &env,
            true,
            uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            Some("eu-central-1"),
        );

        assert!(config.exports_enabled());
        assert_eq!(
            config.otlp_endpoint.as_deref(),
            Some("https://prod.eu-central-1.telemetry-v2.kiro.dev")
        );
    }

    #[test]
    fn cli_session_started_event_sets_launch_dimensions() {
        let event = cli_session_started_event(
            metric::ClientApplication::ChatCliV3,
            metric::SessionInterface::InteractiveCli,
            metric::Engine::V3,
        );

        assert_eq!(event.client_application.as_deref(), Some("chat_cli_v3"));
        assert_eq!(event.session_interface, Some(metric::SessionInterface::InteractiveCli));
        assert_eq!(event.engine, Some(metric::Engine::V3));
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
        let event = cli_session_completed_event(
            metric::ExitReason::Clean,
            metric::AgentKind::Kas,
            metric::SessionInterface::InteractiveCli,
            metric::Engine::V3,
            metric::RunOutcome::Success,
        );

        assert_eq!(event.session_interface, Some(metric::SessionInterface::InteractiveCli));
        assert_eq!(event.engine, Some(metric::Engine::V3));
        assert_eq!(event.metric_context.run_outcome, Some(metric::RunOutcome::Success));
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
    fn chat_session_started_round_trips_typed_agent_modes() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let thread = TelemetryThread {
            enabled: true,
            runtime: Arc::new(Mutex::new(TelemetryRuntime {
                otel_handle: None,
                legacy_handle: None,
                otel_providers: None,
                tx: Some(tx),
            })),
            client_id: uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            process_identity: Arc::new(Mutex::new(None)),
            run_receipt_store: None,
            run_receipt: Arc::new(Mutex::new(None)),
            startup_state: Arc::new(AtomicU8::new(STARTUP_PENDING)),
        };

        for agent_mode in [metric::AgentMode::Spec, metric::AgentMode::Autonomous] {
            thread
                .send_chat_session_started_for_engine(
                    metric::SessionInterface::NoninteractiveCli,
                    agent_mode,
                    metric::Engine::V3,
                )
                .unwrap();
            let event = rx.try_recv().unwrap();
            let record = kiro_telemetry_legacy::event_to_otel_metric_record(&event).unwrap();
            let recorded_agent_mode = record
                .attributes
                .iter()
                .find(|attribute| attribute.key == "agent_mode")
                .map(|attribute| attribute.value.as_str());

            assert_eq!(recorded_agent_mode, Some(agent_mode.as_str()));
        }
    }

    #[test]
    fn govcloud_partition_detects_gov_regions() {
        assert_eq!(govcloud_partition(US_GOV_EAST), Some("aws-us-gov"));
        assert_eq!(govcloud_partition(US_GOV_WEST), Some("aws-us-gov"));
        assert_eq!(govcloud_partition("us-east-1"), None);
    }

    #[test]
    fn prohibited_govcloud_channel_record_shape() {
        let record = govcloud_channel_leak_record("legacy_toolkit");

        assert_eq!(record.name, "kiro_cli_prohibited_telemetry_channel_enabled_total");
        assert_eq!(record.value, kiro_telemetry::MetricValue::Counter(1));
        assert!(
            record
                .attributes
                .iter()
                .any(|attribute| { attribute.key == "telemetry_channel" && attribute.value == "legacy_toolkit" })
        );
        assert!(
            record.attributes.iter().all(|attribute| attribute.key != "partition"),
            "prohibited-channel metrics must not create a partition dimension"
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
        let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, false)
            .await
            .unwrap();
        let clone = thread.clone();

        assert_eq!(clone.client_id(), thread.client_id());
        clone.finish().await.unwrap();
        thread.finish().await.unwrap();
    }

    #[tokio::test]
    async fn telemetry_thread_finish_returns_after_timeout_when_worker_is_stuck() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let thread = TelemetryThread {
            enabled: false,
            runtime: Arc::new(Mutex::new(TelemetryRuntime {
                otel_handle: Some(tokio::spawn(async {
                    std::future::pending::<()>().await;
                })),
                legacy_handle: None,
                otel_providers: None,
                tx: Some(tx),
            })),
            client_id: uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
            process_identity: Arc::new(Mutex::new(None)),
            run_receipt_store: None,
            run_receipt: Arc::new(Mutex::new(None)),
            startup_state: Arc::new(AtomicU8::new(STARTUP_PENDING)),
        };

        thread.finish_with_timeout(Duration::from_millis(1)).await.unwrap();
    }

    #[test]
    fn startup_terminal_events_are_mutually_exclusive() {
        fn thread() -> (TelemetryThread, mpsc::UnboundedReceiver<Event>) {
            let (tx, rx) = mpsc::unbounded_channel();
            (
                TelemetryThread {
                    enabled: true,
                    runtime: Arc::new(Mutex::new(TelemetryRuntime {
                        otel_handle: None,
                        legacy_handle: None,
                        otel_providers: None,
                        tx: Some(tx),
                    })),
                    client_id: uuid!("ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e"),
                    process_identity: Arc::new(Mutex::new(None)),
                    run_receipt_store: None,
                    run_receipt: Arc::new(Mutex::new(None)),
                    startup_state: Arc::new(AtomicU8::new(STARTUP_PENDING)),
                },
                rx,
            )
        }

        let (successful, mut successful_rx) = thread();
        successful
            .send_startup_duration(1.0, metric::SessionInterface::InteractiveCli, metric::Engine::V2)
            .unwrap();
        successful
            .send_startup_failure(
                metric::SessionInterface::InteractiveCli,
                metric::Engine::V2,
                metric::StartupFailureStage::InterfaceInit,
            )
            .unwrap();
        assert!(matches!(
            successful_rx.try_recv().unwrap().ty,
            EventType::StartupDuration { .. }
        ));
        assert!(successful_rx.try_recv().is_err());

        let (failed, mut failed_rx) = thread();
        failed
            .send_startup_failure(
                metric::SessionInterface::NoninteractiveCli,
                metric::Engine::V1,
                metric::StartupFailureStage::RuntimeSetup,
            )
            .unwrap();
        failed
            .send_startup_duration(1.0, metric::SessionInterface::NoninteractiveCli, metric::Engine::V1)
            .unwrap();
        assert!(matches!(
            failed_rx.try_recv().unwrap().ty,
            EventType::StartupFailure { .. }
        ));
        assert!(failed_rx.try_recv().is_err());
    }

    #[test]
    fn tool_use_event_normalizes_delegation_and_execution_context() {
        let mut event = tool_use_suggested_event(
            ToolUseEventBuilder::new(
                "conversation".to_string(),
                "tool-use".to_string(),
                Some("model".to_string()),
            )
            .set_tool_name("subagent".to_string()),
            metric::ExecutionContext::Subagent,
        );

        prepare_v1_event(&mut event);

        assert!(event.is_subagent);
        assert_eq!(
            event.metric_context.canonical_tool_name.as_deref(),
            Some("use_subagent")
        );

        let event = tool_use_suggested_event(
            ToolUseEventBuilder::new("conversation".to_string(), "tool-use".to_string(), None),
            metric::ExecutionContext::Main,
        );
        assert!(!event.is_subagent);
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_send() {
        let mut database = Database::new_default().await.unwrap();
        let env = Env::new();
        let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, false)
            .await
            .unwrap();
        thread.send_user_logged_in(&database).await.ok();
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
        let thread = TelemetryThread::new(&env, &Fs::new(), &mut database, None, false)
            .await
            .unwrap();

        thread.send_user_logged_in(&database).await.ok();
        thread
            .send_cli_subcommand_executed(&database, "version".to_string())
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
        let client = TelemetryClient::new(&Env::new(), &Fs::new(), &mut database, None, false)
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
