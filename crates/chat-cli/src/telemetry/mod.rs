pub mod cognito;
pub mod core;
pub mod definitions;
pub mod endpoint;
mod install_method;

use core::ToolUseEventBuilder;
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
use cognito::{
    CognitoProvider,
    get_cognito_credentials,
};
use endpoint::StaticEndpoint;
pub use install_method::{
    InstallMethod,
    get_install_method,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{
    debug,
    error,
    trace,
};
use uuid::{
    Uuid,
    uuid,
};

use crate::api_client::Client as CodewhispererClient;
use crate::aws_common::app_name;
use crate::cli::CliRootCommands;
use crate::database::Database;
use crate::database::settings::Setting;
use crate::database::state::StateDatabase;
use crate::platform::Env;
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
    ClientError(#[from] amzn_toolkit_telemetry_client::operation::post_metrics::PostMetricsError),
    #[error(transparent)]
    SendError(#[from] mpsc::error::SendError<Event>),
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
pub struct TelemetryThread {
    handle: Option<JoinHandle<()>>,
    tx: mpsc::UnboundedSender<Event>,
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
    pub async fn new(env: &Env, database: &mut Database) -> Self {
        let telemetry_client = TelemetryClient::new(env, database).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                trace!("Sending telemetry event: {:?}", event);
                telemetry_client.send_event(event).await;
            }
        });

        Self {
            handle: Some(handle),
            tx,
        }
    }

    pub fn send_user_logged_in(&self) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::UserLoggedIn {}))?)
    }

    pub fn send_refresh_credentials(
        &self,
        credential_start_url: String,
        request_id: String,
        oauth_flow: String,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(
            Event::new(EventType::RefreshCredentials {
                request_id,
                result: TelemetryResult::Succeeded,
                reason: None,
                oauth_flow,
            })
            .with_credential_start_url(credential_start_url),
        )?)
    }

    pub fn send_cli_subcommand_executed(&self, subcommand: Option<&CliRootCommands>) -> Result<(), TelemetryError> {
        let Some(subcommand) = subcommand else {
            return Ok(());
        };

        Ok(self.tx.send(Event::new(EventType::CliSubcommandExecuted {
            subcommand: subcommand.name().to_string(),
        }))?)
    }

    pub fn send_chat_added_message(
        &self,
        conversation_id: String,
        message_id: String,
        context_file_length: Option<usize>,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ChatAddedMessage {
            conversation_id,
            message_id,
            context_file_length,
        }))?)
    }

    pub fn send_tool_use_suggested(&self, event: ToolUseEventBuilder) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(EventType::ToolUseSuggested {
            conversation_id: event.conversation_id,
            utterance_id: event.utterance_id,
            user_input_id: event.user_input_id,
            tool_use_id: event.tool_use_id,
            tool_name: event.tool_name,
            is_accepted: event.is_accepted,
            is_success: event.is_success,
            is_valid: event.is_valid,
            is_custom_tool: event.is_custom_tool,
            input_token_size: event.input_token_size,
            output_token_size: event.output_token_size,
            custom_tool_call_latency: event.custom_tool_call_latency,
        }))?)
    }

    pub fn send_mcp_server_init(
        &self,
        conversation_id: String,
        init_failure_reason: Option<String>,
        number_of_tools: usize,
    ) -> Result<(), TelemetryError> {
        Ok(self.tx.send(Event::new(crate::telemetry::EventType::McpServerInit {
            conversation_id,
            init_failure_reason,
            number_of_tools,
        }))?)
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
}

#[derive(Debug, Clone)]
struct TelemetryClient {
    client_id: Uuid,
    telemetry_enabled: bool,
    codewhisperer_client: CodewhispererClient,
    toolkit_telemetry_client: Option<ToolkitTelemetryClient>,
}

impl TelemetryClient {
    async fn new(env: &Env, database: &mut Database) -> Self {
        let telemetry_enabled = !cfg!(test)
            && !env.get_os("Q_DISABLE_TELEMETRY").is_some()
            && database.settings.get_bool(Setting::TelemetryEnabled).unwrap_or(true);

        // If telemetry is disabled we do not emit using toolkit_telemetry
        let toolkit_telemetry_client = match telemetry_enabled {
            true => match get_cognito_credentials(database, &TelemetryStage::EXTERNAL_PROD).await {
                Ok(credentials) => Some(ToolkitTelemetryClient::from_conf(
                    Config::builder()
                        .http_client(crate::aws_common::http_client::client(database))
                        .behavior_version(BehaviorVersion::v2025_01_17())
                        .endpoint_resolver(StaticEndpoint(TelemetryStage::EXTERNAL_PROD.endpoint))
                        .app_name(app_name())
                        .region(TelemetryStage::EXTERNAL_PROD.region.clone())
                        .credentials_provider(SharedCredentialsProvider::new(CognitoProvider::new(credentials)))
                        .build(),
                )),
                Err(err) => {
                    error!("Failed to acquire cognito credentials: {err}");
                    None
                },
            },
            false => None,
        };

        fn client_id(env: &Env, database: &mut Database, telemetry_enabled: bool) -> Uuid {
            if !telemetry_enabled {
                return uuid!("ffffffff-ffff-ffff-ffff-ffffffffffff");
            }

            if let Ok(client_id) = env.get(CLIENT_ID_ENV_VAR) {
                if let Ok(uuid) = Uuid::from_str(&client_id) {
                    return uuid;
                }
            }

            match database.get_client_id() {
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
            }
        }

        Self {
            client_id: client_id(env, database, telemetry_enabled),
            telemetry_enabled,
            toolkit_telemetry_client,
            codewhisperer_client: CodewhispererClient::new(database, None).await,
        }
    }

    async fn send_event(&self, event: Event) {
        // This client will exist when telemetry is disabled.
        self.send_cw_telemetry_event(&event).await;

        // This client won't exist when telemetry is disabled.
        self.send_telemetry_toolkit_metric(event).await;
    }

    async fn send_cw_telemetry_event(&self, event: &Event) {
        if let EventType::ChatAddedMessage {
            conversation_id,
            message_id,
            ..
        } = &event.ty
        {
            let user_context = self.user_context().unwrap();

            let chat_add_message_event = match ChatAddMessageEvent::builder()
                .conversation_id(conversation_id)
                .message_id(message_id)
                .build()
            {
                Ok(event) => event,
                Err(err) => {
                    error!(err =% DisplayErrorContext(err), "Failed to send telemetry event");
                    return;
                },
            };

            if let Err(err) = self
                .codewhisperer_client
                .send_telemetry_event(
                    TelemetryEvent::ChatAddMessageEvent(chat_add_message_event),
                    user_context,
                    self.telemetry_enabled,
                )
                .await
            {
                error!(err =% DisplayErrorContext(err), "Failed to send telemetry event");
            }
        }
    }

    async fn send_telemetry_toolkit_metric(&self, event: Event) {
        let Some(toolkit_telemetry_client) = self.toolkit_telemetry_client.clone() else {
            return;
        };
        let client_id = self.client_id;
        let Some(metric_datum) = event.into_metric_datum() else {
            return;
        };

        let product = AwsProduct::CodewhispererTerminal;
        let metric_name = metric_datum.metric_name().to_owned();

        debug!(?product, ?metric_datum, "Posting metrics");
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
            error!(%err, ?metric_name, "Failed to post metric");
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

#[cfg(test)]
mod test {
    use uuid::uuid;

    use super::*;

    #[tokio::test]
    async fn client_context() {
        let mut database = Database::new().await.unwrap();
        let client = TelemetryClient::new(&Env::new(), &mut database).await;
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
        let thread = TelemetryThread::new(&Env::new(), &mut database).await;
        thread.send_user_logged_in();

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
        let thread = TelemetryThread::new(&Env::new(), &mut database).await;

        thread.send_user_logged_in();
        thread.send_cli_subcommand_executed(Some(&CliRootCommands::Version { changelog: None }));
        thread.send_chat_added_message("version".to_owned(), "version".to_owned(), Some(123));

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
        let client = TelemetryClient::new(&Env::new(), &mut database).await;
        client
            .codewhisperer_client
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
            )
            .await
            .unwrap();
    }
}
