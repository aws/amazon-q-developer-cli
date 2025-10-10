mod credentials;
mod endpoints;
pub mod error;
pub mod model;
mod opt_out;
pub mod request;
mod retry_classifier;
pub mod send_message_output;

use std::sync::{
    Arc,
    RwLock,
};
use std::time::Duration;

use amzn_codewhisperer_client::Client as CodewhispererClient;
use amzn_codewhisperer_client::types::Model;
use amzn_codewhisperer_streaming_client::Client as CodewhispererStreamingClient;
use amzn_qdeveloper_streaming_client::Client as QDeveloperStreamingClient;
use amzn_qdeveloper_streaming_client::types::Origin;
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
use aws_credential_types::provider::ProvideCredentials;
use aws_types::request_id::RequestId;
use aws_types::sdk_config::StalledStreamProtectionConfig;
use credentials::CredentialsChain;
use endpoints::Endpoint;
use error::{
    ApiClientError,
    ConverseStreamError,
    ConverseStreamErrorKind,
};
use model::ConversationState;
use send_message_output::SendMessageOutput;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::debug;

use crate::auth::builder_id::BearerResolver;
use crate::aws_common::{
    UserAgentOverrideInterceptor,
    app_name,
    behavior_version,
};

pub const X_AMZN_CODEWHISPERER_OPT_OUT_HEADER: &str = "x-amzn-codewhisperer-optout";

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(60 * 5);

#[derive(Clone, Debug)]
pub struct ModelListResult {
    pub models: Vec<Model>,
    pub default_model: Model,
}

impl From<ModelListResult> for (Vec<Model>, Model) {
    fn from(v: ModelListResult) -> Self {
        (v.models, v.default_model)
    }
}

type ModelCache = Arc<RwLock<Option<ModelListResult>>>;

#[derive(Clone)]
pub struct ApiClient {
    client: CodewhispererClient,
    streaming_client: Option<CodewhispererStreamingClient>,
    sigv4_streaming_client: Option<QDeveloperStreamingClient>,
    profile: Option<AuthProfile>,
    model_cache: ModelCache,
}

impl std::fmt::Debug for ApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiClient")
            .field(
                "streaming_client",
                if self.streaming_client.is_some() {
                    &"Some(_)"
                } else {
                    &"None"
                },
            )
            .field(
                "sigv4_streaming_client",
                if self.sigv4_streaming_client.is_some() {
                    &"Some(_)"
                } else {
                    &"None"
                },
            )
            .field("profile", &self.profile)
            .field("model_cache", &self.model_cache)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthProfile {
    pub arn: String,
    pub profile_name: String,
}

impl ApiClient {
    pub async fn new() -> Result<Self, ApiClientError> {
        let endpoint = Endpoint::DEFAULT_ENDPOINT;

        let credentials = Credentials::new("xxx", "xxx", None, None, "xxx");
        let bearer_sdk_config = aws_config::defaults(behavior_version())
            .region(endpoint.region.clone())
            .credentials_provider(credentials)
            .timeout_config(timeout_config())
            .retry_config(retry_config())
            .load()
            .await;

        let client = CodewhispererClient::from_conf(
            amzn_codewhisperer_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                // .interceptor(OptOutInterceptor::new(database))
                .interceptor(UserAgentOverrideInterceptor::new())
                .bearer_token_resolver(BearerResolver)
                .app_name(app_name())
                .endpoint_url(endpoint.url())
                .build(),
        );

        // If SIGV4_AUTH_ENABLED is true, use Q developer client
        let mut streaming_client = None;
        let mut sigv4_streaming_client = None;
        match std::env::var("AMAZON_Q_SIGV4").is_ok() {
            true => {
                let credentials_chain = CredentialsChain::new().await;
                if let Err(err) = credentials_chain.provide_credentials().await {
                    return Err(ApiClientError::Credentials(err));
                };

                sigv4_streaming_client = Some(QDeveloperStreamingClient::from_conf(
                    amzn_qdeveloper_streaming_client::config::Builder::from(
                        &aws_config::defaults(behavior_version())
                            .region(endpoint.region.clone())
                            .credentials_provider(credentials_chain)
                            .timeout_config(timeout_config())
                            .retry_config(retry_config())
                            .load()
                            .await,
                    )
                    .http_client(crate::aws_common::http_client::client())
                    // .interceptor(OptOutInterceptor::new(database))
                    .interceptor(UserAgentOverrideInterceptor::new())
                    // .interceptor(DelayTrackingInterceptor::new())
                    .app_name(app_name())
                    .endpoint_url(endpoint.url())
                    .retry_classifier(retry_classifier::QCliRetryClassifier::new())
                    .stalled_stream_protection(stalled_stream_protection_config())
                    .build(),
                ));
            },
            false => {
                streaming_client = Some(CodewhispererStreamingClient::from_conf(
                    amzn_codewhisperer_streaming_client::config::Builder::from(&bearer_sdk_config)
                        .http_client(crate::aws_common::http_client::client())
                        // .interceptor(OptOutInterceptor::new(database))
                        .interceptor(UserAgentOverrideInterceptor::new())
                        // .interceptor(DelayTrackingInterceptor::new())
                        .bearer_token_resolver(BearerResolver)
                        .app_name(app_name())
                        .endpoint_url(endpoint.url())
                        .retry_classifier(retry_classifier::QCliRetryClassifier::new())
                        .stalled_stream_protection(stalled_stream_protection_config())
                        .build(),
                ));
            },
        }

        let profile = None;
        // let profile = match database.get_auth_profile() {
        //     Ok(profile) => profile,
        //     Err(err) => {
        //         error!("Failed to get auth profile: {err}");
        //         None
        //     },
        // };

        Ok(Self {
            client,
            streaming_client,
            sigv4_streaming_client,
            profile,
            model_cache: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn send_message(
        &self,
        conversation: ConversationState,
    ) -> Result<SendMessageOutput, ConverseStreamError> {
        debug!("Sending conversation: {:#?}", conversation);

        let ConversationState {
            conversation_id,
            user_input_message,
            history,
        } = conversation;

        let model_id_opt: Option<String> = user_input_message.model_id.clone();

        if let Some(client) = &self.streaming_client {
            let conversation_state = amzn_codewhisperer_streaming_client::types::ConversationState::builder()
                .set_conversation_id(conversation_id)
                .current_message(
                    amzn_codewhisperer_streaming_client::types::ChatMessage::UserInputMessage(
                        user_input_message.into(),
                    ),
                )
                .chat_trigger_type(amzn_codewhisperer_streaming_client::types::ChatTriggerType::Manual)
                .set_history(
                    history
                        .map(|v| v.into_iter().map(|i| i.try_into()).collect::<Result<Vec<_>, _>>())
                        .transpose()?,
                )
                .build()
                .expect("building conversation should not fail");

            match client
                .generate_assistant_response()
                .conversation_state(conversation_state)
                .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()))
                .send()
                .await
            {
                Ok(response) => Ok(SendMessageOutput::Codewhisperer(response)),
                Err(err) => {
                    let request_id = err
                        .as_service_error()
                        .and_then(|err| err.meta().request_id())
                        .map(|s| s.to_string());
                    let status_code = err.raw_response().map(|res| res.status().as_u16());

                    let body = err
                        .raw_response()
                        .and_then(|resp| resp.body().bytes())
                        .unwrap_or_default();
                    Err(
                        ConverseStreamError::new(classify_error_kind(status_code, body), Some(err))
                            .set_request_id(request_id)
                            .set_status_code(status_code),
                    )
                },
            }
        } else if let Some(client) = &self.sigv4_streaming_client {
            let conversation_state = amzn_qdeveloper_streaming_client::types::ConversationState::builder()
                .set_conversation_id(conversation_id)
                .current_message(amzn_qdeveloper_streaming_client::types::ChatMessage::UserInputMessage(
                    user_input_message.into(),
                ))
                .chat_trigger_type(amzn_qdeveloper_streaming_client::types::ChatTriggerType::Manual)
                .set_history(
                    history
                        .map(|v| v.into_iter().map(|i| i.try_into()).collect::<Result<Vec<_>, _>>())
                        .transpose()?,
                )
                .build()
                .expect("building conversation_state should not fail");

            match client
                .send_message()
                .conversation_state(conversation_state)
                .set_source(Some(Origin::from("CLI")))
                .send()
                .await
            {
                Ok(response) => Ok(SendMessageOutput::QDeveloper(response)),
                Err(err) => {
                    let request_id = err
                        .as_service_error()
                        .and_then(|err| err.meta().request_id())
                        .map(|s| s.to_string());
                    let status_code = err.raw_response().map(|res| res.status().as_u16());

                    let body = err
                        .raw_response()
                        .and_then(|resp| resp.body().bytes())
                        .unwrap_or_default();
                    Err(
                        ConverseStreamError::new(classify_error_kind(status_code, body), Some(err))
                            .set_request_id(request_id)
                            .set_status_code(status_code),
                    )
                },
            }
        } else {
            unreachable!("One of the clients must be created by this point");
        }
    }
}

fn classify_error_kind(status_code: Option<u16>, body: &[u8]) -> ConverseStreamErrorKind {
    let contains = |haystack: &[u8], needle: &[u8]| haystack.windows(needle.len()).any(|v| v == needle);

    let is_throttling = status_code.is_some_and(|status| status == 429);
    let is_context_window_overflow = contains(body, b"Input is too long.");
    let is_model_unavailable = contains(body, b"INSUFFICIENT_MODEL_CAPACITY")
        || (status_code.is_some_and(|status| status == 500)
            && contains(
                body,
                b"Encountered unexpectedly high load when processing the request, please try again.",
            ));
    let is_monthly_limit_err = contains(body, b"MONTHLY_REQUEST_COUNT");

    if is_context_window_overflow {
        return ConverseStreamErrorKind::ContextWindowOverflow;
    }

    // Both ModelOverloadedError and Throttling return 429,
    // so check is_model_unavailable first.
    if is_model_unavailable {
        return ConverseStreamErrorKind::ModelOverloadedError;
    }

    if is_throttling {
        return ConverseStreamErrorKind::Throttling;
    }

    if is_monthly_limit_err {
        return ConverseStreamErrorKind::MonthlyLimitReached;
    }

    ConverseStreamErrorKind::Unknown
}

fn timeout_config() -> TimeoutConfig {
    let timeout = DEFAULT_TIMEOUT_DURATION;

    TimeoutConfig::builder()
        .read_timeout(timeout)
        .operation_timeout(timeout)
        .operation_attempt_timeout(timeout)
        .connect_timeout(timeout)
        .build()
}

fn retry_config() -> RetryConfig {
    RetryConfig::adaptive()
        .with_max_attempts(3)
        .with_max_backoff(Duration::from_secs(10))
}

pub fn stalled_stream_protection_config() -> StalledStreamProtectionConfig {
    StalledStreamProtectionConfig::enabled()
        .grace_period(Duration::from_secs(60 * 5))
        .build()
}
