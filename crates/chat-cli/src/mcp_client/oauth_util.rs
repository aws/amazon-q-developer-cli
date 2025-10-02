use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use http::{
    HeaderMap,
    StatusCode,
};
use http_body_util::Full;
use hyper::Response;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use reqwest::Client;
use rmcp::service::{
    DynService,
    ServiceExt,
};
use rmcp::transport::auth::{
    AuthClient,
    OAuthClientConfig,
    OAuthState,
    OAuthTokenResponse,
};
use rmcp::transport::sse_client::SseClientConfig;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{
    AuthorizationManager,
    AuthorizationSession,
    SseClientTransport,
    StreamableHttpClientTransport,
};
use rmcp::{
    RoleClient,
    Service,
    serde_json,
};
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use tokio::sync::oneshot::Sender;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    info,
};
use url::Url;

use super::messenger::Messenger;
use crate::os::Os;
use crate::util::directories::{
    DirectoryError,
    get_mcp_auth_dir,
};

#[derive(Debug, thiserror::Error)]
pub enum OauthUtilError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] url::ParseError),
    #[error(transparent)]
    Auth(#[from] rmcp::transport::AuthError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Missing authorization manager")]
    MissingAuthorizationManager,
    #[error("Missing auth client when token refresh is needed")]
    MissingAuthClient,
    #[error(transparent)]
    OneshotRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Http(String),
    #[error("Malformed directory")]
    MalformDirectory,
    #[error("Missing credential")]
    MissingCredentials,
    #[error("Failed to create a running service after running through all fallbacks: {0}")]
    ServiceNotObtained(String),
    #[error("{0}")]
    SseTransport(String),
}

/// A guard that automatically cancels the cancellation token when dropped.
/// This ensures that the OAuth loopback server is properly cleaned up
/// when the guard goes out of scope.
struct LoopBackDropGuard {
    cancellation_token: CancellationToken,
}

impl Drop for LoopBackDropGuard {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

/// OAuth Authorization Server metadata for endpoint discovery
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OAuthMeta {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
}

/// This is modeled after [OAuthClientConfig]
/// It's only here because [OAuthClientConfig] does not implement Serialize and Deserialize
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Registration {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

impl From<OAuthClientConfig> for Registration {
    fn from(value: OAuthClientConfig) -> Self {
        Self {
            client_id: value.client_id,
            client_secret: value.client_secret,
            scopes: value.scopes,
            redirect_uri: value.redirect_uri,
        }
    }
}

/// A wrapper that manages an authenticated MCP client.
///
/// This struct wraps an `AuthClient` and provides access to OAuth credentials
/// for MCP server connections that require authentication. The credentials
/// are managed separately from this wrapper's lifecycle.
#[derive(Clone, Debug)]
pub struct AuthClientWrapper {
    pub cred_full_path: PathBuf,
    pub auth_client: AuthClient<Client>,
}

impl AuthClientWrapper {
    pub fn new(cred_full_path: PathBuf, auth_client: AuthClient<Client>) -> Self {
        Self {
            cred_full_path,
            auth_client,
        }
    }

    /// Refreshes token in memory using the registration read from when the auth client was
    /// spawned. This also persists the retrieved token
    pub async fn refresh_token(&self) -> Result<(), OauthUtilError> {
        let cred = self.auth_client.auth_manager.lock().await.refresh_token().await?;
        let parent_path = self.cred_full_path.parent().ok_or(OauthUtilError::MalformDirectory)?;
        tokio::fs::create_dir_all(parent_path).await?;

        let cred_as_bytes = serde_json::to_string_pretty(&cred)?;
        tokio::fs::write(&self.cred_full_path, &cred_as_bytes).await?;

        Ok(())
    }
}

pub fn get_default_scopes() -> &'static [&'static str] {
    &["openid", "email", "profile", "offline_access"]
}

enum TransportType {
    Http,
    Sse,
}

enum HttpServiceBuilderState {
    AttemptConnection(TransportType, bool),
    FailedBecauseTokenMightBeExpired,
    Exhausted,
}

pub type HttpRunningService = (
    rmcp::service::RunningService<RoleClient, Box<dyn DynService<RoleClient>>>,
    Option<AuthClientWrapper>,
);

pub struct HttpServiceBuilder<'a> {
    pub server_name: &'a str,
    pub os: &'a Os,
    pub url: &'a str,
    pub timeout: u64,
    pub scopes: &'a [String],
    pub headers: &'a HashMap<String, String>,
    pub messenger: &'a dyn Messenger,
}

impl<'a> HttpServiceBuilder<'a> {
    pub fn new(
        server_name: &'a str,
        os: &'a Os,
        url: &'a str,
        timeout: u64,
        scopes: &'a [String],
        headers: &'a HashMap<String, String>,
        messenger: &'a dyn Messenger,
    ) -> Self {
        Self {
            server_name,
            os,
            url,
            timeout,
            scopes,
            headers,
            messenger,
        }
    }

    pub async fn try_build<S: Service<RoleClient> + Clone>(
        self,
        service: &S,
    ) -> Result<HttpRunningService, OauthUtilError> {
        let HttpServiceBuilder {
            server_name,
            os,
            url,
            timeout,
            scopes,
            headers,
            messenger,
        } = self;

        let mut state = HttpServiceBuilderState::AttemptConnection(TransportType::Http, false);
        let cred_dir = get_mcp_auth_dir(os)?;
        let url = Url::from_str(url)?;
        let key = compute_key(&url);
        let cred_full_path = cred_dir.join(format!("{key}.token.json"));
        let reg_full_path = cred_dir.join(format!("{key}.registration.json"));
        let mut auth_client = None::<AuthClient<Client>>;

        let mut client_builder = reqwest::ClientBuilder::new().timeout(std::time::Duration::from_millis(timeout));
        if !headers.is_empty() {
            let headers = HeaderMap::try_from(headers).map_err(|e| OauthUtilError::Http(e.to_string()))?;
            client_builder = client_builder.default_headers(headers);
        };
        let reqwest_client = client_builder.build()?;

        // The probe request, like all other request, should adhere to the standards as per https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#sending-messages-to-the-server
        let probe_resp = reqwest_client
            .post(url.clone())
            .header("Accept", "application/json, text/event-stream")
            .send()
            .await;
        let is_probe_err = probe_resp.is_err();
        let is_status_401_or_403 = probe_resp
            .as_ref()
            .is_ok_and(|resp| resp.status() == StatusCode::UNAUTHORIZED || resp.status() == StatusCode::FORBIDDEN);

        let contains_auth_header = probe_resp.is_ok_and(|resp| {
            resp.headers().get("www-authenticate").is_some_and(|v| {
                let value_as_str = v.to_str();
                if let Ok(value) = value_as_str {
                    value.to_lowercase().contains("bearer")
                } else {
                    false
                }
            })
        });
        
        let needs_auth = is_probe_err || is_status_401_or_403 || contains_auth_header;

        // Here we attempt the following in the order they are presented:
        // 1. Build transport, first assume http on attempt one, sse on attempt two
        //   - If it fails and it needs auth, attempt to refresh token (#2)
        //   - If it fails and it does not need auth OR if it fails after a refresh, attempt sse (#3)
        // 2. Refresh token, go back to #1
        // 3. Attempt sse
        //   - If it fails, abort (because at this point we have run out of things to try, note that
        //     refreshing of token is agnostic to the type of transport)
        loop {
            match state {
                HttpServiceBuilderState::AttemptConnection(transport_type, has_refreshed) => {
                    if needs_auth {
                        let ac = match auth_client {
                            Some(ref auth_client) => auth_client.clone(),
                            None => {
                                let am = get_auth_manager(
                                    url.clone(),
                                    cred_full_path.clone(),
                                    reg_full_path.clone(),
                                    scopes,
                                    messenger,
                                )
                                .await?;

                                let ac = AuthClient::new(reqwest_client.clone(), am);
                                auth_client.replace(ac.clone());
                                ac
                            },
                        };

                        match transport_type {
                            TransportType::Http => {
                                let transport = StreamableHttpClientTransport::with_client(
                                    ac.clone(),
                                    StreamableHttpClientTransportConfig {
                                        uri: url.as_str().into(),
                                        allow_stateless: true,
                                        ..Default::default()
                                    },
                                );

                                match service.clone().into_dyn().serve(transport).await {
                                    Ok(service) => {
                                        let auth_client_wrapper = AuthClientWrapper::new(cred_full_path, ac);
                                        return Ok((service, Some(auth_client_wrapper)));
                                    },
                                    Err(e) => {
                                        if !has_refreshed {
                                            error!(
                                                "## mcp: http handshake attempt failed for {server_name}: {:?}. Attempting to refresh token",
                                                e
                                            );
                                            // first we'll try refreshing the token
                                            state = HttpServiceBuilderState::FailedBecauseTokenMightBeExpired;
                                        } else {
                                            error!(
                                                "## mcp: http handshake attempt failed for {server_name}: {:?}. Attempting sse",
                                                e
                                            );
                                            state =
                                                HttpServiceBuilderState::AttemptConnection(TransportType::Sse, true);
                                        }
                                    },
                                }
                            },
                            TransportType::Sse => {
                                let transport = SseClientTransport::start_with_client(ac.clone(), SseClientConfig {
                                    sse_endpoint: url.as_str().into(),
                                    ..Default::default()
                                })
                                .await
                                .map_err(|e| OauthUtilError::SseTransport(e.to_string()))?;

                                match service.clone().into_dyn().serve(transport).await {
                                    Ok(service) => {
                                        let auth_client_wrapper = AuthClientWrapper::new(cred_full_path, ac);
                                        return Ok((service, Some(auth_client_wrapper)));
                                    },
                                    Err(e) => {
                                        // at this point we would have already tried refreshing
                                        // we are out of things to try and should just fail
                                        error!(
                                            "## mcp: sse handshake attempted failed for {server_name}: {:?}. Aborting",
                                            e
                                        );
                                        state = HttpServiceBuilderState::Exhausted;
                                    },
                                }
                            },
                        }
                    } else {
                        info!(
                            "## mcp: No OAuth endpoints discovered for {server_name}, using unauthenticated transport"
                        );

                        match transport_type {
                            TransportType::Http => {
                                info!("## mcp: attempting open http handshake for {server_name}");
                                let transport = StreamableHttpClientTransport::with_client(
                                    reqwest_client.clone(),
                                    StreamableHttpClientTransportConfig {
                                        uri: url.as_str().into(),
                                        allow_stateless: true,
                                        ..Default::default()
                                    },
                                );

                                match service.clone().into_dyn().serve(transport).await {
                                    Ok(service) => return Ok((service, None)),
                                    Err(e) => {
                                        error!(
                                            "## mcp: open http handshake attempted failed for {server_name}: {:?}. Attempting sse",
                                            e
                                        );
                                        state = HttpServiceBuilderState::AttemptConnection(TransportType::Sse, false);
                                    },
                                }
                            },
                            TransportType::Sse => {
                                info!("## mcp: attempting open sse handshake for {server_name}");
                                let transport =
                                    SseClientTransport::start_with_client(reqwest_client.clone(), SseClientConfig {
                                        sse_endpoint: url.as_str().into(),
                                        ..Default::default()
                                    })
                                    .await
                                    .map_err(|e| OauthUtilError::SseTransport(e.to_string()))?;

                                match service.clone().into_dyn().serve(transport).await {
                                    Ok(service) => return Ok((service, None)),
                                    Err(e) => {
                                        error!(
                                            "## mcp: open sse handshake attempted failed for {server_name}: {:?}. Aborting",
                                            e
                                        );
                                        state = HttpServiceBuilderState::Exhausted;
                                    },
                                }
                            },
                        }
                    }
                },
                HttpServiceBuilderState::FailedBecauseTokenMightBeExpired => {
                    let auth_client_ref = auth_client.as_ref().ok_or(OauthUtilError::MissingAuthClient)?;
                    let auth_client_wrapper = AuthClientWrapper::new(cred_full_path.clone(), auth_client_ref.clone());
                    let refresh_res = auth_client_wrapper.refresh_token().await;

                    if let Err(e) = refresh_res {
                        error!("## mcp: token refresh attempt failed: {:?}", e);
                        info!("Retry for http transport failed {e}. Possible reauth needed");
                        // This could be because the refresh token is expired, in which
                        // case we would need to have user go through the auth flow
                        // again. We do this by deleting the cred
                        // and discarding the client to trigger a full auth flow
                        if cred_full_path.is_file() {
                            tokio::fs::remove_file(&cred_full_path).await?;
                        }

                        // we'll also need to remove the auth client to force a reauth when we go
                        // back to attempt the first step again
                        auth_client.take();
                    }

                    state = HttpServiceBuilderState::AttemptConnection(TransportType::Http, true);
                },
                HttpServiceBuilderState::Exhausted => {
                    return Err(OauthUtilError::ServiceNotObtained(
                        "Max number of retries exhausted".to_string(),
                    ));
                },
            }
        }
    }
}

async fn get_auth_manager(
    url: Url,
    cred_full_path: PathBuf,
    reg_full_path: PathBuf,
    scopes: &[String],
    messenger: &dyn Messenger,
) -> Result<AuthorizationManager, OauthUtilError> {
    let cred_as_bytes = tokio::fs::read(&cred_full_path).await;
    let reg_as_bytes = tokio::fs::read(&reg_full_path).await;
    let mut oauth_state = OAuthState::new(url, None).await?;

    match (cred_as_bytes, reg_as_bytes) {
        (Ok(cred_as_bytes), Ok(reg_as_bytes)) => {
            let token = serde_json::from_slice::<OAuthTokenResponse>(&cred_as_bytes)?;
            let reg = serde_json::from_slice::<Registration>(&reg_as_bytes)?;

            oauth_state.set_credentials(&reg.client_id, token).await?;

            debug!("## mcp: credentials set with cache");

            Ok(oauth_state
                .into_authorization_manager()
                .ok_or(OauthUtilError::MissingAuthorizationManager)?)
        },
        _ => {
            info!("Error reading cached credentials");
            debug!("## mcp: cache read failed. constructing auth manager from scratch");
            let (am, redirect_uri) = get_auth_manager_impl(oauth_state, scopes, messenger).await?;

            // Client registration is done in [start_authorization]
            // If we have gotten past that point that means we have the info to persist the
            // registration on disk.
            let (client_id, credentials) = am.get_credentials().await?;
            let reg = Registration {
                client_id,
                client_secret: None,
                scopes: get_default_scopes()
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                redirect_uri,
            };
            let reg_as_str = serde_json::to_string_pretty(&reg)?;
            let reg_parent_path = reg_full_path.parent().ok_or(OauthUtilError::MalformDirectory)?;
            tokio::fs::create_dir_all(reg_parent_path).await?;
            tokio::fs::write(reg_full_path, &reg_as_str).await?;

            let credentials = credentials.ok_or(OauthUtilError::MissingCredentials)?;

            let cred_parent_path = cred_full_path.parent().ok_or(OauthUtilError::MalformDirectory)?;
            tokio::fs::create_dir_all(cred_parent_path).await?;
            let reg_as_str = serde_json::to_string_pretty(&credentials)?;
            tokio::fs::write(cred_full_path, &reg_as_str).await?;

            Ok(am)
        },
    }
}

async fn get_auth_manager_impl(
    mut oauth_state: OAuthState,
    scopes: &[String],
    messenger: &dyn Messenger,
) -> Result<(AuthorizationManager, String), OauthUtilError> {
    let socket_addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let cancellation_token = tokio_util::sync::CancellationToken::new();
    let (tx, rx) = tokio::sync::oneshot::channel::<(String, String)>();

    let (actual_addr, _dg) = make_svc(tx, socket_addr, cancellation_token).await?;
    info!("Listening on local host port {:?} for oauth", actual_addr);

    let redirect_uri = format!("http://{}", actual_addr);
    let scopes_as_str = scopes.iter().map(String::as_str).collect::<Vec<_>>();
    let scopes_as_slice = scopes_as_str.as_slice();
    start_authorization(&mut oauth_state, scopes_as_slice, &redirect_uri).await?;

    let auth_url = oauth_state.get_authorization_url().await?;
    _ = messenger.send_oauth_link(auth_url).await;

    let (auth_code, csrf_token) = rx.await?;
    oauth_state.handle_callback(&auth_code, &csrf_token).await?;
    let am = oauth_state
        .into_authorization_manager()
        .ok_or(OauthUtilError::MissingAuthorizationManager)?;

    Ok((am, redirect_uri))
}

pub fn compute_key(rs: &Url) -> String {
    let mut hasher = Sha256::new();
    let input = format!("{}{}", rs.origin().ascii_serialization(), rs.path());
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// This is our own implementation of [OAuthState::start_authorization].
/// This differs from [OAuthState::start_authorization] by assigning our own client_id for DCR.
/// We need this because the SDK hardcodes their own client id. And some servers will use client_id
/// to identify if a client is even allowed to perform the auth handshake.
async fn start_authorization(
    oauth_state: &mut OAuthState,
    scopes: &[&str],
    redirect_uri: &str,
) -> Result<(), OauthUtilError> {
    // DO NOT CHANGE THIS
    // This string has significance as it is used for remote servers to identify us
    const CLIENT_ID: &str = "Q DEV CLI";

    let stub_cred = get_stub_credentials()?;
    oauth_state.set_credentials(CLIENT_ID, stub_cred).await?;

    // The setting of credentials would put the oauth state into authorize.
    if let OAuthState::Authorized(auth_manager) = oauth_state {
        // set redirect uri
        let config = OAuthClientConfig {
            client_id: CLIENT_ID.to_string(),
            client_secret: None,
            scopes: scopes.iter().map(|s| (*s).to_string()).collect(),
            redirect_uri: redirect_uri.to_string(),
        };

        // try to dynamic register client
        let config = match auth_manager.register_client(CLIENT_ID, redirect_uri).await {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Dynamic registration failed: {}", e);
                // fallback to default config
                config
            },
        };
        // reset client config
        auth_manager.configure_client(config)?;
        let auth_url = auth_manager.get_authorization_url(scopes).await?;

        let mut stub_auth_manager = AuthorizationManager::new("http://localhost").await?;
        std::mem::swap(auth_manager, &mut stub_auth_manager);

        let session = AuthorizationSession {
            auth_manager: stub_auth_manager,
            auth_url,
            redirect_uri: redirect_uri.to_string(),
        };

        let mut new_oauth_state = OAuthState::Session(session);
        std::mem::swap(oauth_state, &mut new_oauth_state);
    } else {
        unreachable!()
    }

    Ok(())
}

/// This looks silly but [rmcp::transport::auth::OAuthTokenResponse] is private and there is no
/// other way to create this directly
fn get_stub_credentials() -> Result<OAuthTokenResponse, serde_json::Error> {
    const STUB_TOKEN: &str = r#"
            {
              "access_token": "stub",
              "token_type": "bearer",
              "expires_in": 3600,
              "refresh_token": "stub",
              "scope": "stub"
            }
        "#;

    serde_json::from_str::<OAuthTokenResponse>(STUB_TOKEN)
}

async fn make_svc(
    one_shot_sender: Sender<(String, String)>,
    socket_addr: SocketAddr,
    cancellation_token: CancellationToken,
) -> Result<(SocketAddr, LoopBackDropGuard), OauthUtilError> {
    type AuthCodeSender = Sender<(String, String)>;
    #[derive(Clone, Debug)]
    struct LoopBackForSendingAuthCode {
        one_shot_sender: Arc<std::sync::Mutex<Option<AuthCodeSender>>>,
    }

    #[derive(Debug, thiserror::Error)]
    enum LoopBackError {
        #[error("Poison error encountered: {0}")]
        Poison(String),
        #[error(transparent)]
        Http(#[from] http::Error),
        #[error("Failed to send auth code")]
        Send((String, String)),
    }

    fn mk_response(s: String) -> Result<Response<Full<Bytes>>, LoopBackError> {
        Ok(Response::builder().body(Full::new(Bytes::from(s)))?)
    }

    impl hyper::service::Service<hyper::Request<hyper::body::Incoming>> for LoopBackForSendingAuthCode {
        type Error = LoopBackError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
        type Response = Response<Full<Bytes>>;

        fn call(&self, req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
            let uri = req.uri();
            let query = uri.query().unwrap_or("");
            let params: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
            debug!("## mcp: uri: {}, query: {}, params: {:?}", uri, query, params);

            let self_clone = self.clone();
            Box::pin(async move {
                let error = params.get("error");
                let resp = if let Some(err) = error {
                    mk_response(format!(
                        "OAuth failed. Check URL for precise reasons. Possible reasons: {}.\n\
                         If this is scope related, you can try configuring the server scopes \n\
                         to be an empty array by adding \"oauthScopes\": [] to your server config.\n\
                         Example: {{\"type\": \"http\", \"uri\": \"https://example.com/mcp\", \"oauthScopes\": []}}\n",
                        err
                    ))
                } else {
                    mk_response("You can close this page now".to_string())
                };

                let code = params.get("code").cloned().unwrap_or_default();
                let state = params.get("state").cloned().unwrap_or_default();
                if let Some(sender) = self_clone
                    .one_shot_sender
                    .lock()
                    .map_err(|e| LoopBackError::Poison(e.to_string()))?
                    .take()
                {
                    sender.send((code, state)).map_err(LoopBackError::Send)?;
                }

                resp
            })
        }
    }

    let listener = tokio::net::TcpListener::bind(socket_addr).await?;
    let actual_addr = listener.local_addr()?;
    let cancellation_token_clone = cancellation_token.clone();
    let dg = LoopBackDropGuard {
        cancellation_token: cancellation_token_clone,
    };

    let loop_back = LoopBackForSendingAuthCode {
        one_shot_sender: Arc::new(std::sync::Mutex::new(Some(one_shot_sender))),
    };

    // This is one and done
    // This server only needs to last as long as it takes to send the auth code or to fail the auth
    // flow
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Oauth loopback server cancelled");
            },
            res = http1::Builder::new().serve_connection(io, loop_back) => {
                if let Err(err) = res {
                    error!("Auth code loop back has failed: {:?}", err);
                }
            }
        }

        Ok::<(), eyre::Report>(())
    });

    Ok((actual_addr, dg))
}
