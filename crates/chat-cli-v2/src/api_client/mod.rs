pub mod customization;
pub mod delay_interceptor;
mod endpoints;
pub mod error;
pub mod error_utils;
pub mod model;
pub mod opt_out;
pub mod profile;
mod retry_classifier;
pub mod send_message_output;

use std::collections::{
    HashMap,
    VecDeque,
};
use std::sync::Arc;
use std::time::Duration;

use amzn_codewhisperer_client::Client as CodewhispererClient;
use amzn_codewhisperer_client::operation::create_subscription_token::CreateSubscriptionTokenOutput;
use amzn_codewhisperer_client::types::{
    Model,
    OptInFeatureToggle,
    OptOutPreference,
    Origin,
    SubscriptionStatus,
    TelemetryEvent,
    UserContext,
};
use amzn_codewhisperer_streaming_client::Client as CodewhispererStreamingClient;
use amzn_codewhisperer_streaming_client::config::endpoint::{
    Endpoint as StreamingEndpoint,
    EndpointFuture,
    Params,
    ResolveEndpoint,
};
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
use aws_sdk_ssooidc::error::ProvideErrorMetadata;
use aws_types::request_id::RequestId;
use aws_types::sdk_config::StalledStreamProtectionConfig;
pub use endpoints::Endpoint;
pub use error::ApiClientError;
use error::{
    ConverseStreamError,
    ConverseStreamErrorKind,
};
use parking_lot::Mutex;
pub use profile::list_available_profiles;
use serde_json::Map;
use tokio::sync::{
    RwLock,
    mpsc,
    oneshot,
};
use tracing::{
    debug,
    error,
};

use crate::api_client::delay_interceptor::DelayTrackingInterceptor;
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
};
use crate::api_client::opt_out::OptOutInterceptor;
use crate::api_client::send_message_output::{
    MockStreamItem,
    SendMessageOutput,
    record_send_error,
};
use crate::auth::UnifiedBearerResolver;
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
use crate::util::env_var::is_integ_test;

#[derive(Debug)]
struct StaticEndpointResolver {
    url: String,
}

impl StaticEndpointResolver {
    fn new(url: String) -> Self {
        Self { url }
    }
}

impl ResolveEndpoint for StaticEndpointResolver {
    fn resolve_endpoint<'a>(&'a self, _params: &'a Params) -> EndpointFuture<'a> {
        let url = self.url.clone();
        let endpoint = StreamingEndpoint::builder().url(url).build();
        EndpointFuture::ready(Ok(endpoint))
    }
}

#[derive(Debug)]
struct StaticCodewhispererEndpointResolver {
    url: String,
}

impl StaticCodewhispererEndpointResolver {
    fn new(url: String) -> Self {
        Self { url }
    }
}

impl amzn_codewhisperer_client::config::endpoint::ResolveEndpoint for StaticCodewhispererEndpointResolver {
    fn resolve_endpoint<'a>(
        &'a self,
        _params: &'a amzn_codewhisperer_client::config::endpoint::Params,
    ) -> EndpointFuture<'a> {
        use aws_smithy_types::endpoint::Endpoint;
        let url = self.url.clone();
        let endpoint = Endpoint::builder().url(url).build();
        EndpointFuture::ready(Ok(endpoint))
    }
}

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
enum ApiClientInner {
    Real(RealApiClient),
    IpcMock(IpcMockApiClient),
}

#[derive(Clone, Debug)]
pub struct ApiClient {
    inner: ApiClientInner,
}

#[derive(Clone, Debug)]
struct RealApiClient {
    client: CodewhispererClient,
    streaming_client: Option<CodewhispererStreamingClient>,
    mock_client: Option<Arc<Mutex<std::vec::IntoIter<Vec<ChatResponseStream>>>>>,
    profile: Option<AuthProfile>,
    model_cache: ModelCache,
}

/// Handle to an actor that owns a shared registry for mock API responses, keyed by session_id.
///
/// ## Architecture
///
/// ```text
/// Test Harness                    SessionManager              MockResponseRegistry Actor
///      │                              │                                │
///      │  push_mock_response          │                                │
///      │  (session_id, events)        │                                │
///      │─────────────────────────────►│                                │
///      │                              │  registry.push(session_id,     │
///      │                              │               events)          │
///      │                              │───────────────────────────────►│
///      │                              │                                │ buffers events
///      │                              │                                │ per session_id
///      │                              │                                │
///      │  ACP: Prompt                 │                                │
///      │─────────────────────────────►│                                │
///      │                              │  (routes to AcpSession)        │
///      │                              │                                │
///      │                              │         IpcMockApiClient       │
///      │                              │         ::send_message()       │
///      │                              │                                │
///      │                              │  registry.get_stream           │
///      │                              │  (session_id, conversation)    │
///      │                              │───────────────────────────────►│
///      │                              │                                │ captures request
///      │                              │                                │ creates mpsc channel
///      │                              │◄───────────────────────────────│ drains buffer till None
///      │                              │  returns Receiver              │
///      │                              │                                │
///      │  get_captured_requests       │                                │
///      │  (session_id)                │                                │
///      │─────────────────────────────►│                                │
///      │                              │  registry.get_captured         │
///      │                              │  (session_id)                  │
///      │                              │───────────────────────────────►│
///      │◄─────────────────────────────│◄───────────────────────────────│
///      │  Vec<ConversationState>      │                                │
/// ```
///
/// ## Lifecycle
///
/// 1. `SessionManager` spawns the registry actor on startup (test mode only)
/// 2. `IpcServer` routes `PushSendMessageResponse` commands to `registry.push()`
/// 3. Each `AcpSession` gets an `IpcMockApiClient` holding a clone of the registry
/// 4. When `send_message()` is called, it calls `registry.get_stream(session_id, conversation)`
/// 5. The actor captures the request, creates a channel, drains buffered events, and returns the
///    receiver
/// 6. Tests can retrieve captured requests via `get_captured_requests(session_id)`
#[derive(Clone, Debug)]
pub struct MockResponseRegistryHandle {
    tx: mpsc::Sender<MockRegistryRequest>,
}

/// Messages sent to the mock registry actor.
enum MockRegistryRequest {
    /// Push mock items for a session. `None` signals end of response stream.
    PushEvents {
        session_id: String,
        events: Option<Vec<MockStreamItem>>,
    },
    /// Request a stream of mock events for a session. Called by `IpcMockApiClient::send_message`.
    ///
    /// Returns `Ok(receiver)` for normal streams, or `Err(error)` if first item is `SendError`.
    GetStream {
        session_id: String,
        conversation: Box<ConversationState>,
        respond_to: oneshot::Sender<Result<mpsc::Receiver<MockStreamItem>, ConverseStreamError>>,
    },
    /// Get captured requests for a session.
    GetCapturedRequests {
        session_id: String,
        respond_to: oneshot::Sender<Vec<ConversationState>>,
    },
}

impl MockResponseRegistryHandle {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(mock_registry_actor(rx));
        Self { tx }
    }

    /// Push mock response items for a session.
    pub async fn push_events(&self, session_id: String, events: Option<Vec<MockStreamItem>>) {
        let _ = self
            .tx
            .send(MockRegistryRequest::PushEvents { session_id, events })
            .await;
    }

    /// Get a response stream for a session (called by IpcMockApiClient).
    /// Returns `Err` if first buffered item is `SendError`.
    async fn get_stream(
        &self,
        session_id: &str,
        conversation: ConversationState,
    ) -> Result<mpsc::Receiver<MockStreamItem>, ConverseStreamError> {
        let (respond_to, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(MockRegistryRequest::GetStream {
                session_id: session_id.to_string(),
                conversation: Box::new(conversation),
                respond_to,
            })
            .await;
        rx.await.expect("mock registry actor should respond")
    }

    /// Get captured requests for a session.
    pub async fn get_captured_requests(&self, session_id: &str) -> Vec<ConversationState> {
        let (respond_to, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(MockRegistryRequest::GetCapturedRequests {
                session_id: session_id.to_string(),
                respond_to,
            })
            .await;
        rx.await.expect("mock registry actor should respond")
    }
}

/// Per-session state for mock response buffering and streaming.
#[derive(Default)]
struct SessionMockState {
    /// Items waiting to be consumed. `None` marks end of a response stream.
    buffer: VecDeque<Option<MockStreamItem>>,
    /// Active stream sender, set when `GetStream` is called before all events are buffered.
    stream_tx: Option<mpsc::Sender<MockStreamItem>>,
    /// Captured requests for this session.
    captured_requests: Vec<ConversationState>,
}

/// Actor that manages per-session mock response buffers and streams.
///
/// Handles two message types:
/// - `PushEvents`: Buffer items for a session. If that session has an active stream waiting, drain
///   items to it immediately.
/// - `GetStream`: If first item is `SendError`, return error. Otherwise create channel, drain
///   buffered items, return receiver.
async fn mock_registry_actor(mut rx: mpsc::Receiver<MockRegistryRequest>) {
    let mut sessions: HashMap<String, SessionMockState> = HashMap::new();

    while let Some(req) = rx.recv().await {
        match req {
            MockRegistryRequest::PushEvents { session_id, events } => {
                let state = sessions.entry(session_id).or_default();

                match events {
                    Some(items) => {
                        for item in items {
                            state.buffer.push_back(Some(item));
                        }
                    },
                    None => state.buffer.push_back(None),
                }

                // If there's an active stream waiting for events, drain buffer to it
                if let Some(ref tx) = state.stream_tx {
                    while let Some(item) = state.buffer.front() {
                        match item {
                            Some(_) => {
                                let item = state.buffer.pop_front().unwrap().unwrap();
                                let _ = tx.send(item).await;
                            },
                            None => {
                                state.buffer.pop_front();
                                state.stream_tx = None;
                                break;
                            },
                        }
                    }
                }
            },
            MockRegistryRequest::GetStream {
                session_id,
                conversation,
                respond_to,
            } => {
                let state = sessions.entry(session_id.clone()).or_default();
                assert!(
                    state.stream_tx.is_none(),
                    "GetStream called while previous stream for session {} is still active",
                    session_id
                );

                // Capture the request
                state.captured_requests.push(*conversation);

                // Check if first item is SendError
                #[allow(clippy::collapsible_if)]
                if let Some(Some(MockStreamItem::SendError(_))) = state.buffer.front() {
                    if let Some(Some(MockStreamItem::SendError(err))) = state.buffer.pop_front() {
                        let _ = respond_to.send(Err(err));
                        continue;
                    }
                }

                let (tx, rx) = mpsc::channel(32);
                let _ = respond_to.send(Ok(rx));

                // Drain any buffered items to the new stream
                let mut complete = false;
                while let Some(item) = state.buffer.pop_front() {
                    match item {
                        Some(item) => {
                            let _ = tx.send(item).await;
                        },
                        None => {
                            complete = true;
                            break;
                        },
                    }
                }

                if !complete {
                    state.stream_tx = Some(tx);
                }
            },
            MockRegistryRequest::GetCapturedRequests { session_id, respond_to } => {
                let requests = sessions
                    .get(&session_id)
                    .map(|s| s.captured_requests.clone())
                    .unwrap_or_default();
                let _ = respond_to.send(requests);
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct IpcMockApiClient {
    registry: MockResponseRegistryHandle,
}

impl IpcMockApiClient {
    pub fn new(registry: MockResponseRegistryHandle) -> Self {
        Self { registry }
    }

    pub async fn send_message(
        &self,
        conversation: ConversationState,
    ) -> Result<SendMessageOutput, ConverseStreamError> {
        let session_id = conversation
            .conversation_id
            .clone()
            .expect("conversation_id required in test mode");
        let rx = self.registry.get_stream(&session_id, conversation).await?;
        Ok(SendMessageOutput::IpcMock(rx))
    }

    pub async fn send_telemetry_event(
        &self,
        _telemetry_event: TelemetryEvent,
        _user_context: UserContext,
        _telemetry_enabled: bool,
        _model: Option<String>,
    ) -> Result<(), ApiClientError> {
        Ok(())
    }

    #[allow(clippy::todo)]
    pub async fn list_available_profiles(&self) -> Result<Vec<AuthProfile>, ApiClientError> {
        todo!("IpcMockApiClient::list_available_profiles")
    }

    pub async fn list_available_models(&self) -> Result<ModelListResult, ApiClientError> {
        self.list_available_models_cached().await
    }

    pub async fn list_available_models_cached(&self) -> Result<ModelListResult, ApiClientError> {
        // Return mock models for testing
        let models: Vec<Model> = [
            ("Auto", "Auto"),
            ("claude-sonnet-4.5", "Claude Sonnet 4.5"),
            ("claude-sonnet-4", "Claude Sonnet 4"),
            ("claude-haiku-4.5", "Claude Haiku 4.5"),
            ("claude-opus-4.5", "Claude Opus 4.5"),
            ("claude-sonnet-4.5-1m", "Claude Sonnet 4.5 1M"),
            ("qwen3-coder-480b", "Qwen3 Coder 480B"),
        ]
        .into_iter()
        .map(|(id, name)| Model::builder().model_id(id).model_name(name).build().unwrap())
        .collect();
        let default_model = models[0].clone();
        Ok(ModelListResult { models, default_model })
    }

    #[allow(clippy::todo)]
    pub async fn invalidate_model_cache(&self) {
        todo!("IpcMockApiClient::invalidate_model_cache")
    }

    #[allow(clippy::todo)]
    pub async fn get_available_models(&self, _region: &str) -> Result<ModelListResult, ApiClientError> {
        todo!("IpcMockApiClient::get_available_models")
    }

    #[allow(clippy::todo)]
    pub async fn is_mcp_enabled(&self) -> Result<bool, ApiClientError> {
        todo!("IpcMockApiClient::is_mcp_enabled")
    }

    #[allow(clippy::todo)]
    pub async fn get_mcp_config(&self) -> Result<(bool, Option<String>), ApiClientError> {
        todo!("IpcMockApiClient::get_mcp_config")
    }

    #[allow(clippy::todo)]
    pub async fn create_subscription_token(&self) -> Result<CreateSubscriptionTokenOutput, ApiClientError> {
        todo!("IpcMockApiClient::create_subscription_token")
    }

    #[allow(clippy::todo)]
    pub async fn get_usage_limits(
        &self,
    ) -> Result<amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsOutput, ApiClientError> {
        todo!("IpcMockApiClient::get_usage_limits")
    }
}

impl RealApiClient {
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
                .bearer_token_resolver(UnifiedBearerResolver)
                .app_name(app_name())
                .endpoint_resolver(StaticCodewhispererEndpointResolver::new(endpoint.url().to_string()))
                .build(),
        );

        if cfg!(test) && !is_integ_test() {
            let mut this = Self {
                client,
                streaming_client: None,
                mock_client: None,
                profile: None,
                model_cache: Arc::new(RwLock::new(None)),
            };

            if let Some(json) = crate::util::env_var::get_mock_chat_response(env) {
                this.set_mock_output(serde_json::from_str(fs.read_to_string(json).await.unwrap().as_str()).unwrap());
            }

            return Ok(this);
        }

        // Use CodeWhisperer streaming client with bearer token
        let streaming_client = Some(CodewhispererStreamingClient::from_conf(
            amzn_codewhisperer_streaming_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                .interceptor(OptOutInterceptor::new(database))
                .interceptor(UserAgentOverrideInterceptor::new())
                .interceptor(DelayTrackingInterceptor::new())
                .bearer_token_resolver(UnifiedBearerResolver)
                .app_name(app_name())
                .endpoint_resolver(StaticEndpointResolver::new(endpoint.url().to_string()))
                .retry_classifier(retry_classifier::QCliRetryClassifier::new())
                .stalled_stream_protection(stalled_stream_protection_config())
                .build(),
        ));

        // Check if using custom endpoint
        let use_profile = !is_custom_endpoint(database);
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
            .set_origin(Some(Origin::KiroCli))
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
        let (enabled, _) = self.get_mcp_config().await?;
        Ok(enabled)
    }

    /// Get MCP configuration including enabled status and registry URL
    pub async fn get_mcp_config(&self) -> Result<(bool, Option<String>), ApiClientError> {
        let request = self
            .client
            .get_profile()
            .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()));

        let response = request.send().await?;
        let mcp_config = response
            .profile()
            .opt_in_features()
            .and_then(|features| features.mcp_configuration());

        let mcp_enabled = mcp_config.is_none_or(|config| matches!(config.toggle(), OptInFeatureToggle::On));
        let registry_url = mcp_config.and_then(|config| config.mcp_registry_url().map(|s| s.to_string()));

        Ok((mcp_enabled, registry_url))
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

    pub async fn get_usage_limits(
        &self,
    ) -> Result<amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsOutput, ApiClientError> {
        self.client
            .get_usage_limits()
            .set_origin(Some(amzn_codewhisperer_client::types::Origin::KiroCli))
            .set_profile_arn(self.profile.as_ref().map(|p| p.arn.clone()))
            .send()
            .await
            .map_err(ApiClientError::GetUsageLimitsError)
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
            agent_continuation_id,
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
                .set_agent_continuation_id(agent_continuation_id)
                .agent_task_type(amzn_codewhisperer_streaming_client::types::AgentTaskType::Vibe)
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
                    let err = ConverseStreamError::new(
                        classify_error_kind(status_code, body, model_id_opt.as_deref(), &err),
                        Some(err),
                    )
                    .set_request_id(request_id)
                    .set_status_code(status_code);
                    record_send_error(err.clone());
                    Err(err)
                },
            }
        } else if let Some(client) = &self.mock_client {
            let mut new_events = client.lock().next().unwrap_or_default().clone();
            new_events.reverse();

            Ok(SendMessageOutput::Mock(new_events))
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
                    other => panic!("Unexpected value: {other:?}"),
                }
            }
            mock.push(stream);
        }

        self.mock_client = Some(Arc::new(Mutex::new(mock.into_iter())));
    }
}

fn is_custom_endpoint(database: &Database) -> bool {
    database.settings.get(Setting::ApiCodeWhispererService).is_some()
}

impl ApiClient {
    pub async fn new(
        env: &Env,
        fs: &Fs,
        database: &mut Database,
        endpoint: Option<Endpoint>,
    ) -> Result<Self, ApiClientError> {
        let real = RealApiClient::new(env, fs, database, endpoint).await?;
        Ok(Self {
            inner: ApiClientInner::Real(real),
        })
    }

    /// Create an IPC mock client for E2E testing.
    pub fn new_ipc_mock(registry: MockResponseRegistryHandle) -> Self {
        Self {
            inner: ApiClientInner::IpcMock(IpcMockApiClient::new(registry)),
        }
    }

    pub async fn send_telemetry_event(
        &self,
        telemetry_event: TelemetryEvent,
        user_context: UserContext,
        telemetry_enabled: bool,
        model: Option<String>,
    ) -> Result<(), ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => {
                c.send_telemetry_event(telemetry_event, user_context, telemetry_enabled, model)
                    .await
            },
            ApiClientInner::IpcMock(c) => {
                c.send_telemetry_event(telemetry_event, user_context, telemetry_enabled, model)
                    .await
            },
        }
    }

    pub async fn list_available_profiles(&self) -> Result<Vec<AuthProfile>, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.list_available_profiles().await,
            ApiClientInner::IpcMock(c) => c.list_available_profiles().await,
        }
    }

    pub async fn list_available_models(&self) -> Result<ModelListResult, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.list_available_models().await,
            ApiClientInner::IpcMock(c) => c.list_available_models().await,
        }
    }

    pub async fn list_available_models_cached(&self) -> Result<ModelListResult, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.list_available_models_cached().await,
            ApiClientInner::IpcMock(c) => c.list_available_models_cached().await,
        }
    }

    pub async fn invalidate_model_cache(&self) {
        match &self.inner {
            ApiClientInner::Real(c) => c.invalidate_model_cache().await,
            ApiClientInner::IpcMock(c) => c.invalidate_model_cache().await,
        }
    }

    pub async fn get_available_models(&self, region: &str) -> Result<ModelListResult, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.get_available_models(region).await,
            ApiClientInner::IpcMock(c) => c.get_available_models(region).await,
        }
    }

    pub async fn is_mcp_enabled(&self) -> Result<bool, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.is_mcp_enabled().await,
            ApiClientInner::IpcMock(c) => c.is_mcp_enabled().await,
        }
    }

    pub async fn get_mcp_config(&self) -> Result<(bool, Option<String>), ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.get_mcp_config().await,
            ApiClientInner::IpcMock(c) => c.get_mcp_config().await,
        }
    }

    pub async fn create_subscription_token(&self) -> Result<CreateSubscriptionTokenOutput, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.create_subscription_token().await,
            ApiClientInner::IpcMock(c) => c.create_subscription_token().await,
        }
    }

    pub async fn get_usage_limits(
        &self,
    ) -> Result<amzn_codewhisperer_client::operation::get_usage_limits::GetUsageLimitsOutput, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.get_usage_limits().await,
            ApiClientInner::IpcMock(c) => c.get_usage_limits().await,
        }
    }

    pub async fn send_message(
        &self,
        conversation: ConversationState,
    ) -> Result<SendMessageOutput, ConverseStreamError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.send_message(conversation).await,
            ApiClientInner::IpcMock(c) => c.send_message(conversation).await,
        }
    }

    /// Only meant for testing. Do not use outside of testing responses.
    pub fn set_mock_output(&mut self, json: serde_json::Value) {
        match &mut self.inner {
            ApiClientInner::Real(c) => c.set_mock_output(json),
            ApiClientInner::IpcMock(_) => panic!("set_mock_output not supported on IpcMock"),
        }
    }
}

fn classify_error_kind<T: ProvideErrorMetadata, R>(
    status_code: Option<u16>,
    body: &[u8],
    model_id_opt: Option<&str>,
    sdk_error: &error::SdkError<T, R>,
) -> ConverseStreamErrorKind {
    let contains = |haystack: &[u8], needle: &[u8]| haystack.windows(needle.len()).any(|v| v == needle);

    let is_throttling = status_code.is_some_and(|status| status == 429);
    let is_context_window_overflow = contains(body, b"Input is too long.");
    let is_model_unavailable = contains(body, b"INSUFFICIENT_MODEL_CAPACITY")
        // Legacy error response fallback
        || (model_id_opt.is_some()
            && status_code.is_some_and(|status| status == 500)
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

    ConverseStreamErrorKind::Unknown {
        // do not change - we currently use sdk_error_code for mapping from an arbitrary sdk error
        // to a reason code.
        reason_code: error::sdk_error_code(sdk_error),
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
    use bstr::ByteSlice;

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

        client.set_mock_output(serde_json::json!([["Hello!", " How can I", " assist you today?"]]));

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
                agent_continuation_id: None,
            })
            .await
            .unwrap();

        let mut output_content = String::new();
        while let Some(ChatResponseStream::AssistantResponseEvent { content }) = output.recv().await.unwrap() {
            output_content.push_str(&content);
        }
        assert_eq!(output_content, "Hello! How can I assist you today?");
    }

    #[test]
    fn test_classify_error_kind() {
        use aws_smithy_runtime_api::http::Response;
        use aws_smithy_types::body::SdkBody;

        use crate::api_client::error::{
            GenerateAssistantResponseError,
            SdkError,
        };

        let mock_sdk_error = || {
            SdkError::service_error(
                GenerateAssistantResponseError::unhandled("test"),
                Response::new(500.try_into().unwrap(), SdkBody::empty()),
            )
        };

        #[allow(clippy::type_complexity)]
        let test_cases: Vec<(Option<u16>, &[u8], Option<&str>, ConverseStreamErrorKind)> = vec![
            (
                Some(400),
                b"Input is too long.",
                None,
                ConverseStreamErrorKind::ContextWindowOverflow,
            ),
            (
                Some(500),
                b"INSUFFICIENT_MODEL_CAPACITY",
                Some("model-1"),
                ConverseStreamErrorKind::ModelOverloadedError,
            ),
            (
                Some(500),
                b"Encountered unexpectedly high load when processing the request, please try again.",
                Some("model-1"),
                ConverseStreamErrorKind::ModelOverloadedError,
            ),
            (
                Some(429),
                b"Rate limit exceeded",
                None,
                ConverseStreamErrorKind::Throttling,
            ),
            (
                Some(400),
                b"MONTHLY_REQUEST_COUNT exceeded",
                None,
                ConverseStreamErrorKind::MonthlyLimitReached,
            ),
            (
                Some(429),
                b"Input is too long.",
                None,
                ConverseStreamErrorKind::ContextWindowOverflow,
            ),
            (
                Some(429),
                b"INSUFFICIENT_MODEL_CAPACITY",
                Some("model-1"),
                ConverseStreamErrorKind::ModelOverloadedError,
            ),
            (
                Some(500),
                b"Encountered unexpectedly high load when processing the request, please try again.",
                None,
                ConverseStreamErrorKind::Unknown {
                    reason_code: "test".to_string(),
                },
            ),
            (
                Some(400),
                b"Encountered unexpectedly high load when processing the request, please try again.",
                Some("model-1"),
                ConverseStreamErrorKind::Unknown {
                    reason_code: "test".to_string(),
                },
            ),
            (Some(500), b"Some other error", None, ConverseStreamErrorKind::Unknown {
                reason_code: "test".to_string(),
            }),
        ];

        for (status_code, body, model_id, expected) in test_cases {
            let result = classify_error_kind(status_code, body, model_id, &mock_sdk_error());
            assert_eq!(
                std::mem::discriminant(&result),
                std::mem::discriminant(&expected),
                "expected '{}', got '{}' | status_code: {:?}, body: '{}', model_id: '{:?}'",
                expected,
                result,
                status_code,
                body.to_str_lossy(),
                model_id
            );
        }
    }
}
