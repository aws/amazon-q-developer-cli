//! Unified auth portal integration for streamlined authentication
//! Handles callbacks from https://app.kiro.dev/signin

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
use rand::Rng;
use tokio::net::TcpListener;
use tracing::{
    debug,
    error,
    info,
    warn,
};

use crate::auth::AuthError;
use crate::auth::pkce::{
    generate_code_challenge,
    generate_code_verifier,
};
use crate::auth::social::{
    CALLBACK_PORTS,
    SocialProvider,
    SocialToken,
};
use crate::database::Database;
use crate::util::system_info::is_mwinit_available;

const AUTH_PORTAL_URL: &str = "https://app.kiro.dev/signin";
const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
struct AuthPortalCallback {
    login_option: String,
    code: Option<String>,
    issuer_url: Option<String>,
    sso_region: Option<String>,
    state: String,
    path: String,
    error: Option<String>,
    error_description: Option<String>,
}

pub enum PortalResult {
    Social(SocialProvider),
    BuilderId {
        issuer_url: String,
        idc_region: String,
    },
    AwsIdc {
        issuer_url: String,
        idc_region: String,
    },
    /// Internal amazon user
    Internal {
        issuer_url: String,
        idc_region: String,
    },
}

/// Local-only: open unified portal and handle single callback
pub async fn start_unified_auth(db: &mut Database) -> Result<PortalResult, AuthError> {
    info!("Starting unified auth portal flow");

    // PKCE params for portal + social token exchange
    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);
    let state = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(10)
        .collect::<Vec<_>>();
    let state = String::from_utf8(state).unwrap_or("state".to_string());

    let listener = bind_allowed_port(CALLBACK_PORTS).await?;
    let port = listener.local_addr()?.port();

    let redirect_base = format!("http://localhost:{}", port);
    info!(%port, %redirect_base, "Unified auth portal listening for callback");

    let auth_url = build_auth_url(&redirect_base, &state, &challenge);

    crate::util::open::open_url_async(&auth_url)
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to open browser: {}", e)))?;

    let callback = wait_for_auth_callback(listener, state.clone()).await?;

    if let Some(error) = &callback.error {
        let friendly_msg =
            format_user_friendly_error(error, callback.error_description.as_deref(), &callback.login_option);

        warn!(
            "OAuth error for {}: {} - {}",
            callback.login_option, error, friendly_msg
        );

        return Err(match callback.login_option.as_str() {
            "google" | "github" => AuthError::SocialAuthProviderFailure(friendly_msg),
            _ => AuthError::OAuthCustomError(friendly_msg),
        });
    }

    process_portal_callback(db, callback, port, &verifier).await
}

fn format_user_friendly_error(error_code: &str, description: Option<&str>, provider: &str) -> String {
    let cleaned_description = description.map(|d| {
        let first_part = d.split(';').next().unwrap_or(d);
        // Replace + with spaces (URL encoding)
        first_part.replace('+', " ").trim().to_string()
    });

    match error_code {
        "access_denied" => {
            format!(
                "{} denied access to Kiro. Please ensure you grant all required permissions.",
                provider
            )
        },
        "invalid_request" => "Authentication failed due to an invalid request. Please try again.".to_string(),
        "unauthorized_client" => "The application is not authorized. Please contact support.".to_string(),
        "server_error" => {
            format!("{} login is temporarily unavailable. Please try again later.", provider)
        },
        "invalid_scope" => "The requested permissions are invalid. Please contact support.".to_string(),
        _ => {
            // For unknown errors, use cleaned description or a generic message
            cleaned_description.unwrap_or_else(|| format!("Authentication failed: {}. Please try again.", error_code))
        },
    }
}

/// Build the authorization URL with all required parameters
fn build_auth_url(redirect_base: &str, state: &str, challenge: &str) -> String {
    let is_internal = is_mwinit_available();
    let internal_param = if is_internal { "&from_amazon_internal=true" } else { "" };

    format!(
        "{}?state={}&code_challenge={}&code_challenge_method=S256&redirect_uri={}{}&redirect_from=kirocli",
        AUTH_PORTAL_URL,
        state,
        challenge,
        urlencoding::encode(redirect_base),
        internal_param
    )
}

async fn process_portal_callback(
    db: &mut Database,
    callback: AuthPortalCallback,
    port: u16,
    verifier: &str,
) -> Result<PortalResult, AuthError> {
    match callback.login_option.as_str() {
        "google" | "github" => handle_social_callback(db, callback, port, verifier).await,
        "internal" => {
            let (issuer_url, sso_region) = extract_sso_params(&callback, "internal")?;
            Ok(PortalResult::Internal {
                issuer_url,
                idc_region: sso_region,
            })
        },
        "awsidc" => {
            let (issuer_url, sso_region) = extract_sso_params(&callback, "awsIdc")?;
            Ok(PortalResult::AwsIdc {
                issuer_url,
                idc_region: sso_region,
            })
        },
        "builderid" => {
            let (issuer_url, sso_region) = extract_sso_params(&callback, "builderId")?;
            Ok(PortalResult::BuilderId {
                issuer_url,
                idc_region: sso_region,
            })
        },
        other => Err(AuthError::OAuthCustomError(format!("Unknown login_option: {}", other))),
    }
}

/// Handle social provider callback (Google/GitHub)
async fn handle_social_callback(
    db: &mut Database,
    callback: AuthPortalCallback,
    port: u16,
    verifier: &str,
) -> Result<PortalResult, AuthError> {
    let provider = match callback.login_option.as_str() {
        "google" => SocialProvider::Google,
        "github" => SocialProvider::Github,
        _ => unreachable!(),
    };

    let code = callback.code.ok_or(AuthError::OAuthMissingCode)?;
    let redirect_uri = format!(
        "http://localhost:{}{}?login_option={}",
        port,
        callback.path,
        urlencoding::encode(&callback.login_option)
    );

    SocialToken::exchange_social_token(db, provider, verifier, &code, &redirect_uri).await?;
    Ok(PortalResult::Social(provider))
}

/// Extract issuer_url and sso_region from callback, returning descriptive error if missing
fn extract_sso_params(callback: &AuthPortalCallback, auth_type: &str) -> Result<(String, String), AuthError> {
    let issuer_url = callback
        .issuer_url
        .clone()
        .ok_or_else(|| AuthError::OAuthCustomError(format!("Missing issuer_url for {} auth", auth_type)))?;

    let sso_region = callback
        .sso_region
        .clone()
        .ok_or_else(|| AuthError::OAuthCustomError(format!("Missing sso_region for {} auth", auth_type)))?;

    Ok((issuer_url, sso_region))
}

async fn wait_for_auth_callback(
    listener: TcpListener,
    expected_state: String,
) -> Result<AuthPortalCallback, AuthError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AuthPortalCallback>(1);

    let server_handle = tokio::spawn(async move {
        const MAX_CONNECTIONS: usize = 3;
        let mut count = 0;

        loop {
            if count >= MAX_CONNECTIONS {
                warn!("Reached max connections ({})", MAX_CONNECTIONS);
                break;
            }

            match listener.accept().await {
                Ok((stream, _)) => {
                    count += 1;
                    debug!("Connection {}/{}", count, MAX_CONNECTIONS);

                    let io = TokioIo::new(stream);
                    let service = AuthCallbackService { tx: tx.clone() };

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

    let callback = tokio::select! {
        result = rx.recv() => {
            result.ok_or(AuthError::OAuthCustomError("Failed to receive callback".into()))?
        },
        _ = tokio::time::sleep(DEFAULT_AUTHORIZATION_TIMEOUT) => {
            return Err(AuthError::OAuthTimeout);
        }
    };

    server_handle.abort();

    if callback.state != expected_state {
        return Err(AuthError::OAuthStateMismatch {
            actual: callback.state,
            expected: expected_state,
        });
    }

    Ok(callback)
}

#[derive(Clone)]
struct AuthCallbackService {
    tx: tokio::sync::mpsc::Sender<AuthPortalCallback>,
}

impl Service<Request<Incoming>> for AuthCallbackService {
    type Error = AuthError;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Response<Full<Bytes>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let tx = self.tx.clone();

        Box::pin(async move {
            let uri = req.uri();
            let path = uri.path();

            if path == "/oauth/callback" || path == "/signin/callback" {
                handle_valid_callback(uri, path, tx).await
            } else {
                handle_invalid_callback(path).await
            }
        })
    }
}

/// Handle valid callback paths
async fn handle_valid_callback(
    uri: &hyper::Uri,
    path: &str,
    tx: tokio::sync::mpsc::Sender<AuthPortalCallback>,
) -> Result<Response<Full<Bytes>>, AuthError> {
    let query_params = uri
        .query()
        .map(|query| {
            query
                .split('&')
                .filter_map(|kv| {
                    kv.split_once('=')
                        .map(|(k, v)| (k.to_string(), urlencoding::decode(v).unwrap_or_default().to_string()))
                })
                .collect::<std::collections::HashMap<String, String>>() // 
        })
        .ok_or(AuthError::OAuthCustomError("query parameters are missing".into()))?;

    let callback = AuthPortalCallback {
        login_option: query_params.get("login_option").cloned().unwrap_or_default(),
        code: query_params.get("code").cloned(),
        issuer_url: query_params.get("issuer_url").cloned(),
        sso_region: query_params.get("idc_region").cloned(),
        state: query_params.get("state").cloned().unwrap_or_default(),
        path: path.to_string(),
        error: query_params.get("error").cloned(),
        error_description: query_params.get("error_description").cloned(),
    };

    let _ = tx.send(callback.clone()).await;

    if let Some(error) = &callback.error {
        let error_msg = callback.error_description.as_deref().unwrap_or(error.as_str());
        build_redirect_response("error", Some(error_msg))
    } else {
        build_redirect_response("success", None)
    }
}

async fn handle_invalid_callback(path: &str) -> Result<Response<Full<Bytes>>, AuthError> {
    info!(%path, "Invalid callback path: {}, redirecting to portal", path);
    build_redirect_response("error", Some("Invalid callback path"))
}

/// Build a redirect response to the auth portal
fn build_redirect_response(status: &str, error_message: Option<&str>) -> Result<Response<Full<Bytes>>, AuthError> {
    let mut redirect_url = format!("{}?auth_status={}&redirect_from=kirocli", AUTH_PORTAL_URL, status);

    if let Some(msg) = error_message {
        redirect_url.push_str(&format!("&error_message={}", urlencoding::encode(msg)));
    }

    Ok(Response::builder()
        .status(302)
        .header("Location", redirect_url)
        .header("Cache-Control", "no-store")
        .body(Full::new(Bytes::from("")))
        .expect("valid response"))
}

async fn bind_allowed_port(ports: &[u16]) -> Result<TcpListener, AuthError> {
    for port in ports {
        match TcpListener::bind(("127.0.0.1", *port)).await {
            Ok(listener) => return Ok(listener),
            Err(e) => {
                debug!("Failed to bind to port {}: {}", port, e);
            },
        }
    }

    Err(AuthError::OAuthCustomError(
        "All callback ports are in use. Please close some applications and try again.".into(),
    ))
}
