//! Common OAuth callback server utilities shared by PKCE and External IdP flows.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{
    debug,
    error,
};

use crate::auth::AuthError;

// NOTE: We use a fixed set of callback ports (not random) because:
// - IdP/Cognito only accepts pre-registered redirect URIs.
// - This list must match the Cognito allowlist
// - Bind only on loopback (127.0.0.1); never expose externally.
// - If all ports are in use, show a clear error.
// IMPORTANT: Do not change without auth service coordination.
pub const CALLBACK_PORTS: &[u16] = &[3128, 4649, 6588, 8008, 9091, 49153, 50153, 51153, 52153, 53153];

/// Bind to the first available port from the list, optionally excluding one.
pub async fn bind_callback_port(ports: &[u16], exclude: Option<u16>) -> Result<TcpListener, AuthError> {
    for port in ports {
        if Some(*port) == exclude {
            continue;
        }
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", *port)).await {
            return Ok(listener);
        }
    }
    Err(AuthError::OAuthCustomError("All callback ports are in use".into()))
}

/// Result from OAuth callback: (code, state)
pub type CallbackResult = Result<(String, String), AuthError>;
pub type CallbackSender = tokio::sync::mpsc::Sender<CallbackResult>;

/// Wait for OAuth callback with timeout, validating state.
pub async fn wait_for_callback(
    listener: TcpListener,
    expected_state: String,
    timeout: Duration,
    max_connections: usize,
) -> Result<String, AuthError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<CallbackResult>(1);
    let host = listener.local_addr()?.to_string();

    let server_handle = tokio::spawn(async move {
        let mut count = 0;
        loop {
            if count >= max_connections {
                break;
            }
            match listener.accept().await {
                Ok((stream, _)) => {
                    count += 1;
                    debug!("OAuth callback connection {}/{}", count, max_connections);
                    let io = TokioIo::new(stream);
                    let service = OAuthCallbackService {
                        tx: Arc::new(tx.clone()),
                        host: host.clone(),
                    };
                    tokio::spawn(async move {
                        let _ = http1::Builder::new().serve_connection(io, service).await;
                    });
                },
                Err(e) => {
                    error!("Accept failed: {}", e);
                    break;
                },
            }
        }
    });

    let result = tokio::select! {
        result = rx.recv() => result.ok_or(AuthError::OAuthCustomError("Callback channel closed".into()))?,
        _ = tokio::time::sleep(timeout) => {
            server_handle.abort();
            return Err(AuthError::OAuthTimeout);
        }
    };

    let (code, state) = result?;

    if state != expected_state {
        server_handle.abort();
        return Err(AuthError::OAuthStateMismatch {
            actual: state,
            expected: expected_state,
        });
    }

    // Wait for redirect to /index.html to complete before shutting down server
    tokio::time::sleep(Duration::from_millis(500)).await;
    server_handle.abort();

    Ok(code)
}

/// Generic OAuth callback HTTP service.
#[derive(Clone)]
pub struct OAuthCallbackService {
    pub tx: Arc<CallbackSender>,
    pub host: String,
}

type ServiceResponse = Response<Full<Bytes>>;
type ServiceFuture = Pin<Box<dyn Future<Output = Result<ServiceResponse, AuthError>> + Send>>;

impl Service<Request<Incoming>> for OAuthCallbackService {
    type Error = AuthError;
    type Future = ServiceFuture;
    type Response = ServiceResponse;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let tx = Arc::clone(&self.tx);
        let host = self.host.clone();
        Box::pin(async move {
            debug!(?req, "Handling OAuth callback");
            match req.uri().path() {
                "/oauth/callback" | "/oauth/callback/" => handle_callback(req.uri(), &tx, &host).await,
                "/index.html" => Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html")
                    .header("Connection", "close")
                    .body(include_str!("./index.html").into())
                    .expect("valid")),
                _ => Ok(Response::builder()
                    .status(404)
                    .body(Full::new(Bytes::from("")))
                    .expect("valid")),
            }
        })
    }
}

async fn handle_callback(uri: &hyper::Uri, tx: &CallbackSender, host: &str) -> Result<ServiceResponse, AuthError> {
    let query_params: HashMap<String, String> = uri
        .query()
        .map(|q| {
            q.split('&')
                .filter_map(|kv| {
                    kv.split_once('=')
                        .map(|(k, v)| (k.to_string(), urlencoding::decode(v).unwrap_or_default().to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(error) = query_params.get("error") {
        let desc = query_params.get("error_description").cloned().unwrap_or_default();
        let _ = tx
            .send(Err(AuthError::OAuthCustomError(format!("{error}: {desc}"))))
            .await;
        return redirect_to_index(host, Some(&format!("?error={}", urlencoding::encode(error))));
    }

    match (query_params.get("code"), query_params.get("state")) {
        (Some(code), Some(state)) => {
            let _ = tx.send(Ok((code.clone(), state.clone()))).await;
            redirect_to_index(host, None)
        },
        _ => {
            let _ = tx
                .send(Err(AuthError::OAuthCustomError("Missing code or state".into())))
                .await;
            redirect_to_index(host, Some("?error=missing_parameters"))
        },
    }
}

pub fn redirect_to_index(host: &str, query: Option<&str>) -> Result<ServiceResponse, AuthError> {
    Ok(Response::builder()
        .status(302)
        .header("Location", format!("http://{host}/index.html{}", query.unwrap_or("")))
        .body(Full::new(Bytes::from("")))
        .expect("valid"))
}
