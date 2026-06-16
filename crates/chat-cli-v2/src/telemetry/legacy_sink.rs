//! V2-only legacy telemetry sink: posts events to the AWS Toolkit telemetry
//! endpoint and to CodeWhisperer.
//!
//! This module is the seam that lets the shared
//! [`kiro_telemetry_host::TelemetryThread`] stay free of AWS-SDK dependencies.
//! V3 / kiro-bot pass `legacy_sink: None`; V2 passes
//! `Some(Arc::new(V2LegacySink::build(..)?))`.

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
use futures::FutureExt;
use futures::future::BoxFuture;
use kiro_telemetry_host::config::LegacySink;
use kiro_telemetry_host::event::ChatAddedMessageParams;
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

use super::cognito::CognitoProvider;
use super::endpoint::StaticEndpoint;
use crate::api_client::{
    ApiClient,
    ApiClientError,
};
use crate::aws_common::app_name;
use crate::database::Database;
use crate::database::settings::Setting;
use crate::os::{
    Env,
    Fs,
};
use crate::util::system_info::os_version;

const PRODUCT: &str = "CodeWhisperer";
const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// IDE toolkit telemetry stage (endpoint + cognito + region triple).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TelemetryStage {
    pub endpoint: &'static str,
    pub cognito_pool_id: &'static str,
    pub region: Region,
}

impl TelemetryStage {
    #[cfg(test)]
    pub const BETA: Self = Self::new(
        "https://7zftft3lj2.execute-api.us-east-1.amazonaws.com/Beta",
        "us-east-1:db7bfc9f-8ecd-4fbb-bea7-280c16069a99",
        "us-east-1",
    );
    pub const EXTERNAL_PROD: Self = Self::new(
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

#[derive(Debug, thiserror::Error)]
pub enum LegacySinkError {
    #[error(transparent)]
    Database(#[from] crate::database::DatabaseError),
    #[error(transparent)]
    ApiClient(Box<ApiClientError>),
}

impl From<ApiClientError> for LegacySinkError {
    fn from(value: ApiClientError) -> Self {
        Self::ApiClient(Box::new(value))
    }
}

#[allow(dead_code)] // some fields used only behind cfg-feature gates
#[derive(Debug)]
pub struct V2LegacySink {
    client_id: Uuid,
    telemetry_enabled: bool,
    codewhisperer_client: Option<ApiClient>,
    toolkit_telemetry_client: Option<ToolkitTelemetryClient>,
}

fn should_build_toolkit_telemetry_client(telemetry_enabled: bool, govcloud_partition: Option<&str>) -> bool {
    telemetry_enabled && govcloud_partition.is_none() && cfg!(feature = "legacy_toolkit_sink")
}

fn should_build_codewhisperer_telemetry_client() -> bool {
    cfg!(feature = "legacy_codewhisperer_sink")
}

/// Resolve the persistent telemetry client id (env > database > new uuid).
pub fn resolve_client_id(env: &Env, database: &mut Database, telemetry_enabled: bool) -> Result<Uuid, LegacySinkError> {
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

/// Resolve whether telemetry is enabled for this run (cfg(test) > env > db setting).
pub fn resolve_telemetry_enabled(database: &Database) -> bool {
    !cfg!(test)
        && !crate::util::env_var::is_telemetry_disabled()
        && database.settings.get_bool(Setting::TelemetryEnabled).unwrap_or(true)
}

impl V2LegacySink {
    /// Construct the V2 legacy sink. `client_id` and `telemetry_enabled` should be
    /// resolved by the caller via [`resolve_client_id`] / [`resolve_telemetry_enabled`]
    /// — they MUST match the values placed on `HostConfig`.
    pub async fn build(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        govcloud_partition: Option<&str>,
        client_id: Uuid,
        telemetry_enabled: bool,
    ) -> Result<Arc<dyn LegacySink>, LegacySinkError> {
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

        let codewhisperer_client = if should_build_codewhisperer_telemetry_client() {
            Some(ApiClient::new(env, fs, database, None).await?)
        } else {
            None
        };

        Ok(Arc::new(Self {
            client_id,
            telemetry_enabled,
            codewhisperer_client,
            toolkit_telemetry_client,
        }))
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
}

impl LegacySink for V2LegacySink {
    fn send_event(&self, event: Event) -> BoxFuture<'_, ()> {
        async move {
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
        .boxed()
    }

    fn send_event_govcloud(&self, event: Event, _partition: &'static str) -> BoxFuture<'_, ()> {
        async move {
            if self.toolkit_telemetry_client.is_some() {
                // The host already emits the GovCloud disabled-channel posture metric;
                // we just need to NOT post to the toolkit endpoint.
                trace!("GovCloud: dropping toolkit telemetry");
            }
            #[cfg(feature = "legacy_codewhisperer_sink")]
            self.send_cw_telemetry_event(&event).await;
            #[cfg(not(feature = "legacy_codewhisperer_sink"))]
            {
                let _ = event;
                trace!("legacy CodeWhisperer telemetry sink disabled by cargo feature");
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
