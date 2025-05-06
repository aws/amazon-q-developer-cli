pub mod cognito;
pub mod core;
pub mod definitions;
pub mod endpoint;
mod install_method;
mod util;

use std::sync::LazyLock;

use amzn_codewhisperer_client::types::{
    ChatAddMessageEvent,
    IdeCategory,
    OperatingSystem,
    OptOutPreference,
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
use tokio::sync::{
    Mutex,
    OnceCell,
};
use tokio::task::JoinSet;
use tracing::{
    debug,
    error,
};
use util::telemetry_is_disabled;
use uuid::Uuid;

use crate::api_client::Client as CodewhispererClient;
use crate::fig_aws_common::app_name;
use crate::util::system_info::os_version;
use crate::telemetry::core::Event;
pub use crate::telemetry::core::{
    EventType,
    QProfileSwitchIntent,
    TelemetryResult,
};

#[derive(thiserror::Error, Debug)]
pub enum TelemetryError {
    #[error(transparent)]
    ClientError(#[from] amzn_toolkit_telemetry_client::operation::post_metrics::PostMetricsError),
}

const PRODUCT: &str = "CodeWhisperer";
const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

// TODO: DO NOT USE OUTSIDE THIS FILE. Currently being used in one other place as part of a rewrite.
// but.
pub async fn client() -> &'static Client {
    static CLIENT: OnceCell<Client> = OnceCell::const_new();
    CLIENT
        .get_or_init(|| async { Client::new(TelemetryStage::EXTERNAL_PROD).await })
        .await
}

/// A IDE toolkit telemetry stage
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TelemetryStage {
    pub endpoint: &'static str,
    pub cognito_pool_id: &'static str,
    pub region: Region,
}

impl TelemetryStage {
    #[allow(dead_code)]
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

static JOIN_SET: LazyLock<Mutex<JoinSet<()>>> = LazyLock::new(|| Mutex::new(JoinSet::new()));

/// Joins all current telemetry events
pub async fn finish_telemetry() {
    let mut set = JOIN_SET.lock().await;
    while let Some(res) = set.join_next().await {
        if let Err(err) = res {
            error!(%err, "Failed to join telemetry event");
        }
    }
}

/// Joins all current telemetry events and panics if any fail to join
#[cfg(test)]
pub async fn finish_telemetry_unwrap() {
    let mut set = JOIN_SET.lock().await;
    while let Some(res) = set.join_next().await {
        res.unwrap();
    }
}

fn opt_out_preference() -> OptOutPreference {
    if telemetry_is_disabled() {
        OptOutPreference::OptOut
    } else {
        OptOutPreference::OptIn
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    client_id: Uuid,
    toolkit_telemetry_client: Option<ToolkitTelemetryClient>,
    codewhisperer_client: Option<CodewhispererClient>,
}

impl Client {
    pub async fn new(telemetry_stage: TelemetryStage) -> Self {
        let client_id = util::get_client_id();

        if cfg!(test) {
            return Self {
                client_id,
                toolkit_telemetry_client: None,
                codewhisperer_client: CodewhispererClient::new().await.ok(),
            };
        }

        let toolkit_telemetry_client = Some(amzn_toolkit_telemetry_client::Client::from_conf(
            Config::builder()
                .http_client(crate::fig_aws_common::http_client::client())
                .behavior_version(BehaviorVersion::v2025_01_17())
                .endpoint_resolver(StaticEndpoint(telemetry_stage.endpoint))
                .app_name(app_name())
                .region(telemetry_stage.region.clone())
                .credentials_provider(SharedCredentialsProvider::new(CognitoProvider::new(telemetry_stage)))
                .build(),
        ));
        let codewhisperer_client = CodewhispererClient::new().await.ok();

        Self {
            client_id,
            toolkit_telemetry_client,
            codewhisperer_client,
        }
    }

    /// TODO: DO NOT USE OUTSIDE THIS FILE
    pub async fn send_event(&self, event: Event) {
        if telemetry_is_disabled() {
            return;
        }

        self.send_cw_telemetry_event(&event).await;
        self.send_telemetry_toolkit_metric(event).await;
    }

    async fn send_telemetry_toolkit_metric(&self, event: Event) {
        let Some(toolkit_telemetry_client) = self.toolkit_telemetry_client.clone() else {
            return;
        };
        let client_id = self.client_id;
        let Some(metric_datum) = event.into_metric_datum() else {
            return;
        };

        let mut set = JOIN_SET.lock().await;
        set.spawn({
            async move {
                let product = AwsProduct::CodewhispererTerminal;
                let product_version = env!("CARGO_PKG_VERSION");
                let os = std::env::consts::OS;
                let os_architecture = std::env::consts::ARCH;
                let os_version = os_version().map(|v| v.to_string()).unwrap_or_default();
                let metric_name = metric_datum.metric_name().to_owned();

                debug!(?product, ?metric_datum, "Posting metrics");
                if let Err(err) = toolkit_telemetry_client
                    .post_metrics()
                    .aws_product(product)
                    .aws_product_version(product_version)
                    .client_id(client_id)
                    .os(os)
                    .os_architecture(os_architecture)
                    .os_version(os_version)
                    .metric_data(metric_datum)
                    .send()
                    .await
                    .map_err(DisplayErrorContext)
                {
                    error!(%err, ?metric_name, "Failed to post metric");
                }
            }
        });
    }

    async fn send_cw_telemetry_event(&self, event: &Event) {
        if let EventType::ChatAddedMessage {
            conversation_id,
            message_id,
            ..
        } = &event.ty
        {
            self.send_cw_telemetry_chat_add_message_event(conversation_id.clone(), message_id.clone())
                .await;
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

    async fn send_cw_telemetry_chat_add_message_event(&self, conversation_id: String, message_id: String) {
        let Some(codewhisperer_client) = self.codewhisperer_client.clone() else {
            return;
        };
        let user_context = self.user_context().unwrap();
        let opt_out_preference = opt_out_preference();

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

        let mut set = JOIN_SET.lock().await;
        set.spawn(async move {
            if let Err(err) = codewhisperer_client
                .send_telemetry_event(
                    TelemetryEvent::ChatAddMessageEvent(chat_add_message_event),
                    user_context,
                    opt_out_preference,
                )
                .await
            {
                error!(err =% DisplayErrorContext(err), "Failed to send telemetry event");
            }
        });
    }
}

pub async fn send_user_logged_in() {
    client().await.send_event(Event::new(EventType::UserLoggedIn {})).await;
}

pub async fn send_refresh_credentials(credential_start_url: String, request_id: String, oauth_flow: String) {
    client()
        .await
        .send_event(
            Event::new(EventType::RefreshCredentials {
                request_id,
                result: TelemetryResult::Succeeded,
                reason: None,
                oauth_flow,
            })
            .with_credential_start_url(credential_start_url),
        )
        .await;
}

pub async fn send_cli_subcommand_executed(subcommand: impl Into<String>) {
    client()
        .await
        .send_event(Event::new(EventType::CliSubcommandExecuted {
            subcommand: subcommand.into(),
        }))
        .await;
}

pub async fn send_chat_added_message(conversation_id: String, message_id: String, context_file_length: Option<usize>) {
    client()
        .await
        .send_event(Event::new(EventType::ChatAddedMessage {
            conversation_id,
            message_id,
            context_file_length,
        }))
        .await;
}

pub async fn send_mcp_server_init(
    conversation_id: String,
    init_failure_reason: Option<String>,
    number_of_tools: usize,
) {
    client()
        .await
        .send_event(Event::new(crate::telemetry::EventType::McpServerInit {
            conversation_id,
            init_failure_reason,
            number_of_tools,
        }))
        .await;
}

pub async fn send_did_select_profile(
    source: QProfileSwitchIntent,
    amazonq_profile_region: String,
    result: TelemetryResult,
    sso_region: Option<String>,
    profile_count: Option<i64>,
) {
    client()
        .await
        .send_event(Event::new(EventType::DidSelectProfile {
            source,
            amazonq_profile_region,
            result,
            sso_region,
            profile_count,
        }))
        .await;
}

pub async fn send_profile_state(
    source: QProfileSwitchIntent,
    amazonq_profile_region: String,
    result: TelemetryResult,
    sso_region: Option<String>,
) {
    client()
        .await
        .send_event(Event::new(EventType::ProfileState {
            source,
            amazonq_profile_region,
            result,
            sso_region,
        }))
        .await;
}

#[cfg(test)]
mod test {
    use uuid::uuid;

    use super::*;

    #[tokio::test]
    async fn client_context() {
        let client = client().await;
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
        finish_telemetry_unwrap().await;

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
        send_user_logged_in().await;
        send_cli_subcommand_executed("doctor").await;
        send_chat_added_message("debug".to_owned(), "debug".to_owned(), Some(123)).await;

        finish_telemetry_unwrap().await;

        assert!(!logs_contain("ERROR"));
        assert!(!logs_contain("error"));
        assert!(!logs_contain("WARN"));
        assert!(!logs_contain("warn"));
        assert!(!logs_contain("Failed to post metric"));
    }

    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_without_optout() {
        let client = Client::new(TelemetryStage::BETA).await;
        client
            .codewhisperer_client
            .as_ref()
            .unwrap()
            .send_telemetry_event(
                TelemetryEvent::ChatAddMessageEvent(
                    ChatAddMessageEvent::builder()
                        .conversation_id("debug".to_owned())
                        .message_id("debug".to_owned())
                        .build()
                        .unwrap(),
                ),
                client.user_context().unwrap(),
                OptOutPreference::OptIn,
            )
            .await
            .unwrap();
    }
}
