use std::str::FromStr;

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
use kiro_telemetry_host::{
    Event,
    EventType,
};
use kiro_telemetry_legacy::event_to_metric_datum;
use tracing::{
    debug,
    error,
    trace,
};
use uuid::{
    Uuid,
    uuid,
};

use super::TelemetryError;
use super::cognito::CognitoProvider;
use super::core::ChatAddedMessageParams;
use super::endpoint::StaticEndpoint;
use crate::api_client::ApiClient;
use crate::aws_common::app_name;
use crate::database::Database;
use crate::database::settings::Setting;

const KIRO_TELEMETRY_TOOLKIT_ENDPOINT: &str = "KIRO_TELEMETRY_TOOLKIT_ENDPOINT";
use crate::os::{
    Env,
    Fs,
};
use crate::util::system_info::os_version;

#[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
pub(super) const PRODUCT: &str = "CodeWhisperer";
#[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
pub(super) const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    pub(super) const BETA: Self = Self::new(
        "https://7zftft3lj2.execute-api.us-east-1.amazonaws.com/Beta",
        "us-east-1:db7bfc9f-8ecd-4fbb-bea7-280c16069a99",
        "us-east-1",
    );
    pub(super) const EXTERNAL_PROD: Self = Self::new(
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

pub(super) fn should_build_toolkit_telemetry_client(telemetry_enabled: bool, govcloud_partition: Option<&str>) -> bool {
    telemetry_enabled && govcloud_partition.is_none() && cfg!(feature = "legacy_toolkit_sink")
}

pub(super) fn should_build_codewhisperer_telemetry_client() -> bool {
    cfg!(feature = "legacy_codewhisperer_sink")
}

pub(super) fn resolve_client_id(
    env: &Env,
    database: &mut Database,
    telemetry_enabled: bool,
) -> Result<Uuid, TelemetryError> {
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

#[derive(Debug)]
pub(super) struct TelemetryClient {
    #[cfg_attr(not(feature = "legacy_toolkit_sink"), allow(dead_code))]
    client_id: Uuid,
    #[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
    telemetry_enabled: bool,
    #[cfg_attr(not(feature = "legacy_codewhisperer_sink"), allow(dead_code))]
    pub(super) codewhisperer_client: Option<ApiClient>,
    toolkit_telemetry_client: Option<ToolkitTelemetryClient>,
}

impl TelemetryClient {
    pub(super) async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        govcloud_partition: Option<&str>,
        client_id: Uuid,
        telemetry_enabled: bool,
    ) -> Result<Self, TelemetryError> {
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

        let codewhisperer_client = if should_build_codewhisperer_telemetry_client() {
            Some(ApiClient::new(env, fs, database, None).await?)
        } else {
            None
        };

        Ok(Self {
            client_id,
            telemetry_enabled,
            toolkit_telemetry_client,
            codewhisperer_client,
        })
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
    pub(super) fn user_context(&self) -> Option<UserContext> {
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

impl kiro_telemetry_host::LegacySink for TelemetryClient {
    fn send_event(&self, event: Event) -> futures::future::BoxFuture<'_, ()> {
        Box::pin(self.send_legacy_event(event, None))
    }

    fn send_event_govcloud(&self, event: Event, partition: &'static str) -> futures::future::BoxFuture<'_, ()> {
        Box::pin(self.send_legacy_event(event, Some(partition)))
    }
}
