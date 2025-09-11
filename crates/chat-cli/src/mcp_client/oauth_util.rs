use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use http::StatusCode;
use http_body_util::Full;
use hyper::Response;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use reqwest::Client;
use rmcp::serde_json;
use rmcp::transport::auth::{
    AuthClient,
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
use sha2::{
    Digest,
    Sha256,
};
use tokio::sync::oneshot::Sender;
use tokio_util::sync::CancellationToken;
use tracing::{
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

#[derive(Clone, Debug)]
pub struct AuthClientDropGuard {
    pub should_write: bool,
    pub cred_full_path: PathBuf,
    pub auth_client: AuthClient<Client>,
}

impl AuthClientDropGuard {
    pub fn new(cred_full_path: PathBuf, auth_client: AuthClient<Client>) -> Self {
        Self {
            should_write: true,
            cred_full_path,
            auth_client,
        }
    }
}

impl Drop for AuthClientDropGuard {
    fn drop(&mut self) {
        if !self.should_write {
            return;
        }

        let auth_client_clone = self.auth_client.clone();
        let path = self.cred_full_path.clone();

        tokio::spawn(async move {
            let Ok((client_id, cred)) = auth_client_clone.auth_manager.lock().await.get_credentials().await else {
                error!("Failed to retrieve credentials in drop routine");
                return;
            };
            let Some(cred) = cred else {
                error!("Failed to retrieve credentials in drop routine from {client_id}");
                return;
            };
            let Some(parent_path) = path.parent() else {
                error!("Failed to retrieve parent path for token in drop routine for {client_id}");
                return;
            };
            if let Err(e) = tokio::fs::create_dir_all(parent_path).await {
                error!("Error making parent directory for token cache in drop routine for {client_id}: {e}");
                return;
            }

            let serialized_cred = match serde_json::to_string_pretty(&cred) {
                Ok(cred) => cred,
                Err(e) => {
                    error!("Failed to serialize credentials for {client_id}: {e}");
                    return;
                },
            };
            if let Err(e) = tokio::fs::write(path, &serialized_cred).await {
                error!("Error making writing token cache in drop routine: {e}");
            }
        });
    }
}

pub enum HttpTransport {
    WithAuth(
        (
            WorkerTransport<StreamableHttpClientWorker<AuthClient<Client>>>,
            AuthClientDropGuard,
        ),
    ),
    WithoutAuth(WorkerTransport<StreamableHttpClientWorker<Client>>),
}

pub async fn get_http_transport(
    os: &Os,
    delete_cache: bool,
    url: &str,
    messenger: &dyn Messenger,
) -> Result<HttpTransport, OauthUtilError> {
    let cred_dir = get_mcp_auth_dir(os)?;
    let url = Url::from_str(url)?;
    let key = compute_key(&url);
    let cred_full_path = cred_dir.join(format!("{key}.token.json"));

    if delete_cache && cred_full_path.is_file() {
        tokio::fs::remove_file(&cred_full_path).await?;
    }

    let reqwest_client = reqwest::Client::default();
    let probe_resp = reqwest_client.get(url.clone()).send().await?;
    match probe_resp.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            let am = get_auth_manager(url.clone(), cred_full_path.clone(), messenger).await?;
            let auth_client = AuthClient::new(reqwest_client, am);
            let transport =
                StreamableHttpClientTransport::with_client(auth_client.clone(), StreamableHttpClientTransportConfig {
                    uri: url.as_str().into(),
                    allow_stateless: false,
                    ..Default::default()
                });

            let auth_dg = AuthClientDropGuard::new(cred_full_path, auth_client);

            Ok(HttpTransport::WithAuth((transport, auth_dg)))
        },
        _ => {
            let transport = StreamableHttpClientTransport::from_uri(url.as_str());

            Ok(HttpTransport::WithoutAuth(transport))
        },
    }
}

async fn get_auth_manager(
    url: Url,
    cred_full_path: PathBuf,
    messenger: &dyn Messenger,
) -> Result<AuthorizationManager, OauthUtilError> {
    let content_as_bytes = tokio::fs::read(cred_full_path).await;
    let mut oauth_state = OAuthState::new(url, None).await?;

    match content_as_bytes {
        Ok(bytes) => {
            let token = serde_json::from_slice::<OAuthTokenResponse>(&bytes)?;

            oauth_state.set_credentials("id", token).await?;

            Ok(oauth_state
                .into_authorization_manager()
                .ok_or(OauthUtilError::MissingAuthorizationManager)?)
        },
        Err(e) => {
            info!("Error reading cached credentials: {e}");
            get_auth_manager_impl(oauth_state, messenger).await
        },
    }
}

async fn get_auth_manager_impl(
    mut oauth_state: OAuthState,
    messenger: &dyn Messenger,
) -> Result<AuthorizationManager, OauthUtilError> {
    let socket_addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let cancellation_token = tokio_util::sync::CancellationToken::new();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    let (actual_addr, _dg) = make_svc(tx, socket_addr, cancellation_token).await?;
    info!("Listening on local host port {:?} for oauth", actual_addr);

    oauth_state
        .start_authorization(&["mcp", "profile", "email"], &format!("http://{}", actual_addr))
        .await?;

    let auth_url = oauth_state.get_authorization_url().await?;
    _ = messenger.send_oauth_link(auth_url).await;

    let auth_code = rx.await?;
    oauth_state.handle_callback(&auth_code).await?;
    let am = oauth_state
        .into_authorization_manager()
        .ok_or(OauthUtilError::MissingAuthorizationManager)?;

    Ok(am)
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

            let self_clone = self.clone();
            Box::pin(async move {
                let code = params.get("code").cloned().unwrap_or_default();
                if let Some(sender) = self_clone
                    .one_shot_sender
                    .lock()
                    .map_err(|e| LoopBackError::Poison(e.to_string()))?
                    .take()
                {
                    sender.send(code).map_err(LoopBackError::Send)?;
                }
                mk_response("Auth code sent".to_string())
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
