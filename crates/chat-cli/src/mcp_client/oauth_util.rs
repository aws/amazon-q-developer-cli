use std::io::stderr;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crossterm::execute;
use crossterm::style::{
    Color,
    Print,
    ResetColor,
    SetForegroundColor,
};
use http_body_util::Full;
use hyper::Response;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use reqwest::Client;
use rmcp::serde_json;
use rmcp::transport::AuthorizationManager;
use rmcp::transport::auth::{
    AuthClient,
    OAuthState,
    OAuthTokenResponse,
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

pub struct AuthClientDropGuard {
    pub path: PathBuf,
    pub auth_client: AuthClient<Client>,
}

impl Drop for AuthClientDropGuard {
    fn drop(&mut self) {
        let auth_client_clone = self.auth_client.clone();
        let path = self.path.clone();

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

pub async fn get_auth_manager(
    url: Url,
    cred_full_path: PathBuf,
    messenger: &Box<dyn Messenger>,
) -> Result<AuthorizationManager, OauthUtilError> {
    let content_as_bytes = tokio::fs::read(cred_full_path).await;
    let mut oauth_state = OAuthState::new(url, None).await?;
    error!("## mcp: content as bytes: {:?}", content_as_bytes);

    match content_as_bytes {
        Ok(bytes) => {
            let token = serde_json::from_slice::<OAuthTokenResponse>(&bytes)?;

            oauth_state.set_credentials("id", token).await?;
            if let Err(e) = oauth_state.refresh_token().await {
                info!("Token refresh failed: {e}. Continuing with reauth.");
                return get_auth_manager_impl(oauth_state, messenger).await;
            }

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
    messenger: &Box<dyn Messenger>,
) -> Result<AuthorizationManager, OauthUtilError> {
    let socket_addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let cancellation_token = tokio_util::sync::CancellationToken::new();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();

    info!("Listening on local host port {:?} for oauth", socket_addr);
    let (actual_addr, _dg) = make_svc(tx, socket_addr, cancellation_token).await?;

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
