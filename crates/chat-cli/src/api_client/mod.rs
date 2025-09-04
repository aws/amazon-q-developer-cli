mod credentials;
pub mod customization;
mod delay_interceptor;
mod endpoints;
mod error;
pub mod model;
mod opt_out;
pub mod profile;
mod retry_classifier;
pub mod send_message_output;
use std::sync::Arc;
use std::time::Duration;

use amzn_codewhisperer_client::Client as CodewhispererClient;
use amzn_codewhisperer_client::operation::create_subscription_token::CreateSubscriptionTokenOutput;
use amzn_codewhisperer_client::types::Origin::Cli;
use amzn_codewhisperer_client::types::{
    Model,
    OptInFeatureToggle,
    OptOutPreference,
    SubscriptionStatus,
    TelemetryEvent,
    UserContext,
};
use amzn_codewhisperer_streaming_client::Client as CodewhispererStreamingClient;
use amzn_qdeveloper_streaming_client::Client as QDeveloperStreamingClient;
use amzn_qdeveloper_streaming_client::types::Origin;
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
use aws_credential_types::provider::ProvideCredentials;
use aws_types::request_id::RequestId;
use aws_types::sdk_config::StalledStreamProtectionConfig;
pub use endpoints::Endpoint;
pub use error::ApiClientError;
use parking_lot::Mutex;
pub use profile::list_available_profiles;
use serde_json::Map;
use tokio::sync::RwLock;
use tracing::{
    debug,
    error,
};

use crate::api_client::credentials::CredentialsChain;
use crate::api_client::delay_interceptor::DelayTrackingInterceptor;
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
};
use crate::api_client::opt_out::OptOutInterceptor;
use crate::api_client::send_message_output::SendMessageOutput;
use crate::auth::builder_id::BearerResolver;
use crate::aws_common::{
    UserAgentOverrideInterceptor,
    app_name,
    behavior_version,
};
use crate::database::settings::Setting;
use crate::database::{
    AuthProfile,
    Database,
};
use crate::os::{
    Env,
    Fs,
};

// Opt out constants
pub const X_AMZN_CODEWHISPERER_OPT_OUT_HEADER: &str = "x-amzn-codewhisperer-optout";

// TODO(bskiser): confirm timeout is updated to an appropriate value?
const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(60 * 5);

pub const MAX_RETRY_DELAY_DURATION: Duration = Duration::from_secs(10);

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

#[derive(Clone, Debug)]
pub struct ApiClient {
    client: CodewhispererClient,
    streaming_client: Option<CodewhispererStreamingClient>,
    sigv4_streaming_client: Option<QDeveloperStreamingClient>,
    mock_client: Option<Arc<Mutex<std::vec::IntoIter<Vec<ChatResponseStream>>>>>,
    profile: Option<AuthProfile>,
    model_cache: ModelCache,
}

impl ApiClient {
    pub async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        // endpoint is only passed here for list_profiles where it needs to be called for each region
        endpoint: Option<Endpoint>,
    ) -> Result<Self, ApiClientError> {
        let endpoint = endpoint.unwrap_or(Endpoint::configured_value(database));

        let credentials = Credentials::new("xxx", "xxx", None, None, "xxx");
        let bearer_sdk_config = aws_config::defaults(behavior_version())
            .region(endpoint.region.clone())
            .credentials_provider(credentials)
            .timeout_config(timeout_config(database))
            .retry_config(retry_config())
            .load()
            .await;

        let client = CodewhispererClient::from_conf(
            amzn_codewhisperer_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                .interceptor(OptOutInterceptor::new(database))
                .interceptor(UserAgentOverrideInterceptor::new())
                .bearer_token_resolver(BearerResolver)
                .app_name(app_name())
                .endpoint_url(endpoint.url())
                .build(),
        );

        if cfg!(test) {
            let mut this = Self {
                client,
                streaming_client: None,
                sigv4_streaming_client: None,
                mock_client: None,
                profile: None,
                model_cache: Arc::new(RwLock::new(None)),
            };

            if let Ok(json) = env.get("Q_MOCK_CHAT_RESPONSE") {
                this.set_mock_output(serde_json::from_str(fs.read_to_string(json).await.unwrap().as_str()).unwrap());
            }

            return Ok(this);
        }

        // If SIGV4_AUTH_ENABLED is true, use Q developer client
        let mut streaming_client = None;
        let mut sigv4_streaming_client = None;
        match env.get("AMAZON_Q_SIGV4").is_ok() {
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
                            .timeout_config(timeout_config(database))
                            .retry_config(retry_config())
                            .load()
                            .await,
                    )
                    .http_client(crate::aws_common::http_client::client())
                    .interceptor(OptOutInterceptor::new(database))
                    .interceptor(UserAgentOverrideInterceptor::new())
                    .interceptor(DelayTrackingInterceptor::new())
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
                        .interceptor(OptOutInterceptor::new(database))
                        .interceptor(UserAgentOverrideInterceptor::new())
                        .interceptor(DelayTrackingInterceptor::new())
                        .bearer_token_resolver(BearerResolver)
                        .app_name(app_name())
                        .endpoint_url(endpoint.url())
                        .retry_classifier(retry_classifier::QCliRetryClassifier::new())
                        .stalled_stream_protection(stalled_stream_protection_config())
                        .build(),
                ));
            },
        }

        // Check if using custom endpoint
        let use_profile = !Self::is_custom_endpoint(database);
        let profile = if use_profile {
            match database.get_auth_profile() {
                Ok(profile) => profile,
                Err(err) => {
                    error!("Failed to get auth profile: {err}");
                    None
                },
            }
        } else {
            debug!("Custom endpoint detected, skipping profile ARN");
            None
        };

        Ok(Self {
            client,
            streaming_client,
            sigv4_streaming_client,
            mock_client: None,
            profile,
            model_cache: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn send_telemetry_event(
        &self,
        telemetry_event: TelemetryEvent,
        user_context: UserContext,
        telemetry_enabled: bool,
        model: Option<String>,
    ) -> Result<(), ApiClientError> {
        if cfg!(test) {
            return Ok(());
        }

        self.client
            .send_telemetry_event()
            .telemetry_event(telemetry_event)
            .user_context(user_context)
            .opt_out_preference(match telemetry_enabled {
                true => OptOutPreference::OptIn,
                false => OptOutPreference::OptOut,
            })
            .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()))
            .set_model_id(model)
            .send()
            .await?;

        Ok(())
    }

    pub async fn list_available_profiles(&self) -> Result<Vec<AuthProfile>, ApiClientError> {
        if cfg!(test) {
            return Ok(vec![
                AuthProfile {
                    arn: "my:arn:1".to_owned(),
                    profile_name: "MyProfile".to_owned(),
                },
                AuthProfile {
                    arn: "my:arn:2".to_owned(),
                    profile_name: "MyOtherProfile".to_owned(),
                },
            ]);
        }

        let mut profiles = vec![];
        let mut stream = self.client.list_available_profiles().into_paginator().send();
        while let Some(profiles_output) = stream.next().await {
            profiles.extend(profiles_output?.profiles().iter().cloned().map(AuthProfile::from));
        }

        Ok(profiles)
    }

    pub async fn list_available_models(&self) -> Result<ModelListResult, ApiClientError> {
        if cfg!(test) {
            let m = Model::builder()
                .model_id("model-1")
                .description("Test Model 1")
                .build()
                .unwrap();

            return Ok(ModelListResult {
                models: vec![m.clone()],
                default_model: m,
            });
        }

        let mut models = Vec::new();
        let mut default_model = None;
        let request = self
            .client
            .list_available_models()
            .set_origin(Some(Cli))
            .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()));
        let mut paginator = request.into_paginator().send();

        while let Some(result) = paginator.next().await {
            let models_output = result?;
            models.extend(models_output.models().iter().cloned());

            if default_model.is_none() {
                default_model = Some(models_output.default_model().clone());
            }
        }
        let default_model = default_model.ok_or_else(|| ApiClientError::DefaultModelNotFound)?;
        Ok(ModelListResult { models, default_model })
    }

    pub async fn list_available_models_cached(&self) -> Result<ModelListResult, ApiClientError> {
        {
            let cache = self.model_cache.read().await;
            if let Some(cached) = cache.as_ref() {
                tracing::debug!("Returning cached model list");
                return Ok(cached.clone());
            }
        }

        tracing::debug!("Cache miss, fetching models from list_available_models API");
        let result = self.list_available_models().await?;
        {
            let mut cache = self.model_cache.write().await;
            *cache = Some(result.clone());
        }
        Ok(result)
    }

    pub async fn invalidate_model_cache(&self) {
        let mut cache = self.model_cache.write().await;
        *cache = None;
        tracing::info!("Model cache invalidated");
    }

    pub async fn get_available_models(&self, _region: &str) -> Result<ModelListResult, ApiClientError> {
        let res = self.list_available_models_cached().await?;
        // TODO: Once we have access to gpt-oss, add back.
        // if region == "us-east-1" {
        //     let gpt_oss = Model::builder()
        //         .model_id("OPENAI_GPT_OSS_120B_1_0")
        //         .model_name("openai-gpt-oss-120b-preview")
        //         .token_limits(TokenLimits::builder().max_input_tokens(128_000).build())
        //         .build()
        //         .map_err(ApiClientError::from)?;

        //     models.push(gpt_oss);
        // }

        Ok(res)
    }

    pub async fn is_mcp_enabled(&self) -> Result<bool, ApiClientError> {
        let request = self
            .client
            .get_profile()
            .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()));

        let response = request.send().await?;
        let mcp_enabled = response
            .profile()
            .opt_in_features()
            .and_then(|features| features.mcp_configuration())
            .is_none_or(|config| matches!(config.toggle(), OptInFeatureToggle::On));
        Ok(mcp_enabled)
    }

    pub async fn create_subscription_token(&self) -> Result<CreateSubscriptionTokenOutput, ApiClientError> {
        if cfg!(test) {
            return Ok(CreateSubscriptionTokenOutput::builder()
                .set_encoded_verification_url(Some("test/url".to_string()))
                .set_status(Some(SubscriptionStatus::Inactive))
                .set_token(Some("test-token".to_string()))
                .build()?);
        }

        self.client
            .create_subscription_token()
            .send()
            .await
            .map_err(ApiClientError::CreateSubscriptionToken)
    }

    pub async fn send_message(&self, conversation: ConversationState) -> Result<SendMessageOutput, ApiClientError> {
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
                    let status_code = err.raw_response().map(|res| res.status().as_u16());
                    let is_quota_breach = status_code.is_some_and(|status| status == 429);
                    let is_context_window_overflow = err.as_service_error().is_some_and(|err| {
                        matches!(err, err if err.meta().code() == Some("ValidationException") && err.meta().message() == Some("Input is too long."))
                    });

                    let is_model_unavailable = {
                        // check if ThrottlingException
                        let is_throttling_exception = err
                            .as_service_error()
                            .is_some_and(|service_err| service_err.meta().code() == Some("ThrottlingException"));

                        // check if the response contains INSUFFICIENT_MODEL_CAPACITY
                        let has_insufficient_capacity = err
                            .raw_response()
                            .and_then(|resp| resp.body().bytes())
                            .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
                            .is_some_and(|body| body.contains("INSUFFICIENT_MODEL_CAPACITY"));

                        (is_throttling_exception && has_insufficient_capacity)
                        // Legacy error response fallback
                        || (model_id_opt.is_some()
                        && status_code.is_some_and(|status| status == 500)
                        && err.as_service_error().is_some_and(|err| {
                            err.meta().message() == Some(
                    "Encountered unexpectedly high load when processing the request, please try again.",
                )}))
                    };

                    let is_monthly_limit_err = err
                        .raw_response()
                        .and_then(|resp| resp.body().bytes())
                        .and_then(|bytes| match String::from_utf8(bytes.to_vec()) {
                            Ok(s) => Some(s.contains("MONTHLY_REQUEST_COUNT")),
                            Err(_) => None,
                        })
                        .unwrap_or(false);

                    if is_context_window_overflow {
                        return Err(ApiClientError::ContextWindowOverflow { status_code });
                    }

                    // Both ModelOverloadedError and QuotaBreach return 429,
                    // so check is_model_unavailable first.
                    if is_model_unavailable {
                        return Err(ApiClientError::ModelOverloadedError {
                            request_id: err
                                .as_service_error()
                                .and_then(|err| err.meta().request_id())
                                .map(|s| s.to_string()),
                            status_code,
                        });
                    }

                    if is_quota_breach {
                        return Err(ApiClientError::QuotaBreach {
                            message: "quota has reached its limit",
                            status_code,
                        });
                    }

                    if is_monthly_limit_err {
                        return Err(ApiClientError::MonthlyLimitReached { status_code });
                    }

                    Err(err.into())
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
                    let status_code = err.raw_response().map(|res| res.status().as_u16());
                    let is_quota_breach = status_code.is_some_and(|status| status == 429);
                    let is_context_window_overflow = err.as_service_error().is_some_and(|err| {
                        matches!(err, err if err.meta().code() == Some("ValidationException") && err.meta().message() == Some("Input is too long."))
                    });

                    let is_model_unavailable = {
                        // check if ThrottlingException
                        let is_throttling_exception = err
                            .as_service_error()
                            .is_some_and(|service_err| service_err.meta().code() == Some("ThrottlingException"));

                        // check if the response contains INSUFFICIENT_MODEL_CAPACITY
                        let has_insufficient_capacity = err
                            .raw_response()
                            .and_then(|resp| resp.body().bytes())
                            .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
                            .is_some_and(|body| body.contains("INSUFFICIENT_MODEL_CAPACITY"));

                        (is_throttling_exception && has_insufficient_capacity)
                        // Legacy error response fallback
                        || (model_id_opt.is_some()
                        && status_code.is_some_and(|status| status == 500)
                        && err.as_service_error().is_some_and(|err| {
                            err.meta().message() == Some(
                    "Encountered unexpectedly high load when processing the request, please try again.",
                )}))
                    };

                    let is_monthly_limit_err = err
                        .raw_response()
                        .and_then(|resp| resp.body().bytes())
                        .and_then(|bytes| match String::from_utf8(bytes.to_vec()) {
                            Ok(s) => Some(s.contains("MONTHLY_REQUEST_COUNT")),
                            Err(_) => None,
                        })
                        .unwrap_or(false);

                    // Both ModelOverloadedError and QuotaBreach return 429,
                    // so check is_model_unavailable first.
                    if is_model_unavailable {
                        return Err(ApiClientError::ModelOverloadedError {
                            request_id: err
                                .as_service_error()
                                .and_then(|err| err.meta().request_id())
                                .map(|s| s.to_string()),
                            status_code,
                        });
                    }

                    if is_quota_breach {
                        return Err(ApiClientError::QuotaBreach {
                            message: "quota has reached its limit",
                            status_code,
                        });
                    }

                    if is_context_window_overflow {
                        return Err(ApiClientError::ContextWindowOverflow { status_code });
                    }

                    if is_monthly_limit_err {
                        return Err(ApiClientError::MonthlyLimitReached { status_code });
                    }

                    Err(err.into())
                },
            }
        } else if let Some(client) = &self.mock_client {
            let mut new_events = client.lock().next().unwrap_or_default().clone();
            new_events.reverse();

            return Ok(SendMessageOutput::Mock(new_events));
        } else {
            unreachable!("One of the clients must be created by this point");
        }
    }

    /// Only meant for testing. Do not use outside of testing responses.
    pub fn set_mock_output(&mut self, json: serde_json::Value) {
        let mut mock = Vec::new();
        for response in json.as_array().unwrap() {
            let mut stream = Vec::new();
            for event in response.as_array().unwrap() {
                match event {
                    serde_json::Value::String(assistant_text) => {
                        stream.push(ChatResponseStream::AssistantResponseEvent {
                            content: assistant_text.clone(),
                        });
                    },
                    serde_json::Value::Object(tool_use) => {
                        stream.append(&mut split_tool_use_event(tool_use));
                    },
                    other => panic!("Unexpected value: {:?}", other),
                }
            }
            mock.push(stream);
        }

        self.mock_client = Some(Arc::new(Mutex::new(mock.into_iter())));
    }

    // Add a helper method to check if using non-default endpoint
    fn is_custom_endpoint(database: &Database) -> bool {
        database.settings.get(Setting::ApiCodeWhispererService).is_some()
    }
}

fn timeout_config(database: &Database) -> TimeoutConfig {
    let timeout = database
        .settings
        .get_int(Setting::ApiTimeout)
        .and_then(|i| i.try_into().ok())
        .map_or(DEFAULT_TIMEOUT_DURATION, Duration::from_millis);

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
        .with_max_backoff(MAX_RETRY_DELAY_DURATION)
}

pub fn stalled_stream_protection_config() -> StalledStreamProtectionConfig {
    StalledStreamProtectionConfig::enabled()
        .grace_period(Duration::from_secs(60 * 5))
        .build()
}

fn split_tool_use_event(value: &Map<String, serde_json::Value>) -> Vec<ChatResponseStream> {
    let tool_use_id = value.get("tool_use_id").unwrap().as_str().unwrap().to_string();
    let name = value.get("name").unwrap().as_str().unwrap().to_string();
    let args_str = value.get("args").unwrap().to_string();
    let split_point = args_str.len() / 2;
    vec![
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: None,
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: Some(args_str.split_at(split_point).0.to_string()),
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: Some(args_str.split_at(split_point).1.to_string()),
            stop: None,
        },
        ChatResponseStream::ToolUseEvent {
            tool_use_id: tool_use_id.clone(),
            name: name.clone(),
            input: None,
            stop: Some(true),
        },
    ]
}

#[cfg(test)]
mod tests {
    use amzn_codewhisperer_client::types::{
        ChatAddMessageEvent,
        IdeCategory,
        OperatingSystem,
    };

    use super::*;
    use crate::api_client::model::UserInputMessage;

    #[tokio::test]
    async fn create_clients() {
        let env = Env::new();
        let fs = Fs::new();
        let mut database = crate::database::Database::new().await.unwrap();
        let _ = ApiClient::new(&env, &fs, &mut database, None).await;
    }

    #[tokio::test]
    async fn test_mock() {
        let env = Env::new();
        let fs = Fs::new();
        let mut database = crate::database::Database::new().await.unwrap();
        let mut client = ApiClient::new(&env, &fs, &mut database, None).await.unwrap();
        client
            .send_telemetry_event(
                TelemetryEvent::ChatAddMessageEvent(
                    ChatAddMessageEvent::builder()
                        .conversation_id("<conversation-id>")
                        .message_id("<message-id>")
                        .build()
                        .unwrap(),
                ),
                UserContext::builder()
                    .ide_category(IdeCategory::Cli)
                    .operating_system(OperatingSystem::Linux)
                    .product("<product>")
                    .build()
                    .unwrap(),
                false,
                Some("model".to_owned()),
            )
            .await
            .unwrap();

        client.mock_client = Some(Arc::new(Mutex::new(
            vec![vec![
                ChatResponseStream::AssistantResponseEvent {
                    content: "Hello!".to_owned(),
                },
                ChatResponseStream::AssistantResponseEvent {
                    content: " How can I".to_owned(),
                },
                ChatResponseStream::AssistantResponseEvent {
                    content: " assist you today?".to_owned(),
                },
            ]]
            .into_iter(),
        )));

        let mut output = client
            .send_message(ConversationState {
                conversation_id: None,
                user_input_message: UserInputMessage {
                    images: None,
                    content: "Hello".into(),
                    user_input_message_context: None,
                    user_intent: None,
                    model_id: Some("model".to_owned()),
                },
                history: None,
            })
            .await
            .unwrap();

        let mut output_content = String::new();
        while let Some(ChatResponseStream::AssistantResponseEvent { content }) = output.recv().await.unwrap() {
            output_content.push_str(&content);
        }
        assert_eq!(output_content, "Hello! How can I assist you today?");
    }
}
