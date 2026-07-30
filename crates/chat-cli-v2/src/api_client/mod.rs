pub mod attempt_header;
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
pub mod token_type_interceptor;

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
use amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseError;
use amzn_codewhisperer_streaming_client::types::ValidationExceptionReason;
use aws_config::Region;
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_credential_types::Credentials;
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

use crate::api_client::attempt_header::AttemptHeaderInterceptor;
use crate::api_client::delay_interceptor::DelayTrackingInterceptor;
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
};
use crate::api_client::opt_out::OptOutInterceptor;
use crate::api_client::send_message_output::{
    MockStreamItem,
    SendMessageOutput,
    record_request,
    record_send_error,
};
use crate::api_client::token_type_interceptor::{
    AuthMode,
    TokenTypeInterceptor,
};
use crate::auth::UnifiedBearerResolver;
use crate::auth::external_idp::ExternalIdpToken;
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

const DEFAULT_TIMEOUT_DURATION: Duration = Duration::from_secs(600);

pub const MAX_RETRY_DELAY_DURATION: Duration = Duration::from_secs(10);

/// Max attempts for API client requests (control-plane and streaming).
const MAX_ATTEMPTS: u32 = 3;

/// Profile ARN for BuilderId (free tier) users who have no IAM IdC profile stored in the DB.
pub(crate) const BUILDER_ID_PROFILE_ARN: &str = "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX";

/// Single entry point for all profile ARN resolution.
///
/// Encapsulates three cases:
/// - BuilderId users: a hardcoded well-known ARN set at construction
/// - IdC users: ARN loaded from the local DB at construction
/// - IdC users with missing DB entry: ARN lazily resolved via `list_available_profiles` on first
///   API call, cached so the API is called at most once per session
#[derive(Clone, Debug)]
struct ProfileResolver {
    resolved: Arc<std::sync::Mutex<Option<AuthProfile>>>,
    /// When true, skip the lazy `list_available_profiles` fallback in `require_arn`.
    /// Social users always have their profile ARN stored at login time, so hitting
    /// the slow path means something went wrong — we should error, not call the API.
    skip_lazy_resolve: bool,
}

impl ProfileResolver {
    fn new(initial: Option<AuthProfile>) -> Self {
        Self {
            resolved: Arc::new(std::sync::Mutex::new(initial)),
            skip_lazy_resolve: false,
        }
    }

    fn for_builder_id() -> Self {
        Self::new(Some(AuthProfile {
            arn: BUILDER_ID_PROFILE_ARN.to_string(),
            profile_name: "BuilderId".to_string(),
        }))
    }

    fn for_social(profile: Option<AuthProfile>) -> Self {
        Self {
            resolved: Arc::new(std::sync::Mutex::new(profile)),
            skip_lazy_resolve: true,
        }
    }

    fn arn_if_known(&self) -> Option<String> {
        self.resolved.lock().unwrap().as_ref().map(|p| p.arn.clone())
    }

    fn profile_if_known(&self) -> Option<AuthProfile> {
        self.resolved.lock().unwrap().clone()
    }

    fn is_known(&self) -> bool {
        self.resolved.lock().unwrap().is_some()
    }

    fn set(&self, profile: AuthProfile) {
        *self.resolved.lock().unwrap() = Some(profile);
    }

    /// Returns the profile ARN, calling `list_profiles` lazily if not yet resolved.
    /// The result is cached so `list_profiles` is called at most once per session.
    async fn require_arn<F, Fut>(&self, list_profiles: F) -> Result<String, ApiClientError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Vec<AuthProfile>, ApiClientError>>,
    {
        // Fast path: check cache (sync, no await)
        if let Some(arn) = self.arn_if_known() {
            return Ok(arn);
        }

        // Social users should always have a profile ARN from login.
        // If we get here, something went wrong — don't call list_available_profiles.
        if self.skip_lazy_resolve {
            tracing::error!(
                "profileArn missing for social user — this should not happen. The profile ARN should have been stored at login time."
            );
            return Err(ApiClientError::Other(
                "profileArn is required but was not found for social login. Please log out and log in again.".into(),
            ));
        }

        // Slow path (at most once per session): resolve via list_available_profiles
        tracing::info!("profileArn missing, attempting lazy resolution via list_available_profiles");
        let profiles = list_profiles().await?;

        if let Some(profile) = profiles.into_iter().next() {
            tracing::info!(arn = %profile.arn, "Lazily resolved profileArn from list_available_profiles");
            let mut guard = self.resolved.lock().unwrap();
            if guard.is_none() {
                *guard = Some(profile);
            }
            Ok(guard.as_ref().unwrap().arn.clone())
        } else {
            Err(ApiClientError::Other(
                "No profiles available. Your administrator has not granted you access to Kiro. \
                 Please contact your organization's administrator to be added to a Kiro profile."
                    .into(),
            ))
        }
    }
}

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
    telemetry_client: CodewhispererClient,
    streaming_client: Option<CodewhispererStreamingClient>,
    mock_client: Option<Arc<Mutex<std::vec::IntoIter<Vec<ChatResponseStream>>>>>,
    resolve_profile: ProfileResolver,
    model_cache: ModelCache,
    endpoint: Endpoint,
    auth_mode: AuthMode,
    retry_warnings: delay_interceptor::RetryWarningBuffer,
    request_attempts: delay_interceptor::RequestAttemptsTracker,
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

                // If a previous stream's receiver was dropped (e.g. agent cancelled),
                // the sender is stale — clear it so we can start a new stream.
                if let Some(ref tx) = state.stream_tx
                    && tx.is_closed()
                {
                    state.stream_tx = None;
                }
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
                        // The harness pushes a None terminator after every stream, including
                        // SendError-only streams. Consume that trailing terminator so it does
                        // not become a leading None for the next GetStream call.
                        if let Some(None) = state.buffer.front() {
                            state.buffer.pop_front();
                        }
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
        // Return mock models for testing.
        //
        // Two effort-schema shapes are exercised:
        //   - Claude family + qwen → `output_config.effort`
        //   - GPT family            → `reasoning.effort`
        // Amazon Nova has no effort schema at all.
        let output_config_schema = json_to_document(&serde_json::json!({
            "type": "object",
            "properties": {
                "output_config": {
                    "type": "object",
                    "properties": {
                        "effort": {
                            "type": "string",
                            "enum": ["low", "medium", "high", "xhigh", "max"]
                        }
                    }
                }
            }
        }));
        let reasoning_schema = json_to_document(&serde_json::json!({
            "type": "object",
            "properties": {
                "reasoning": {
                    "type": "object",
                    "properties": {
                        "effort": {
                            "type": "string",
                            "enum": ["low", "medium", "high", "xhigh"]
                        }
                    }
                }
            }
        }));

        let output_config_models = [
            ("claude-opus-4.7", "Claude Opus 4.7"),
            ("claude-opus-4.6", "Claude Opus 4.6"),
            ("claude-sonnet-4.6", "Claude Sonnet 4.6"),
            ("Auto", "Auto"),
            ("claude-sonnet-4.5", "Claude Sonnet 4.5"),
            ("claude-sonnet-4", "Claude Sonnet 4"),
            ("claude-haiku-4.5", "Claude Haiku 4.5"),
            ("claude-opus-4.5", "Claude Opus 4.5"),
            ("claude-sonnet-4.5-1m", "Claude Sonnet 4.5 1M"),
            ("qwen3-coder-480b", "Qwen3 Coder 480B"),
        ];
        let reasoning_models = [("gpt-5.1", "GPT-5.1")];

        let mut models: Vec<Model> = output_config_models
            .into_iter()
            .map(|(id, name)| {
                Model::builder()
                    .model_id(id)
                    .model_name(name)
                    .additional_model_request_fields_schema(output_config_schema.clone())
                    .build()
                    .unwrap()
            })
            .chain(reasoning_models.into_iter().map(|(id, name)| {
                Model::builder()
                    .model_id(id)
                    .model_name(name)
                    .additional_model_request_fields_schema(reasoning_schema.clone())
                    .build()
                    .unwrap()
            }))
            .collect();
        // Model without effort support (no additional_fields schema)
        models.push(
            Model::builder()
                .model_id("amazon-nova-pro")
                .model_name("Amazon Nova Pro")
                .build()
                .unwrap(),
        );
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
        Ok((true, None))
    }

    pub async fn get_governance_config(&self) -> Result<(bool, Option<String>, bool), ApiClientError> {
        Ok((true, None, true))
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

        // Check if using External IdP authentication
        let is_external_idp = ExternalIdpToken::load(database)
            .await
            .map(|t| t.is_some())
            .unwrap_or(false);

        // Determine auth mode: must match UnifiedBearerResolver priority.
        // Stored credentials take precedence; API key is only used as fallback.
        let auth_mode = if is_external_idp {
            AuthMode::ExternalIdp
        } else if crate::auth::is_builder_id_logged_in(database).await
            || crate::auth::social::is_social_logged_in(&*database).await
        {
            AuthMode::Normal
        } else if crate::util::env_var::get_api_key().is_some() {
            AuthMode::ApiKey
        } else {
            AuthMode::Normal
        };

        // Check if using Builder ID (free tier) — these users have no IAM IdC profile,
        // so we inject a hardcoded prod profile ARN for routing purposes.
        let is_builder_id = matches!(
            crate::auth::builder_id::BuilderIdToken::load(database, None).await,
            Ok(Some(ref t)) if matches!(t.token_type(), crate::auth::builder_id::TokenType::BuilderId)
        );

        let is_social = crate::auth::social::is_social_logged_in(database).await;

        let krs_endpoint = parse_endpoint_setting(database, Setting::ApiKrsService)
            .unwrap_or_else(|| Endpoint::krs_for_region(endpoint.region().as_ref()));
        let cps_endpoint = parse_endpoint_setting(database, Setting::ApiCpsService)
            .unwrap_or_else(|| Endpoint::cps_for_region(endpoint.region().as_ref()));

        let credentials = Credentials::new("xxx", "xxx", None, None, "xxx");
        let bearer_sdk_config = aws_config::defaults(behavior_version())
            .region(endpoint.region.clone())
            .credentials_provider(credentials)
            .timeout_config(timeout_config(database))
            .retry_config(retry_config())
            .load()
            .await;

        // Control plane client for CPS operations
        let client = CodewhispererClient::from_conf(
            amzn_codewhisperer_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                .interceptor(OptOutInterceptor::new(database))
                .interceptor(UserAgentOverrideInterceptor::new())
                .interceptor(TokenTypeInterceptor::new(auth_mode.clone()))
                .bearer_token_resolver(UnifiedBearerResolver)
                .app_name(app_name())
                .endpoint_resolver(StaticCodewhispererEndpointResolver::new(cps_endpoint.url().to_string()))
                .build(),
        );

        // Telemetry client — send_telemetry_event stays on legacy RTS
        let telemetry_client = CodewhispererClient::from_conf(
            amzn_codewhisperer_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                .interceptor(OptOutInterceptor::new(database))
                .interceptor(UserAgentOverrideInterceptor::new())
                .interceptor(TokenTypeInterceptor::new(auth_mode.clone()))
                .bearer_token_resolver(UnifiedBearerResolver)
                .app_name(app_name())
                .endpoint_resolver(StaticCodewhispererEndpointResolver::new(endpoint.url().to_string()))
                .build(),
        );

        let retry_warnings: delay_interceptor::RetryWarningBuffer = Arc::new(Mutex::new(Vec::new()));
        let request_attempts: delay_interceptor::RequestAttemptsTracker =
            Arc::new(std::sync::atomic::AtomicU32::new(0));

        if cfg!(test) && !is_integ_test() {
            let mut this = Self {
                client,
                telemetry_client,
                streaming_client: None,
                mock_client: None,
                resolve_profile: ProfileResolver::new(None),
                model_cache: Arc::new(RwLock::new(None)),
                endpoint: endpoint.clone(),
                auth_mode: auth_mode.clone(),
                retry_warnings: retry_warnings.clone(),
                request_attempts: request_attempts.clone(),
            };

            if let Some(json) = crate::util::env_var::get_mock_chat_response(env) {
                apply_mock_chat_response(&mut this, fs, &json).await;
            }

            return Ok(this);
        }

        // Use CodeWhisperer streaming client with bearer token
        let streaming_client = Some(CodewhispererStreamingClient::from_conf(
            amzn_codewhisperer_streaming_client::config::Builder::from(&bearer_sdk_config)
                .http_client(crate::aws_common::http_client::client())
                .interceptor(OptOutInterceptor::new(database))
                .interceptor(UserAgentOverrideInterceptor::new())
                .interceptor(DelayTrackingInterceptor::new(
                    retry_warnings.clone(),
                    request_attempts.clone(),
                    MAX_ATTEMPTS,
                ))
                .interceptor(TokenTypeInterceptor::new(auth_mode.clone()))
                .bearer_token_resolver(UnifiedBearerResolver)
                .app_name(app_name())
                .endpoint_resolver(StaticEndpointResolver::new(krs_endpoint.url().to_string()))
                .retry_config(retry_config())
                .retry_classifier(retry_classifier::QCliRetryClassifier::new())
                .stalled_stream_protection(stalled_stream_protection_config())
                .build(),
        ));

        let resolve_profile = if is_builder_id {
            ProfileResolver::for_builder_id()
        } else if is_social {
            let profile = match database.get_auth_profile() {
                Ok(profile) => profile,
                Err(err) => {
                    error!("Failed to get auth profile for social user: {err}");
                    None
                },
            };
            ProfileResolver::for_social(profile)
        } else {
            // Always populate the profile ARN, including for custom (non-prod) endpoints.
            // KRS now requires `profileArn` on every GenerateAssistantResponse call, so the
            // legacy RTS-only behavior of suppressing it on alpha/gamma endpoints would
            // cause ValidationException. Pre-prod testing accounts must have a profile
            // available either in the local DB or via list_available_profiles().
            let profile = match database.get_auth_profile() {
                Ok(profile) => profile,
                Err(err) => {
                    error!("Failed to get auth profile: {err}");
                    None
                },
            };
            ProfileResolver::new(profile)
        };

        let mut this = Self {
            client,
            telemetry_client,
            streaming_client,
            mock_client: None,
            resolve_profile,
            model_cache: Arc::new(RwLock::new(None)),
            endpoint,
            auth_mode,
            retry_warnings,
            request_attempts,
        };

        if let Some(json) = crate::util::env_var::get_mock_chat_response(env) {
            apply_mock_chat_response(&mut this, fs, &json).await;
        }

        Ok(this)
    }

    /// Returns the profile ARN, delegating all resolution logic to `resolve_profile`.
    async fn require_profile_arn(&self) -> Result<String, ApiClientError> {
        self.resolve_profile
            .require_arn(|| self.list_available_profiles())
            .await
    }

    /// Returns the profile ARN if available, or `None` for API key auth where
    /// profile ARN is not used.
    async fn optional_profile_arn(&self) -> Option<String> {
        if matches!(self.auth_mode, AuthMode::ApiKey) {
            return None;
        }
        self.require_profile_arn().await.ok()
    }

    /// Drain any retry warnings accumulated by the delay tracking interceptor.
    pub fn drain_retry_warnings(&self) -> Vec<delay_interceptor::RetryWarning> {
        std::mem::take(&mut *self.retry_warnings.lock())
    }

    /// Get a clone of the retry warning buffer for real-time polling.
    pub fn retry_warning_buffer(&self) -> delay_interceptor::RetryWarningBuffer {
        self.retry_warnings.clone()
    }

    /// Drain the max attempt count recorded by the delay tracking interceptor and reset
    /// the counter to 0 so the next `send_message` starts fresh.
    ///
    /// Returns `None` if no attempts have been recorded (e.g. if the request did not reach
    /// the interceptor — rare, but possible for early-failure paths like construction errors).
    /// Returns `Some(n)` where `n >= 1` when the interceptor ran at least once.
    pub fn drain_request_attempts(&self) -> Option<u32> {
        let n = self.request_attempts.swap(0, std::sync::atomic::Ordering::Relaxed);
        if n == 0 { None } else { Some(n) }
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

        self.telemetry_client
            .send_telemetry_event()
            .telemetry_event(telemetry_event)
            .user_context(user_context)
            .opt_out_preference(match telemetry_enabled {
                true => OptOutPreference::OptIn,
                false => OptOutPreference::OptOut,
            })
            .set_profile_arn(self.resolve_profile.arn_if_known())
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
            .set_profile_arn(self.optional_profile_arn().await);
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
        // Only cache when profile is already resolved — if still unresolved, the lazy path may
        // have picked a different ARN on retry, so we don't want to cache a potentially stale result.
        if self.resolve_profile.is_known() {
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

    /// Call GetProfile with no profile ARN to validate that this endpoint accepts the API key.
    pub async fn get_profile_for_api_key(
        &self,
    ) -> Result<amzn_codewhisperer_client::operation::get_profile::GetProfileOutput, ApiClientError> {
        self.client
            .get_profile()
            .send()
            .await
            .map_err(ApiClientError::GetProfileError)
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

    /// Get MCP and web tools governance config in a single GetProfile call.
    /// Returns `(mcp_enabled, registry_url, web_tools_enabled)`.
    pub async fn get_governance_config(&self) -> Result<(bool, Option<String>, bool), ApiClientError> {
        let governance_timeout = TimeoutConfig::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(15))
            .operation_attempt_timeout(Duration::from_secs(15))
            .operation_timeout(Duration::from_secs(30))
            .build();
        let response = self
            .client
            .get_profile()
            .set_profile_arn(self.optional_profile_arn().await)
            .customize()
            .config_override(amzn_codewhisperer_client::config::Builder::new().timeout_config(governance_timeout))
            .send()
            .await?;

        let opt_in = response.profile().opt_in_features();
        let mcp_config = opt_in.and_then(|f| f.mcp_configuration());
        let mcp_enabled = mcp_config.is_none_or(|c| matches!(c.toggle(), OptInFeatureToggle::On));
        let registry_url = mcp_config.and_then(|c| c.mcp_registry_url().map(|s| s.to_string()));
        let web_tools = opt_in.and_then(|f| f.web_tools());
        let web_tools_enabled = web_tools.is_none_or(|wt| matches!(wt.toggle(), OptInFeatureToggle::On));

        Ok((mcp_enabled, registry_url, web_tools_enabled))
    }

    /// Get MCP configuration including enabled status and registry URL
    pub async fn get_mcp_config(&self) -> Result<(bool, Option<String>), ApiClientError> {
        let (mcp_enabled, registry_url, _) = self.get_governance_config().await?;
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
            .set_profile_arn(self.optional_profile_arn().await)
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
            .set_profile_arn(self.optional_profile_arn().await)
            .send()
            .await
            .map_err(ApiClientError::GetUsageLimitsError)
    }

    pub async fn send_message(
        &self,
        conversation: ConversationState,
    ) -> Result<SendMessageOutput, ConverseStreamError> {
        debug!("Sending conversation: {:#?}", conversation);
        record_request(&conversation);

        let ConversationState {
            conversation_id,
            user_input_message,
            history,
            agent_continuation_id,
            additional_model_request_fields,
        } = conversation;

        let model_id_opt: Option<String> = user_input_message.model_id.clone();

        if let Some(client) = &self.mock_client {
            let mut new_events = client.lock().next().unwrap_or_default().clone();
            new_events.reverse();

            return Ok(SendMessageOutput::Mock(new_events));
        }

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
                .set_additional_model_request_fields(
                    additional_model_request_fields
                        .map(|v| crate::cli::chat::legacy::additional_fields::value_to_document(&v)),
                )
                .set_profile_arn(self.optional_profile_arn().await)
                .customize()
                .interceptor(AttemptHeaderInterceptor::new(MAX_ATTEMPTS))
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

    async fn invoke_mcp(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, ApiClientError> {
        let Some(streaming_client) = &self.streaming_client else {
            return Err(ApiClientError::Other("Streaming client not available".into()));
        };

        let params = aws_smithy_types::Document::Object(
            [
                (
                    "name".to_string(),
                    aws_smithy_types::Document::String(tool_name.to_string()),
                ),
                ("arguments".to_string(), json_to_document(&arguments)),
            ]
            .into_iter()
            .collect(),
        );

        let response = streaming_client
            .invoke_mcp()
            .jsonrpc("2.0")
            .id("1".into())
            .method(amzn_codewhisperer_streaming_client::types::McpMethod::ToolsCall)
            .params(params)
            .set_profile_arn(self.optional_profile_arn().await)
            .send()
            .await
            .map_err(|e| ApiClientError::Other(format!("Failed to invoke MCP: {e}")))?;

        if let Some(error) = response.error() {
            return Err(ApiClientError::Other(format!("MCP error: {error:?}")));
        }

        let result_doc = response
            .result()
            .ok_or_else(|| ApiClientError::Other("No result in MCP response".to_string()))?;

        // Check for isError field
        if let aws_smithy_types::Document::Object(map) = result_doc
            && let Some(aws_smithy_types::Document::Bool(true)) = map.get("isError")
        {
            let error_msg = map
                .get("content")
                .and_then(|c| match c {
                    aws_smithy_types::Document::Array(arr) => arr.first(),
                    _ => None,
                })
                .and_then(|item| match item {
                    aws_smithy_types::Document::Object(obj) => obj.get("text"),
                    _ => None,
                })
                .and_then(|text| match text {
                    aws_smithy_types::Document::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("Unknown error");
            return Err(ApiClientError::Other(format!("MCP tool failed: {error_msg}")));
        }

        let result_json = document_to_json(result_doc)?;

        // Handle both string and object responses
        let content_text: String = if let Some(result_str) = result_json.as_str() {
            let first_level: serde_json::Value = serde_json::from_str(result_str)
                .map_err(|e| ApiClientError::Other(format!("Failed to parse JSON: {e}")))?;

            first_level
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| ApiClientError::Other("No text in content array".to_string()))?
        } else if result_json.is_object() {
            result_json
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| ApiClientError::Other("No text in content array".to_string()))?
        } else {
            return Err(ApiClientError::Other(format!(
                "Unexpected result type: {result_json:?}"
            )));
        };

        serde_json::from_str(&content_text).map_err(|e| ApiClientError::Other(format!("Failed to parse result: {e}")))
    }

    /// Method to be used to reconstruct the client in the Os struct after auth changes.
    /// In the case that a user logs in with a non-commercial account the client associated
    /// with the Os struct will need to be reconstructed (as it is default initialized when
    /// there are no valid credentials in the secret store) to allow for subsequent calls from said
    /// client to succeed.
    pub async fn refresh_auth_profile(
        &mut self,
        env: &Env,
        fs: &Fs,
        database: &mut Database,
    ) -> Result<(), ApiClientError> {
        // Always refresh the profile, including for custom (non-prod) endpoints — KRS
        // requires profileArn on every GenerateAssistantResponse call regardless of stage.
        match database.get_auth_profile() {
            Ok(profile) => {
                tracing::debug!("Refreshed auth profile: {:?}", profile);

                if let Some(profile) = profile {
                    let endpoint = Endpoint::configured_value(database);
                    tracing::debug!("Recreating client with endpoint: {:?}", endpoint);

                    let new_client = Self::new(env, fs, database, Some(endpoint)).await?;
                    // Explicitly set profile in case Self::new() skipped the DB read (e.g., test mode)
                    new_client.resolve_profile.set(profile);
                    *self = new_client;
                }
            },
            Err(err) => {
                error!("Failed to refresh auth profile: {err}");
            },
        }
        Ok(())
    }

    /// If the profile is not yet resolved, resolves it via `list_available_profiles` and
    /// persists the result to the database so subsequent sessions skip the API call.
    pub async fn resolve_profile_if_missing(&self, database: &mut Database) -> Result<(), ApiClientError> {
        if matches!(self.auth_mode, AuthMode::ApiKey) || self.resolve_profile.is_known() {
            return Ok(());
        }
        if let Ok(_arn) = self.require_profile_arn().await
            && let Some(profile) = self.resolve_profile.profile_if_known()
            && let Err(e) = database.set_auth_profile(&profile)
        {
            tracing::warn!("Failed to persist resolved profile to database: {e}");
        }
        Ok(())
    }

    pub fn get_profile(&self) -> Option<AuthProfile> {
        self.resolve_profile.profile_if_known()
    }
}

fn json_to_document(value: &serde_json::Value) -> aws_smithy_types::Document {
    match value {
        serde_json::Value::Null => aws_smithy_types::Document::Null,
        serde_json::Value::Bool(b) => aws_smithy_types::Document::Bool(*b),
        serde_json::Value::Number(n) => {
            aws_smithy_types::Document::Number(aws_smithy_types::Number::Float(n.as_f64().unwrap_or(0.0)))
        },
        serde_json::Value::String(s) => aws_smithy_types::Document::String(s.clone()),
        serde_json::Value::Array(arr) => aws_smithy_types::Document::Array(arr.iter().map(json_to_document).collect()),
        serde_json::Value::Object(obj) => {
            aws_smithy_types::Document::Object(obj.iter().map(|(k, v)| (k.clone(), json_to_document(v))).collect())
        },
    }
}

#[allow(clippy::result_large_err)]
fn document_to_json(doc: &aws_smithy_types::Document) -> Result<serde_json::Value, ApiClientError> {
    match doc {
        aws_smithy_types::Document::Object(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                json_map.insert(k.clone(), document_to_json(v)?);
            }
            Ok(serde_json::Value::Object(json_map))
        },
        aws_smithy_types::Document::Array(arr) => {
            let json_arr: Result<Vec<_>, _> = arr.iter().map(document_to_json).collect();
            Ok(serde_json::Value::Array(json_arr?))
        },
        aws_smithy_types::Document::Number(n) => Ok(serde_json::Value::Number(
            serde_json::Number::from_f64(n.to_f64_lossy())
                .ok_or_else(|| ApiClientError::Other("Invalid number".into()))?,
        )),
        aws_smithy_types::Document::String(s) => Ok(serde_json::Value::String(s.clone())),
        aws_smithy_types::Document::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        aws_smithy_types::Document::Null => Ok(serde_json::Value::Null),
    }
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

    /// Drain any retry warnings accumulated by the delay tracking interceptor.
    /// Returns an empty vec for mock clients.
    pub fn drain_retry_warnings(&self) -> Vec<delay_interceptor::RetryWarning> {
        match &self.inner {
            ApiClientInner::Real(c) => c.drain_retry_warnings(),
            ApiClientInner::IpcMock(_) => Vec::new(),
        }
    }

    /// Get a clone of the retry warning buffer for real-time polling.
    /// Returns an empty buffer for mock clients.
    pub fn retry_warning_buffer(&self) -> delay_interceptor::RetryWarningBuffer {
        match &self.inner {
            ApiClientInner::Real(c) => c.retry_warning_buffer(),
            ApiClientInner::IpcMock(_) => Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Drain the number of attempts made for the most recent `send_message` call.
    /// Returns `None` for mock clients (no interceptor runs in IPC mocks).
    pub fn drain_request_attempts(&self) -> Option<u32> {
        match &self.inner {
            ApiClientInner::Real(c) => c.drain_request_attempts(),
            ApiClientInner::IpcMock(_) => None,
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

    pub async fn get_profile_for_api_key(
        &self,
    ) -> Result<amzn_codewhisperer_client::operation::get_profile::GetProfileOutput, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.get_profile_for_api_key().await,
            ApiClientInner::IpcMock(_) => Err(ApiClientError::Other(
                "get_profile_for_api_key not supported for IPC mock".into(),
            )),
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

    pub async fn get_governance_config(&self) -> Result<(bool, Option<String>, bool), ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.get_governance_config().await,
            ApiClientInner::IpcMock(c) => c.get_governance_config().await,
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

    /// Returns the endpoint URL.
    pub fn endpoint_url(&self) -> &str {
        match &self.inner {
            ApiClientInner::Real(c) => c.endpoint.url(),
            ApiClientInner::IpcMock(_) => "https://q.us-east-1.amazonaws.com",
        }
    }

    /// Returns the region.
    pub fn region(&self) -> &str {
        match &self.inner {
            ApiClientInner::Real(c) => c.endpoint.region().as_ref(),
            ApiClientInner::IpcMock(_) => "us-east-1",
        }
    }

    /// Invokes an MCP tool call.
    pub async fn invoke_mcp(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.invoke_mcp(tool_name, arguments).await,
            ApiClientInner::IpcMock(_) => Err(ApiClientError::Other("invoke_mcp not supported on IpcMock".into())),
        }
    }

    /// Recreate the API client with the current auth profile's endpoint.
    pub async fn refresh_auth_profile(
        &mut self,
        env: &Env,
        fs: &Fs,
        database: &mut Database,
    ) -> Result<(), ApiClientError> {
        match &mut self.inner {
            ApiClientInner::Real(c) => c.refresh_auth_profile(env, fs, database).await,
            ApiClientInner::IpcMock(_) => Ok(()),
        }
    }

    pub fn get_profile(&self) -> Option<AuthProfile> {
        match &self.inner {
            ApiClientInner::Real(c) => c.get_profile(),
            ApiClientInner::IpcMock(_) => None,
        }
    }

    pub async fn resolve_profile_if_missing(&self, database: &mut Database) -> Result<(), ApiClientError> {
        match &self.inner {
            ApiClientInner::Real(c) => c.resolve_profile_if_missing(database).await,
            ApiClientInner::IpcMock(_) => Ok(()),
        }
    }
}

/// Loads a JSON file at `path` and applies it as a mock response sequence on `client`.
/// Logs and returns without modifying `client` on read or parse failure - intended for
/// developer/test use via the `KIRO_MOCK_CHAT_RESPONSE` env var, where a misconfigured
/// path should not crash the binary.
async fn apply_mock_chat_response(client: &mut RealApiClient, fs: &Fs, path: &str) {
    let body = match fs.read_to_string(path).await {
        Ok(s) => s,
        Err(err) => {
            error!(?err, %path, "failed to read KIRO_MOCK_CHAT_RESPONSE file; skipping mock");
            return;
        },
    };
    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(err) => {
            error!(?err, %path, "failed to parse KIRO_MOCK_CHAT_RESPONSE as JSON; skipping mock");
            return;
        },
    };
    let Some(outer) = json.as_array() else {
        error!(%path, "KIRO_MOCK_CHAT_RESPONSE must be a JSON array of arrays of strings or objects; skipping mock");
        return;
    };
    let shape_ok = outer.iter().all(|inner| {
        inner
            .as_array()
            .is_some_and(|events| events.iter().all(|e| e.is_string() || e.is_object()))
    });
    if !shape_ok {
        error!(%path, "KIRO_MOCK_CHAT_RESPONSE must be a JSON array of arrays of strings or objects; skipping mock");
        return;
    }
    client.set_mock_output(json);
}

fn classify_error_kind<R>(
    status_code: Option<u16>,
    body: &[u8],
    model_id_opt: Option<&str>,
    sdk_error: &error::SdkError<GenerateAssistantResponseError, R>,
) -> ConverseStreamErrorKind {
    let contains = |haystack: &[u8], needle: &[u8]| haystack.windows(needle.len()).any(|v| v == needle);

    let is_throttling = status_code.is_some_and(|status| status == 429);
    let is_context_window_overflow = sdk_error.as_service_error().is_some_and(|e| match e {
        GenerateAssistantResponseError::ValidationError(err) => err
            .reason()
            .is_some_and(|r| r == &ValidationExceptionReason::ContentLengthExceedsThreshold),
        _ => false,
    });
    // String match exactly on the ContentLengthExceedsThreshold exception reason.
    //
    // TODO: Figure out why the rust client returns GenerateAssistantResponseError::Unhandled
    // instead of the well-modeled GenerateAssistantResponseError::ValidationError
    let is_context_window_overflow = is_context_window_overflow || contains(body, b"CONTENT_LENGTH_EXCEEDS_THRESHOLD");

    // INVALID_MODEL_ID is returned by the backend when the request's model id is not allowed in
    // the current inference path (e.g. removed or gated). We detect via both the modeled
    // ValidationError.reason() and the raw body — mirroring the ContentLengthExceedsThreshold
    // fallback above, since the SDK sometimes surfaces these as `Unhandled`.
    let is_invalid_model_id = sdk_error.as_service_error().is_some_and(|e| match e {
        GenerateAssistantResponseError::ValidationError(err) => err
            .reason()
            .is_some_and(|r| r == &ValidationExceptionReason::InvalidModelId),
        _ => false,
    });
    let is_invalid_model_id = is_invalid_model_id || contains(body, b"INVALID_MODEL_ID");

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

    if is_invalid_model_id {
        return ConverseStreamErrorKind::InvalidModelId {
            model_id: model_id_opt.map(|s| s.to_string()),
        };
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

    // Extract the user-friendly message from the service error metadata.
    // The SDK often returns Unhandled instead of the well-modeled ValidationError
    // (due to a mismatch between the Smithy model name "ValidationError" and the
    // wire type "ValidationException"), so we use ProvideErrorMetadata which works
    // for all variants including Unhandled.
    let service_message = sdk_error
        .as_service_error()
        .and_then(|e| e.meta().message().map(|s| s.to_string()));

    ConverseStreamErrorKind::Unknown {
        // do not change - we currently use sdk_error_code for mapping from an arbitrary sdk error
        // to a reason code.
        reason_code: error::sdk_error_code(sdk_error),
        message: service_message,
    }
}

/// Parse a custom endpoint override from a JSON setting.
/// Returns `Some(Endpoint)` if the setting is present and valid, `None` otherwise.
/// Logs an error if the setting exists but is malformed.
fn parse_endpoint_setting(database: &Database, setting: Setting) -> Option<Endpoint> {
    #[derive(serde::Deserialize)]
    struct EndpointOverride {
        endpoint: String,
        region: String,
    }

    let value = database.settings.get_value(setting)?;
    match serde_json::from_value::<EndpointOverride>(value.clone()) {
        Ok(o) if !o.endpoint.is_empty() && !o.region.is_empty() => Some(Endpoint {
            url: o.endpoint.into(),
            region: Region::new(o.region),
        }),
        Ok(_) => {
            tracing::error!(
                "Setting {:?} has empty endpoint or region — ignoring override",
                setting.as_ref()
            );
            None
        },
        Err(e) => {
            tracing::error!(
                "Setting {:?} is malformed (expected {{\"endpoint\": \"...\", \"region\": \"...\"}}): {} — ignoring override",
                setting.as_ref(),
                e
            );
            None
        },
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
        .with_max_attempts(MAX_ATTEMPTS)
        .with_max_backoff(MAX_RETRY_DELAY_DURATION)
}

pub fn stalled_stream_protection_config() -> StalledStreamProtectionConfig {
    StalledStreamProtectionConfig::enabled()
        .grace_period(Duration::from_secs(600))
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
                additional_model_request_fields: None,
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
            // ContextWindowOverflow checks
            (
                Some(400),
                b"Input is too long.",
                None,
                // Don't match on error message
                ConverseStreamErrorKind::Unknown {
                    reason_code: "test".to_string(),
                    message: None,
                },
            ),
            (
                Some(400),
                b"CONTENT_LENGTH_EXCEEDS_THRESHOLD",
                None,
                ConverseStreamErrorKind::ContextWindowOverflow,
            ),
            (
                Some(429),
                b"CONTENT_LENGTH_EXCEEDS_THRESHOLD",
                None,
                // Match on exact code
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
                    message: None,
                },
            ),
            (
                Some(400),
                b"Encountered unexpectedly high load when processing the request, please try again.",
                Some("model-1"),
                ConverseStreamErrorKind::Unknown {
                    reason_code: "test".to_string(),
                    message: None,
                },
            ),
            (Some(500), b"Some other error", None, ConverseStreamErrorKind::Unknown {
                reason_code: "test".to_string(),
                message: None,
            }),
            // InvalidModelId checks
            (
                Some(400),
                b"INVALID_MODEL_ID",
                Some("model-1"),
                ConverseStreamErrorKind::InvalidModelId {
                    model_id: Some("model-1".to_string()),
                },
            ),
            (
                Some(400),
                b"{\"reason\":\"INVALID_MODEL_ID\",\"message\":\"...\"}",
                None,
                ConverseStreamErrorKind::InvalidModelId { model_id: None },
            ),
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

    #[tokio::test]
    async fn test_profile_resolver_social_with_profile_returns_arn() {
        let resolver = ProfileResolver::for_social(Some(AuthProfile {
            arn: "arn:aws:iam::123456789012:profile/SocialProfile".to_string(),
            profile_name: "Social_Default_Profile".to_string(),
        }));

        let arn = resolver
            .require_arn(|| async {
                panic!("list_available_profiles should not be called for social users with a profile");
            })
            .await
            .unwrap();

        assert_eq!(arn, "arn:aws:iam::123456789012:profile/SocialProfile");
    }

    #[tokio::test]
    async fn test_profile_resolver_social_without_profile_errors() {
        let resolver = ProfileResolver::for_social(None);

        let result = resolver
            .require_arn(|| async {
                panic!("list_available_profiles should not be called for social users");
            })
            .await;

        assert!(result.is_err(), "social user without profile should error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("social login"),
            "error should mention social login, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_profile_resolver_new_without_profile_calls_list_profiles() {
        let resolver = ProfileResolver::new(None);

        let result = resolver
            .require_arn(|| async {
                Ok(vec![AuthProfile {
                    arn: "arn:aws:iam::123456789012:profile/Resolved".to_string(),
                    profile_name: "Resolved".to_string(),
                }])
            })
            .await
            .unwrap();

        assert_eq!(result, "arn:aws:iam::123456789012:profile/Resolved");
    }

    #[tokio::test]
    async fn test_profile_resolver_new_with_profile_skips_list_profiles() {
        let resolver = ProfileResolver::new(Some(AuthProfile {
            arn: "arn:aws:iam::123456789012:profile/Cached".to_string(),
            profile_name: "Cached".to_string(),
        }));

        let arn = resolver
            .require_arn(|| async {
                panic!("list_available_profiles should not be called when profile is cached");
            })
            .await
            .unwrap();

        assert_eq!(arn, "arn:aws:iam::123456789012:profile/Cached");
    }
}
