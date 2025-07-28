pub mod cognito;
pub mod core;
pub mod definitions;
pub mod endpoint;
mod install_method;

use core::{
    AgentConfigInitArgs,
    ChatAddedMessageParams,
    RecordUserTurnCompletionArgs,
    ToolUseEventBuilder,
};
use std::str::FromStr;

use amzn_codewhisperer_client::types::{
    ChatAddMessageEvent,
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
pub use install_method::{
    InstallMethod,
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
    EventType,
    QProfileSwitchIntent,
    TelemetryResult,
};
use crate::util::system_info::os_version;

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
const CLIENT_ID_ENV_VAR: &str = "Q_TELEMETRY_CLIENT_ID";

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
    pub async fn new(env: &Env, fs: &Fs, database: &mut Database) -> Result<Self, TelemetryError> {
        let telemetry_client = TelemetryClient::new(env, fs, database).await?;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tx = TelemetrySender::Strong(tx);
        let handle = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                trace!("TelemetryThread received new telemetry event: {:?}", event);
                telemetry_client.send_event(event).await;
            }
        });

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

    pub fn send_cli_subcommand_executed(&self, subcommand: &RootSubcommand) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::CliSubcommandExecuted {
            subcommand: subcommand.to_string(),
        }))?)
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
        let mut event = Event::new(EventType::ChatSlashCommandExecuted {
            conversation_id,
            command,
            subcommand,
            result,
            reason,
        });
        set_start_url_and_region(database, &mut event).await;
        Ok(self.tx.send(event)?)
    }

    #[allow(clippy::too_many_arguments)] // TODO: Should make a parameters struct.
    pub async fn send_chat_added_message(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        data: ChatAddedMessageParams,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::ChatAddedMessage {
            conversation_id,
            result,
            data,
        });
        set_start_url_and_region(database, &mut event).await;

        Ok(self.tx.send(event)?)
    }

    pub async fn send_record_user_turn_completion(
        &self,
        database: &Database,
        conversation_id: String,
        result: TelemetryResult,
        args: RecordUserTurnCompletionArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(EventType::RecordUserTurnCompletion {
            conversation_id,
            result,
            args,
        });
        set_start_url_and_region(database, &mut event).await;
        Ok(self.tx.send(event)?)
    }

    pub fn send_tool_use_suggested(&self, event: ToolUseEventBuilder) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ToolUseSuggested {
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
        }))?)
    }

    pub async fn send_mcp_server_init(
        &self,
        database: &Database,
        conversation_id: String,
        server_name: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(crate::telemetry::EventType::McpServerInit {
            conversation_id,
            server_name,
            init_failure_reason,
            number_of_tools,
        });
        set_start_url_and_region(database, &mut event).await;
        Ok(self.tx.send(event)?)
    }

    pub async fn send_agent_config_init(
        &self,
        database: &Database,
        conversation_id: String,
        args: AgentConfigInitArgs,
    ) -> Result<(), TelemetryError> {
        let mut event = Event::new(crate::telemetry::EventType::AgentConfigInit { conversation_id, args });
        set_start_url_and_region(database, &mut event).await;
        Ok(self.tx.send(event)?)
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
        let mut event = Event::new(EventType::MessageResponseError {
            result,
            reason,
            reason_desc,
            status_code,
            conversation_id,
            context_file_length,
            request_id,
            message_id,
        });
        set_start_url_and_region(database, &mut event).await;

        Ok(self.tx.send(event)?)
    }
}

async fn set_start_url_and_region(database: &Database, event: &mut Event) {
    let (start_url, region) = get_start_url_and_region(database).await;
    if let Some(start_url) = start_url {
        event.set_start_url(start_url);
    }
    if let Some(region) = region {
        event.set_sso_region(region);
    }
}

#[derive(Debug)]
struct TelemetryClient {
    client_id: Uuid,
    telemetry_enabled: bool,
    codewhisperer_client: Option<ApiClient>,
    toolkit_telemetry_client: Option<ToolkitTelemetryClient>,
}

impl TelemetryClient {
    async fn new(env: &Env, fs: &Fs, database: &mut Database) -> Result<Self, TelemetryError> {
        let telemetry_enabled = !cfg!(test)
            && env.get_os("Q_DISABLE_TELEMETRY").is_none()
            && database.settings.get_bool(Setting::TelemetryEnabled).unwrap_or(true);

        // If telemetry is disabled we do not emit using toolkit_telemetry
        let toolkit_telemetry_client = if telemetry_enabled {
            Some(ToolkitTelemetryClient::from_conf(
                Config::builder()
                    .http_client(crate::aws_common::http_client::client())
                    .behavior_version(BehaviorVersion::v2025_01_17())
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

            if let Ok(client_id) = env.get(CLIENT_ID_ENV_VAR) {
                if let Ok(uuid) = Uuid::from_str(&client_id) {
                    return Ok(uuid);
                }
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
        let codewhisperer_client = if env.get("AMAZON_Q_SIGV4").is_ok() {
            None
        } else {
            Some(ApiClient::new(env, fs, database, None).await?)
        };

        Ok(Self {
            client_id: client_id(env, database, telemetry_enabled)?,
            telemetry_enabled,
            toolkit_telemetry_client,
            codewhisperer_client,
        })
    }

    /// Sends a telemetry event to both the CW and toolkit API's. If the clients do not exist, then
    /// telemetry is not sent.
    ///
    /// See [TelemetryClient::new] for which conditions the clients are created for.
    async fn send_event(&self, event: Event) {
        self.send_cw_telemetry_event(&event).await;
        self.send_telemetry_toolkit_metric(event).await;
    }

    async fn send_cw_telemetry_event(&self, event: &Event) {
        let Some(codewhisperer_client) = self.codewhisperer_client.clone() else {
            trace!("not sending cw metric - client does not exist");
            return;
        };

        if let EventType::ChatAddedMessage {
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
        } = &event.ty
        {
            let user_context = self.user_context().unwrap();

            let chat_add_message_event = match ChatAddMessageEvent::builder()
                .conversation_id(conversation_id)
                .message_id(message_id.clone().unwrap_or("not_set".to_string()))
                .set_time_to_first_chunk_milliseconds(*time_to_first_chunk_ms)
                .set_time_between_chunks(time_between_chunks_ms.clone())
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
        }
    }

    async fn send_telemetry_toolkit_metric(&self, event: Event) {
        let Some(toolkit_telemetry_client) = self.toolkit_telemetry_client.clone() else {
            trace!("not sending toolkit metric - client does not exist");
            return;
        };
        let client_id = self.client_id;
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
        let mut database = Database::new().await.unwrap();
        let client = TelemetryClient::new(&Env::new(), &Fs::new(), &mut database)
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
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_send() {
        let mut database = Database::new().await.unwrap();
        let thread = TelemetryThread::new(&Env::new(), &Fs::new(), &mut database)
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
        let thread = TelemetryThread::new(&Env::new(), &Fs::new(), &mut database)
            .await
            .unwrap();

        thread.send_user_logged_in().ok();
        thread
            .send_cli_subcommand_executed(&RootSubcommand::Version { changelog: None })
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
        let client = TelemetryClient::new(&Env::new(), &Fs::new(), &mut database)
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
