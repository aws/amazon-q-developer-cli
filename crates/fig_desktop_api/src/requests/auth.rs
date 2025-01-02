use std::sync::{
    Arc,
    LazyLock,
};

use fig_auth::builder_id::{
    PollCreateToken,
    StartDeviceAuthorizationResponse,
    TokenType,
};
use fig_auth::pkce::{
    Client,
    PkceClient,
    PkceRegistration,
};
use fig_auth::secret_store::SecretStore;
use fig_proto::fig::auth_builder_id_poll_create_token_response::PollStatus;
use fig_proto::fig::auth_status_response::AuthKind;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    AuthBuilderIdPollCreateTokenRequest,
    AuthBuilderIdPollCreateTokenResponse,
    AuthBuilderIdStartDeviceAuthorizationRequest,
    AuthBuilderIdStartDeviceAuthorizationResponse,
    AuthCancelPkceAuthorizationRequest,
    AuthCancelPkceAuthorizationResponse,
    AuthFinishPkceAuthorizationRequest,
    AuthFinishPkceAuthorizationResponse,
    AuthStartPkceAuthorizationRequest,
    AuthStartPkceAuthorizationResponse,
    AuthStatusRequest,
    AuthStatusResponse,
};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{
    Receiver,
    Sender,
    channel,
};
use tracing::{
    debug,
    error,
};

use super::RequestResult;
use crate::kv::KVStore;

static PKCE_REGISTRATION: LazyLock<Arc<PkceState<Client>>> = LazyLock::new(PkceState::new);

const BUILDER_ID_DATA_KEY: &str = "builder-id-data";

#[derive(Debug)]
enum PkceError {
    InvalidRequestId,
    RegistrationCancelled,
    AuthError(fig_auth::Error),
}

impl std::error::Error for PkceError {}

impl std::fmt::Display for PkceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            PkceError::InvalidRequestId => write!(f, "Invalid request id"),
            PkceError::RegistrationCancelled => write!(f, "Registration was cancelled"),
            PkceError::AuthError(ref err) => write!(f, "Error occurred during authorization: {err}"),
        }
    }
}

impl From<fig_auth::Error> for PkceError {
    fn from(value: fig_auth::Error) -> Self {
        Self::AuthError(value)
    }
}

/// Represents the state of potentially multiple concurrent PKCE authorization attempts. Intended to
/// be used as a global variable.
///
/// # Background
/// PKCE is weird - to handle the OAuth callback from the authorization server, we have to host a
/// local HTTP server. Therefore, in the case of some malformed client sending lots of registration
/// requests, we'd ideally only want one server to be hosted at a time. This type handles the logic
/// for ensuring that requests won't exhaust or leak system resources by enforcing a single
/// authorization attempt at a time.
#[derive(Debug)]
struct PkceState<T> {
    /// The active authorization request id.
    request_id: Mutex<String>,

    /// [Sender] for new authorization attempts for the global static task to receive.
    new_tx: Sender<(String, PkceRegistration, T, Option<SecretStore>)>,

    /// [Sender] for completed PKCE authorizations. Created per authorization attempt.
    finished_tx: Mutex<Sender<(String, Result<(), fig_auth::Error>)>>,

    /// [Receiver] for completed PKCE authorizations. Created per authorization attempt.
    finished_rx: Mutex<Receiver<(String, Result<(), fig_auth::Error>)>>,

    /// [Sender] for cancelling any potentially ongoing PKCE authorizations.
    cancel_tx: Mutex<Sender<()>>,
}

impl<T: PkceClient + Send + Sync + 'static> PkceState<T> {
    /// Creates a new [PkceState]. A tokio task is spawned for handling and completing incoming
    /// [PkceRegistration].
    fn new() -> Arc<Self> {
        let (new_tx, mut new_rx) = channel::<(String, PkceRegistration, T, Option<SecretStore>)>(1);
        let (finished_tx, finished_rx) = channel(1);
        let (cancel_tx, mut cancel_rx) = channel(1);

        let new_tx_clone = new_tx.clone();
        let pkce_state = Arc::new(PkceState {
            request_id: Mutex::new("".into()),
            new_tx,
            finished_tx: Mutex::new(finished_tx),
            finished_rx: Mutex::new(finished_rx),
            cancel_tx: Mutex::new(cancel_tx),
        });
        let pkce_state_clone = Arc::clone(&pkce_state);
        tokio::spawn(async move {
            let registration_tx = new_tx_clone;
            let pkce_state = pkce_state_clone;
            while let Some((auth_request_id, registration, client, secret_store)) = new_rx.recv().await {
                while cancel_rx.try_recv().is_ok() {
                    debug!("Ignoring buffered PKCE cancellation attempt");
                }

                tokio::select! {
                    // We received a new registration while waiting for the current one to complete,
                    // so resend it back around the loop.
                    Some(new_registration) = new_rx.recv() => {
                        debug!(
                            "Received a new registration for request_id: {} while already processing: {}",
                            new_registration.0,
                            auth_request_id
                        );
                        if let Err(err) = registration_tx.send(new_registration).await {
                            error!(?err, "Error attempting to reprocess registration");
                        }
                    }
                    // Registration successfully finished, send back the result.
                    result = registration.finish(&client, secret_store.as_ref()) => {
                        if let Err(err) = pkce_state
                            .finished_tx
                            .lock()
                            .await
                            .send((auth_request_id, result))
                            .await
                        {
                            error!(?err, "unknown error occurred finishing PKCE registration");
                        }
                    }
                    _ = cancel_rx.recv() => {
                        debug!("Cancelling current registration for request_id: {}", auth_request_id);
                        if let Err(err) = pkce_state
                            .finished_tx
                            .lock()
                            .await
                            .send((auth_request_id, Err(fig_auth::Error::OAuthCustomError("cancelled".into()))))
                            .await
                        {
                            error!(?err, "unknown error occurred cancelling PKCE registration");
                        }
                    }
                }
            }
        });

        pkce_state
    }

    /// Accepts a [PkceRegistration] and the [PkceClient] to use for finishing the registration,
    /// and associated [SecretStore] (if any) to store the access and refresh tokens to.
    /// Returns a request ID to pass to [Self::finish_registration].
    async fn start_registration(
        &self,
        registration: PkceRegistration,
        client: T,
        secret_store: Option<SecretStore>,
    ) -> String {
        let (registration_finished_tx, registration_finished_rx) = channel(1);

        // Drop the current sender, which should unlock the current rx.recv() call (if any).
        *self.finished_tx.lock().await = registration_finished_tx;
        *self.finished_rx.lock().await = registration_finished_rx;

        let uuid = uuid::Uuid::new_v4().to_string();
        (self.request_id.lock().await).clone_from(&uuid);
        self.new_tx
            .send((uuid.clone(), registration, client, secret_store))
            .await
            .unwrap();
        uuid
    }

    async fn finish_registration(&self, request_id: &str) -> Result<(), PkceError> {
        if *self.request_id.lock().await != request_id {
            return Err(PkceError::InvalidRequestId);
        }
        match self.finished_rx.lock().await.recv().await {
            Some((finished_request_id, result)) => {
                if finished_request_id != request_id {
                    return Err(PkceError::InvalidRequestId);
                }
                result.map_err(|e| e.into())
            },
            // Sender was dropped.
            None => Err(PkceError::RegistrationCancelled),
        }
    }

    /// Cancel an ongoing PKCE registration, if one is present.
    async fn cancel_registration(&self) {
        self.cancel_tx.lock().await.try_send(()).ok();
    }
}

pub async fn status(_request: AuthStatusRequest) -> RequestResult {
    let token = fig_auth::builder_id_token().await;
    Ok(ServerOriginatedSubMessage::AuthStatusResponse(AuthStatusResponse {
        authed: matches!(token, Ok(Some(_))),
        auth_kind: match &token {
            Ok(Some(auth)) => match auth.token_type() {
                TokenType::BuilderId => Some(AuthKind::BuilderId.into()),
                TokenType::IamIdentityCenter => Some(AuthKind::IamIdentityCenter.into()),
            },
            _ => None,
        },
        start_url: match &token {
            Ok(Some(auth)) => auth.start_url.clone(),
            _ => None,
        },
        region: match &token {
            Ok(Some(auth)) => auth.region.clone(),
            _ => None,
        },
    })
    .into())
}

pub async fn start_pkce_authorization(
    AuthStartPkceAuthorizationRequest { issuer_url, region }: AuthStartPkceAuthorizationRequest,
) -> RequestResult {
    if issuer_url.is_some() != region.is_some() {
        return Err("start_url and region must both be specified or both be omitted".into());
    }
    let secret_store = SecretStore::new()
        .await
        .map_err(|err| format!("Failed to load secret store: {err}"))?;
    let (client, registration) = fig_auth::pkce::start_pkce_authorization(issuer_url, region)
        .await
        .map_err(|err| format!("Unable to start PKCE authorization: {err}"))?;
    let url = registration.url.clone();
    let auth_request_id = PKCE_REGISTRATION
        .start_registration(registration, client, Some(secret_store))
        .await;
    let response = ServerOriginatedSubMessage::AuthStartPkceAuthorizationResponse(AuthStartPkceAuthorizationResponse {
        auth_request_id,
        url,
    });
    Ok(response.into())
}

pub async fn finish_pkce_authorization(
    AuthFinishPkceAuthorizationRequest { auth_request_id }: AuthFinishPkceAuthorizationRequest,
) -> RequestResult {
    PKCE_REGISTRATION
        .finish_registration(&auth_request_id)
        .await
        .map_err(|err| format!("{}", err))?;

    fig_telemetry::send_user_logged_in().await;
    Ok(ServerOriginatedSubMessage::AuthFinishPkceAuthorizationResponse(AuthFinishPkceAuthorizationResponse {}).into())
}

pub async fn cancel_pkce_authorization(_: AuthCancelPkceAuthorizationRequest) -> RequestResult {
    PKCE_REGISTRATION.cancel_registration().await;
    Ok(ServerOriginatedSubMessage::AuthCancelPkceAuthorizationResponse(AuthCancelPkceAuthorizationResponse {}).into())
}

pub async fn builder_id_start_device_authorization(
    AuthBuilderIdStartDeviceAuthorizationRequest { start_url, region }: AuthBuilderIdStartDeviceAuthorizationRequest,
    ctx: &impl KVStore,
) -> RequestResult {
    if start_url.is_some() != region.is_some() {
        return Err("start_url and region must both be specified or both be omitted".into());
    }

    let secret_store = SecretStore::new()
        .await
        .map_err(|err| format!("Failed to load secret store: {err}"))?;

    let builder_init: StartDeviceAuthorizationResponse =
        fig_auth::builder_id::start_device_authorization(&secret_store, start_url, region)
            .await
            .map_err(|err| format!("Failed to init auth: {err}"))?;

    let uuid = uuid::Uuid::new_v4().to_string();

    ctx.set(&[BUILDER_ID_DATA_KEY, &uuid], &builder_init).unwrap();

    let response = ServerOriginatedSubMessage::AuthBuilderIdStartDeviceAuthorizationResponse(
        AuthBuilderIdStartDeviceAuthorizationResponse {
            auth_request_id: uuid,
            code: builder_init.user_code,
            url: builder_init.verification_uri_complete,
            expires_in: builder_init.expires_in,
            interval: builder_init.interval,
        },
    );

    Ok(response.into())
}

pub async fn builder_id_poll_create_token(
    AuthBuilderIdPollCreateTokenRequest { auth_request_id }: AuthBuilderIdPollCreateTokenRequest,
    ctx: &impl KVStore,
) -> RequestResult {
    let secret_store = SecretStore::new()
        .await
        .map_err(|err| format!("Failed to load secret store: {err}"))?;

    let builder_init: StartDeviceAuthorizationResponse =
        ctx.get(&[BUILDER_ID_DATA_KEY, &auth_request_id]).unwrap().unwrap();

    let response = match fig_auth::builder_id::poll_create_token(
        &secret_store,
        builder_init.device_code,
        Some(builder_init.start_url),
        Some(builder_init.region),
    )
    .await
    {
        PollCreateToken::Pending => AuthBuilderIdPollCreateTokenResponse {
            status: PollStatus::Pending.into(),
            error: None,
            error_verbose: None,
        },
        PollCreateToken::Complete(_) => {
            fig_telemetry::send_user_logged_in().await;
            AuthBuilderIdPollCreateTokenResponse {
                status: PollStatus::Complete.into(),
                error: None,
                error_verbose: None,
            }
        },
        PollCreateToken::Error(err) => AuthBuilderIdPollCreateTokenResponse {
            status: PollStatus::Error.into(),
            error: Some(err.to_string()),
            error_verbose: Some(err.to_verbose_string()),
        },
    };

    Ok(ServerOriginatedSubMessage::AuthBuilderIdPollCreateTokenResponse(response).into())
}

#[cfg(test)]
mod tests {
    use fig_auth::AMZN_START_URL;
    use fig_auth::pkce::*;

    use super::*;

    struct TestPkceClient;

    #[async_trait::async_trait]
    impl PkceClient for TestPkceClient {
        fn scopes() -> Vec<String> {
            vec!["scope:1".into(), "scope:2".into()]
        }

        async fn register_client(&self, _: String, _: String) -> Result<RegisterClientResponse, fig_auth::Error> {
            Ok(RegisterClientResponse {
                output: RegisterClientOutput::builder()
                    .client_id("test_client_id")
                    .client_secret("test_client_secret")
                    .build(),
            })
        }

        async fn create_token(&self, _: CreateTokenArgs) -> Result<CreateTokenResponse, fig_auth::Error> {
            Ok(CreateTokenResponse {
                output: CreateTokenOutput::builder().build(),
            })
        }
    }

    fn test_region() -> Region {
        Region::new("us-east-1")
    }

    fn test_issuer_url() -> String {
        AMZN_START_URL.into()
    }

    async fn send_test_auth_code(redirect_uri: String, state: String) -> reqwest::Result<reqwest::Response> {
        reqwest::get(format!("{}/?code={}&state={}", redirect_uri, "code", state)).await
    }

    #[tokio::test]
    async fn pkce_starts_and_finishes_registration_successfully() {
        let pkce_state = PkceState::new();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, test_region(), test_issuer_url(), None)
            .await
            .unwrap();

        // Verifies that a start and end registration request completes successfully for a given
        // auth_request_id.
        let (uri, state) = (registration.redirect_uri.clone(), registration.state.clone());
        let request_id = pkce_state.start_registration(registration, client, None).await;
        send_test_auth_code(uri, state).await.unwrap();
        assert!(pkce_state.finish_registration(&request_id).await.is_ok());
    }

    #[tokio::test]
    async fn pkce_two_start_requests_cancels_the_first_request() {
        let pkce_state = PkceState::new();
        let client_one = TestPkceClient {};
        let registration_one = PkceRegistration::register(&client_one, test_region(), test_issuer_url(), None)
            .await
            .unwrap();
        let client_two = TestPkceClient {};
        let registration_two = PkceRegistration::register(&client_two, test_region(), test_issuer_url(), None)
            .await
            .unwrap();

        // Start the two registrations. This should start the local HTTP redirect server.
        let (uri_one, state_one) = (registration_one.redirect_uri.clone(), registration_one.state.clone());
        let (uri_two, state_two) = (registration_two.redirect_uri.clone(), registration_two.state.clone());
        let id_one = pkce_state.start_registration(registration_one, client_one, None).await;
        let id_two = pkce_state.start_registration(registration_two, client_two, None).await;

        // Send the oauth code/state params to the local redirect uris.
        // Assert that the first request fails to connect, and the second succeeds.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            send_test_auth_code(uri_one, state_one)
                .await
                .is_err_and(|e| e.is_connect())
        );
        send_test_auth_code(uri_two, state_two)
            .await
            .expect("second request should succeed");

        // First registration ends in error, second succeeds.
        let result_one = pkce_state.finish_registration(&id_one).await;
        let result_two = pkce_state.finish_registration(&id_two).await;
        assert!(matches!(result_one, Err(PkceError::InvalidRequestId)));
        assert!(result_two.is_ok());
    }

    #[tokio::test]
    async fn pkce_start_and_finish_gets_cancelled_by_new_request() {
        let pkce_state = PkceState::new();
        let client_one = TestPkceClient {};
        let registration_one = PkceRegistration::register(&client_one, test_region(), test_issuer_url(), None)
            .await
            .unwrap();
        let client_two = TestPkceClient {};
        let registration_two = PkceRegistration::register(&client_two, test_region(), test_issuer_url(), None)
            .await
            .unwrap();

        // Start and finish the first registration before the second one starts.
        let id_one = pkce_state.start_registration(registration_one, client_one, None).await;
        let pkce_state_clone = Arc::clone(&pkce_state);
        tokio::spawn(async move {
            // Needs to happen after finish is called on the first one.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            pkce_state_clone
                .start_registration(registration_two, client_two, None)
                .await;
        });
        let result_one = pkce_state.finish_registration(&id_one).await;
        assert!(matches!(result_one, Err(PkceError::RegistrationCancelled)));
    }

    #[tokio::test]
    async fn test_pkce_state_is_cancellable() {
        tracing_subscriber::fmt::init();

        let pkce_state = PkceState::new();

        // Cancelling before we can finish the registration.
        {
            pkce_state.cancel_registration().await;
            pkce_state.cancel_registration().await; // works multiple times

            let client = TestPkceClient {};
            let registration = PkceRegistration::register(&client, test_region(), test_issuer_url(), None)
                .await
                .unwrap();
            let (uri, state) = (registration.redirect_uri.clone(), registration.state.clone());
            let request_id = pkce_state.start_registration(registration, client, None).await;
            send_test_auth_code(uri, state).await.unwrap();
            assert!(pkce_state.finish_registration(&request_id).await.is_ok());
        }

        // Cancelling closes the server
        {
            let client = TestPkceClient {};
            let registration = PkceRegistration::register(&client, test_region(), test_issuer_url(), None)
                .await
                .unwrap();
            let (uri, state) = (registration.redirect_uri.clone(), registration.state.clone());
            let _ = pkce_state.start_registration(registration, client, None).await;
            // Give time for the HTTP server to be hosted
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            pkce_state.cancel_registration().await;
            assert!(send_test_auth_code(uri, state).await.is_err());
        }
    }
}
