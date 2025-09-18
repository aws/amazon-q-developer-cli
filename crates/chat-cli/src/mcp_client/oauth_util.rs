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
use rmcp::serde_json;
use rmcp::transport::auth::{
    AuthClient,
    OAuthClientConfig,
    OAuthState,
    OAuthTokenResponse,
};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransportConfig,
    StreamableHttpClientWorker,
};
use rmcp::transport::{
    AuthorizationManager,
    StreamableHttpClientTransport,
    WorkerTransport,
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

/// HTTP transport wrapper that handles both authenticated and non-authenticated MCP connections.
///
/// This enum provides two variants for different authentication scenarios:
/// - `WithAuth`: Used when the MCP server requires OAuth authentication, containing both the
///   transport worker and an auth client guard that manages credential persistence
/// - `WithoutAuth`: Used for servers that don't require authentication, containing only the basic
///   transport worker
///
/// The appropriate variant is automatically selected based on the server's response to
/// an initial probe request during transport creation.
pub enum HttpTransport {
    WithAuth(
        (
            WorkerTransport<StreamableHttpClientWorker<AuthClient<Client>>>,
            AuthClientWrapper,
        ),
    ),
    WithoutAuth(WorkerTransport<StreamableHttpClientWorker<Client>>),
}

pub fn get_default_scopes() -> &'static [&'static str] {
    &["openid", "email", "profile", "offline_access"]
}

pub async fn get_http_transport(
    os: &Os,
    url: &str,
    timeout: u64,
    scopes: &[String],
    headers: &HashMap<String, String>,
    auth_client: Option<AuthClient<Client>>,
    messenger: &dyn Messenger,
) -> Result<HttpTransport, OauthUtilError> {
    let cred_dir = get_mcp_auth_dir(os)?;
    let url = Url::from_str(url)?;
    let key = compute_key(&url);
    let cred_full_path = cred_dir.join(format!("{key}.token.json"));
    let reg_full_path = cred_dir.join(format!("{key}.registration.json"));

    let mut client_builder = reqwest::ClientBuilder::new().timeout(std::time::Duration::from_millis(timeout));
    if !headers.is_empty() {
        let headers = HeaderMap::try_from(headers).map_err(|e| OauthUtilError::Http(e.to_string()))?;
        client_builder = client_builder.default_headers(headers);
    };
    let reqwest_client = client_builder.build()?;

    let probe_resp = reqwest_client.get(url.clone()).send().await?;
    match probe_resp.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            debug!("## mcp: requires auth, auth client passed in is {:?}", auth_client);
            let auth_client = match auth_client {
                Some(auth_client) => auth_client,
                None => {
                    let am = get_auth_manager(
                        url.clone(),
                        cred_full_path.clone(),
                        reg_full_path.clone(),
                        scopes,
                        messenger,
                    )
                    .await?;
                    AuthClient::new(reqwest_client, am)
                },
            };
            let transport =
                StreamableHttpClientTransport::with_client(auth_client.clone(), StreamableHttpClientTransportConfig {
                    uri: url.as_str().into(),
                    allow_stateless: false,
                    ..Default::default()
                });

            let auth_dg = AuthClientWrapper::new(cred_full_path, auth_client);
            debug!("## mcp: transport obtained");

            Ok(HttpTransport::WithAuth((transport, auth_dg)))
        },
        _ => {
            let transport =
                StreamableHttpClientTransport::with_client(reqwest_client, StreamableHttpClientTransportConfig {
                    uri: url.as_str().into(),
                    allow_stateless: false,
                    ..Default::default()
                });

            Ok(HttpTransport::WithoutAuth(transport))
        },
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
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    let (actual_addr, _dg) = make_svc(tx, socket_addr, cancellation_token).await?;
    info!("Listening on local host port {:?} for oauth", actual_addr);

    let redirect_uri = format!("http://{}", actual_addr);
    let scopes_as_str = scopes.iter().map(String::as_str).collect::<Vec<_>>();
    let scopes_as_slice = scopes_as_str.as_slice();
    oauth_state.start_authorization(scopes_as_slice, &redirect_uri).await?;

    let auth_url = oauth_state.get_authorization_url().await?;
    _ = messenger.send_oauth_link(auth_url).await;

    let auth_code = rx.await?;
    oauth_state.handle_callback(&auth_code).await?;
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

async fn make_svc(
    one_shot_sender: Sender<String>,
    socket_addr: SocketAddr,
    cancellation_token: CancellationToken,
) -> Result<(SocketAddr, LoopBackDropGuard), OauthUtilError> {
    #[derive(Clone, Debug)]
    struct LoopBackForSendingAuthCode {
        one_shot_sender: Arc<std::sync::Mutex<Option<Sender<String>>>>,
    }

    #[derive(Debug, thiserror::Error)]
    enum LoopBackError {
        #[error("Poison error encountered: {0}")]
        Poison(String),
        #[error(transparent)]
        Http(#[from] http::Error),
        #[error("Failed to send auth code: {0}")]
        Send(String),
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
                        "Oauth failed. Check url for precise reasons. Possible reasons: {err}.\nIf this is scope related. You can try configuring the server scopes to be an empty array via adding oauth_scopes: []"
                    ))
                } else {
                    mk_response("You can close this page now".to_string())
                };

                let code = params.get("code").cloned().unwrap_or_default();
                if let Some(sender) = self_clone
                    .one_shot_sender
                    .lock()
                    .map_err(|e| LoopBackError::Poison(e.to_string()))?
                    .take()
                {
                    sender.send(code).map_err(LoopBackError::Send)?;
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
