pub mod cognito;
mod dispatch;
pub mod endpoint;
mod event;
mod install_method;
mod util;

use std::any::Any;
use std::sync::LazyLock;
use std::time::{
    Duration,
    SystemTime,
};

use amzn_codewhisperer_client::types::{
    ChatAddMessageEvent,
    CompletionType,
    IdeCategory,
    OperatingSystem,
    OptOutPreference,
    ProgrammingLanguage,
    TelemetryEvent,
    TerminalUserInteractionEvent,
    TerminalUserInteractionEventType,
    UserContext,
    UserTriggerDecisionEvent,
};
use amzn_toolkit_telemetry::config::{
    BehaviorVersion,
    Region,
};
use amzn_toolkit_telemetry::error::DisplayErrorContext;
use amzn_toolkit_telemetry::types::AwsProduct;
use amzn_toolkit_telemetry::{
    Client as ToolkitTelemetryClient,
    Config,
};
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_smithy_types::DateTime;
use cognito::CognitoProvider;
use dispatch::dispatch;
pub use dispatch::{
    DispatchMode,
    dispatch_mode,
    set_dispatch_mode,
};
use endpoint::StaticEndpoint;
pub use event::{
    AppTelemetryEvent,
    InlineShellCompletionActionedOptions,
};
use fig_api_client::Client as CodewhispererClient;
use fig_aws_common::app_name;
use fig_settings::State;
use fig_telemetry_core::{
    Event,
    TelemetryEmitter,
};
pub use fig_telemetry_core::{
    EventType,
    SuggestionState,
};
use fig_util::Shell;
use fig_util::system_info::os_version;
use fig_util::terminal::{
    current_terminal,
    current_terminal_version,
};
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
use util::{
    old_client_id,
    telemetry_is_disabled,
};
use uuid::Uuid;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Telemetry is disabled")]
    TelemetryDisabled,
    #[error(transparent)]
    ClientError(#[from] amzn_toolkit_telemetry::operation::post_metrics::PostMetricsError),
}

const PRODUCT: &str = "CodeWhisperer";
const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

async fn client() -> &'static Client {
    static CLIENT: OnceCell<Client> = OnceCell::const_new();
    CLIENT
        .get_or_init(|| async { Client::new(TelemetryStage::EXTERNAL_PROD).await })
        .await
}

/// A telemetry emitter that first tries sending the event to figterm so that the CLI commands can
/// execute much quicker. Only falls back to sending it directly on the current task if sending to
/// figterm fails.
struct DispatchingTelemetryEmitter;

#[async_trait::async_trait]
impl TelemetryEmitter for DispatchingTelemetryEmitter {
    async fn send(&self, event: fig_telemetry_core::Event) {
        let event = AppTelemetryEvent::from_event(event).await;
        dispatch_or_send_event(event).await;
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn init_global_telemetry_emitter() {
    fig_telemetry_core::init_global_telemetry_emitter(DispatchingTelemetryEmitter {});
}

/// A IDE toolkit telemetry stage
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TelemetryStage {
    pub name: &'static str,
    pub endpoint: &'static str,
    pub cognito_pool_id: &'static str,
    pub region: Region,
}

impl TelemetryStage {
    #[allow(dead_code)]
    const BETA: Self = Self::new(
        "beta",
        "https://7zftft3lj2.execute-api.us-east-1.amazonaws.com/Beta",
        "us-east-1:db7bfc9f-8ecd-4fbb-bea7-280c16069a99",
        "us-east-1",
    );
    const EXTERNAL_PROD: Self = Self::new(
        "prod",
        "https://client-telemetry.us-east-1.amazonaws.com",
        "us-east-1:820fd6d1-95c0-4ca4-bffb-3f01d32da842",
        "us-east-1",
    );

    const fn new(
        name: &'static str,
        endpoint: &'static str,
        cognito_pool_id: &'static str,
        region: &'static str,
    ) -> Self {
        Self {
            name,
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
    state: State,
}

impl Client {
    pub async fn new(telemetry_stage: TelemetryStage) -> Self {
        let client_id = util::get_client_id();
        let toolkit_telemetry_client = Some(amzn_toolkit_telemetry::Client::from_conf(
            Config::builder()
                .behavior_version(BehaviorVersion::v2024_03_28())
                .endpoint_resolver(StaticEndpoint(telemetry_stage.endpoint))
                .app_name(app_name())
                .region(telemetry_stage.region.clone())
                .credentials_provider(SharedCredentialsProvider::new(CognitoProvider::new(telemetry_stage)))
                .build(),
        ));
        let codewhisperer_client = CodewhispererClient::new().await.ok();
        let state = State::new();

        Self {
            client_id,
            toolkit_telemetry_client,
            codewhisperer_client,
            state,
        }
    }

    pub fn mock() -> Self {
        let client_id = util::get_client_id();
        let toolkit_telemetry_client = None;
        let codewhisperer_client = Some(CodewhispererClient::mock());
        let state = State::new_fake();

        Self {
            client_id,
            toolkit_telemetry_client,
            codewhisperer_client,
            state,
        }
    }

    async fn send_event(&self, event: AppTelemetryEvent) {
        self.send_migrate().await;
        self.send_cw_telemetry_event(&event).await;
        self.send_telemetry_toolkit_metric(event).await;
    }

    async fn send_migrate(&self) {
        // If we have not sent the migrate event, send this event
        match self.state.atomic_bool_or("telemetry.sentMigrateClientIdEvent", true) {
            Ok(true) => {},
            Ok(false) => {
                if let Some(old_client_id) = old_client_id() {
                    let event = AppTelemetryEvent::from_event(Event::new(EventType::MigrateClientId {
                        old_client_id: old_client_id.into(),
                    }))
                    .await;
                    self.send_telemetry_toolkit_metric(event).await;
                }
            },
            Err(err) => error!(
                %err,
                "Failed to atomic_bool_or telemetry.sentMigrateClientIdEvent, skipping migrate event"
            ),
        }
    }

    async fn send_telemetry_toolkit_metric(&self, event: AppTelemetryEvent) {
        if telemetry_is_disabled() {
            return;
        }
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

    async fn send_cw_telemetry_event(&self, event: &AppTelemetryEvent) {
        match &event.ty {
            EventType::TranslationActioned {
                latency,
                suggestion_state,
                terminal,
                terminal_version,
                shell,
                shell_version,
            } => {
                self.send_cw_telemetry_translation_action(
                    *latency,
                    *suggestion_state,
                    terminal.clone(),
                    terminal_version.clone(),
                    shell.clone(),
                    shell_version.clone(),
                )
                .await;
            },
            EventType::CompletionInserted {
                command,
                terminal,
                shell,
            } => {
                self.send_cw_telemetry_completion_inserted(command.clone(), terminal.clone(), shell.clone())
                    .await;
            },
            EventType::ChatAddedMessage {
                conversation_id,
                message_id,
                ..
            } => {
                self.send_cw_telemetry_chat_add_message_event(conversation_id.clone(), message_id.clone())
                    .await;
            },
            EventType::InlineShellCompletionActioned {
                session_id,
                request_id,
                latency,
                suggestion_state,
                suggested_chars_len,
                number_of_recommendations,
                ..
            } => {
                self.send_cw_telemetry_user_trigger_decision_event(
                    session_id.clone(),
                    request_id.clone(),
                    *latency,
                    suggestion_state.is_accepted(),
                    *suggested_chars_len,
                    *number_of_recommendations,
                )
                .await;
            },
            _ => {},
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

    async fn send_cw_telemetry_translation_action(
        &self,
        latency: Duration,
        suggestion_state: SuggestionState,
        terminal: Option<String>,
        terminal_version: Option<String>,
        shell: Option<String>,
        shell_version: Option<String>,
    ) {
        let Some(codewhisperer_client) = self.codewhisperer_client.clone() else {
            return;
        };
        let user_context = self.user_context().unwrap();
        let opt_out_preference = opt_out_preference();

        let mut set = JOIN_SET.lock().await;
        set.spawn(async move {
            let mut terminal_user_interaction_event_builder = TerminalUserInteractionEvent::builder()
                .terminal_user_interaction_event_type(
                    TerminalUserInteractionEventType::CodewhispererTerminalTranslationAction,
                )
                .time_to_suggestion(latency.as_millis() as i32)
                .is_completion_accepted(suggestion_state == SuggestionState::Accept);

            if let Some(terminal) = terminal {
                terminal_user_interaction_event_builder = terminal_user_interaction_event_builder.terminal(terminal);
            }

            if let Some(terminal_version) = terminal_version {
                terminal_user_interaction_event_builder =
                    terminal_user_interaction_event_builder.terminal_version(terminal_version);
            }

            if let Some(shell) = shell {
                terminal_user_interaction_event_builder = terminal_user_interaction_event_builder.shell(shell);
            }

            if let Some(shell_version) = shell_version {
                terminal_user_interaction_event_builder =
                    terminal_user_interaction_event_builder.shell_version(shell_version);
            }

            let terminal_user_interaction_event = terminal_user_interaction_event_builder.build();

            if let Err(err) = codewhisperer_client
                .send_telemetry_event(
                    TelemetryEvent::TerminalUserInteractionEvent(terminal_user_interaction_event),
                    user_context,
                    opt_out_preference,
                )
                .await
            {
                error!(err =% DisplayErrorContext(err), "Failed to send telemetry event");
            }
        });
    }

    async fn send_cw_telemetry_completion_inserted(
        &self,
        command: String,
        terminal: Option<String>,
        shell: Option<String>,
    ) {
        let Some(codewhisperer_client) = self.codewhisperer_client.clone() else {
            return;
        };
        let user_context = self.user_context().unwrap();
        let opt_out_preference = opt_out_preference();

        let mut set = JOIN_SET.lock().await;
        set.spawn(async move {
            let mut terminal_user_interaction_event_builder = TerminalUserInteractionEvent::builder()
                .terminal_user_interaction_event_type(
                    TerminalUserInteractionEventType::CodewhispererTerminalCompletionInserted,
                )
                .cli_tool_command(command);

            if let Some(terminal) = terminal {
                terminal_user_interaction_event_builder = terminal_user_interaction_event_builder.terminal(terminal);
            }

            if let Some(shell) = shell {
                terminal_user_interaction_event_builder = terminal_user_interaction_event_builder.shell(shell);
            }

            let terminal_user_interaction_event = terminal_user_interaction_event_builder.build();

            if let Err(err) = codewhisperer_client
                .send_telemetry_event(
                    TelemetryEvent::TerminalUserInteractionEvent(terminal_user_interaction_event),
                    user_context,
                    opt_out_preference,
                )
                .await
            {
                error!(err =% DisplayErrorContext(err), "Failed to send telemetry event");
            }
        });
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

    /// This is the user decision to accept a suggestion for inline suggestions
    async fn send_cw_telemetry_user_trigger_decision_event(
        &self,
        session_id: String,
        request_id: String,
        latency: Duration,
        accepted: bool,
        suggested_chars_len: i32,
        number_of_recommendations: i32,
    ) {
        let Some(codewhisperer_client) = self.codewhisperer_client.clone() else {
            return;
        };
        let user_context = self.user_context().unwrap();
        let opt_out_preference = opt_out_preference();

        let programming_language = match ProgrammingLanguage::builder().language_name("shell").build() {
            Ok(language) => language,
            Err(err) => {
                error!(err =% DisplayErrorContext(err), "Failed to build programming language");
                return;
            },
        };

        let suggestion_state = if accepted {
            SuggestionState::Accept
        } else {
            SuggestionState::Reject
        };

        let user_trigger_decision_event = match UserTriggerDecisionEvent::builder()
            .session_id(session_id)
            .request_id(request_id)
            .programming_language(programming_language)
            .completion_type(CompletionType::Line)
            .suggestion_state(suggestion_state.into())
            .accepted_character_count(if accepted { suggested_chars_len } else { 0 })
            .number_of_recommendations(number_of_recommendations)
            .generated_line(1)
            .recommendation_latency_milliseconds(latency.as_secs_f64() * 1000.0)
            .timestamp(DateTime::from(SystemTime::now()))
            .build()
        {
            Ok(event) => event,
            Err(err) => {
                error!(err =% DisplayErrorContext(err), "Failed to build user trigger decision event");
                return;
            },
        };

        let mut set = JOIN_SET.lock().await;
        set.spawn(async move {
            if let Err(err) = codewhisperer_client
                .send_telemetry_event(
                    TelemetryEvent::UserTriggerDecisionEvent(user_trigger_decision_event),
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

pub async fn send_event(event: AppTelemetryEvent) {
    client().await.send_event(event).await;
}

pub async fn dispatch_or_send_event(event: AppTelemetryEvent) {
    debug!(?event, "Dispatching telemetry event");
    if dispatch(&event).await.should_fallback() {
        debug!(?event, "Dispatch failed, falling back to send_event");
        send_event(event).await;
    }
}

pub async fn send_user_logged_in() {
    let event = AppTelemetryEvent::new(EventType::UserLoggedIn {}).await;
    dispatch_or_send_event(event).await;
}

pub async fn send_completion_inserted(command: String, terminal: Option<String>, shell: Option<String>) {
    let event = AppTelemetryEvent::new(EventType::CompletionInserted {
        command,
        terminal,
        shell,
    })
    .await;
    dispatch_or_send_event(event).await;
}

pub async fn send_translation_actioned(latency: Duration, suggestion_state: SuggestionState) {
    let (shell, shell_version) = shell().await;
    let event = AppTelemetryEvent::new(EventType::TranslationActioned {
        latency,
        suggestion_state,
        terminal: current_terminal().map(|t| t.internal_id().to_string()),
        terminal_version: current_terminal_version().map(Into::into),
        shell: shell.map(|s| s.to_string()),
        shell_version,
    })
    .await;
    dispatch_or_send_event(event).await;
}

pub async fn send_cli_subcommand_executed(subcommand: impl Into<String>) {
    let (shell, shell_version) = shell().await;
    let event = AppTelemetryEvent::new(EventType::CliSubcommandExecuted {
        subcommand: subcommand.into(),
        terminal: current_terminal().map(|t| t.internal_id().to_string()),
        terminal_version: current_terminal_version().map(Into::into),
        shell: shell.map(|s| s.to_string()),
        shell_version,
    })
    .await;
    dispatch_or_send_event(event).await;
}

pub async fn send_doctor_check_failed(failed_check: impl Into<String>) {
    let (shell, shell_version) = shell().await;
    let event = AppTelemetryEvent::new(EventType::DoctorCheckFailed {
        doctor_check: failed_check.into(),
        terminal: current_terminal().map(|t| t.internal_id().to_string()),
        terminal_version: current_terminal_version().map(Into::into),
        shell: shell.map(|s| s.to_string()),
        shell_version,
    })
    .await;
    dispatch_or_send_event(event).await;
}

pub async fn send_dashboard_page_viewed(route: impl Into<String>) {
    let event = AppTelemetryEvent::new(EventType::DashboardPageViewed { route: route.into() }).await;
    dispatch_or_send_event(event).await;
}

pub async fn send_menu_bar_actioned(menu_bar_item: Option<impl Into<String>>) {
    let event = AppTelemetryEvent::new(EventType::MenuBarActioned {
        menu_bar_item: menu_bar_item.map(|i| i.into()),
    })
    .await;
    dispatch_or_send_event(event).await;
}

pub async fn send_fig_user_migrated() {
    let event = AppTelemetryEvent::new(EventType::FigUserMigrated {}).await;
    dispatch_or_send_event(event).await;
}

pub async fn send_start_chat(conversation_id: String) {
    let event = AppTelemetryEvent::new(EventType::ChatStart { conversation_id }).await;
    dispatch_or_send_event(event).await;
}

pub async fn send_end_chat(conversation_id: String) {
    let event = AppTelemetryEvent::new(EventType::ChatEnd { conversation_id }).await;
    dispatch_or_send_event(event).await;
}

pub async fn send_chat_added_message(conversation_id: String, message_id: String, context_file_length: Option<usize>) {
    let event = AppTelemetryEvent::new(EventType::ChatAddedMessage {
        conversation_id,
        message_id,
        context_file_length,
    })
    .await;
    dispatch_or_send_event(event).await;
}

async fn shell() -> (Option<Shell>, Option<String>) {
    Shell::current_shell_version()
        .await
        .map(|(shell, shell_version)| (Some(shell), Some(shell_version)))
        .unwrap_or((None, None))
}

#[cfg(test)]
mod test {
    use event::tests::all_events;
    use fig_util::CLI_BINARY_NAME;
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

    #[tokio::test]
    async fn client_send_event_test() {
        let client = Client::mock();
        for event in all_events().await {
            client.send_event(event).await;
        }
    }

    #[tracing_test::traced_test]
    #[tokio::test]
    #[ignore = "needs auth which is not in CI"]
    async fn test_send() {
        // let (shell, shell_version) = Shell::current_shell_version()
        //     .await
        //     .map(|(shell, shell_version)| (Some(shell), Some(shell_version)))
        //     .unwrap_or((None, None));

        // let client = Client::new(TelemetryStage::BETA).await;

        // client
        //     .post_metric(metrics::CodewhispererterminalCliSubcommandExecuted {
        //         create_time: None,
        //         value: None,
        //         codewhispererterminal_subcommand: Some(CodewhispererterminalSubcommand("doctor".into())),
        //         codewhispererterminal_terminal: CURRENT_TERMINAL
        //             .clone()
        //             .map(|terminal| CodewhispererterminalTerminal(terminal.internal_id().to_string())),
        //         codewhispererterminal_terminal_version: CURRENT_TERMINAL_VERSION
        //             .clone()
        //             .map(CodewhispererterminalTerminalVersion),
        //         codewhispererterminal_shell: shell.map(|shell|
        // CodewhispererterminalShell(shell.to_string())),
        //         codewhispererterminal_shell_version:
        // shell_version.map(CodewhispererterminalShellVersion),         credential_start_url:
        // start_url().await,     })
        //     .await;

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
        send_completion_inserted(CLI_BINARY_NAME.to_owned(), None, None).await;
        send_translation_actioned(Duration::from_millis(10), SuggestionState::Accept).await;
        send_cli_subcommand_executed("doctor").await;
        send_doctor_check_failed("").await;
        send_dashboard_page_viewed("/").await;
        send_menu_bar_actioned(Some("Settings")).await;
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
