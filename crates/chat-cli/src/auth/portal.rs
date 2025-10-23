//! Unified auth portal integration for streamlined authentication
//! Handles callbacks from https://app.kiro.aws.dev/signin

use std::collections::HashMap;
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
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{
    debug,
    error,
    info,
};

use crate::auth::AuthError;
use crate::auth::pkce::{
    generate_code_challenge,
    generate_code_verifier,
};
use crate::auth::social::{
    CALLBACK_PORTS,
    SocialProvider,
};
use crate::database::{
    Database,
    Secret,
};
use crate::util::system_info::is_mwinit_available;

const AUTH_PORTAL_URL: &str = "https://app.kiro.aws.dev/signin";
const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(600);
const USER_AGENT: &str = "Kiro-CLI";

#[derive(Debug, Clone)]
struct AuthPortalCallback {
    login_option: String,
    code: Option<String>,
    issuer_url: Option<String>,
    sso_region: Option<String>,
    state: String,
    path: String,
}

pub enum PortalResult {
    /// User authenticated with social provider (Google/GitHub)
    Social(SocialProvider),
    /// User selected BuilderID authentication
    BuilderId { issuer_url: String, idc_region: String },
    /// User selected AWS Identity Center authentication
    AwsIdc { issuer_url: String, idc_region: String },
    /// User selected internal authentication (Amazon-only)
    Internal { issuer_url: String, idc_region: String },
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

    let listener = bind_allowed_port(CALLBACK_PORTS)
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to bind listener: {}", e)))?;
    let port = listener
        .local_addr()
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to get local addr: {}", e)))?
        .port();

    // We pass the base (no path). Portal will redirect to /oauth/callback or /signin/callback under
    // this base.
    let redirect_base = format!("http://localhost:{}", port);
    info!(%port, %redirect_base, "Unified auth portal listening (base) for callback");
    let is_internal = is_mwinit_available();

    let auth_url = format!(
        "{}?state={}&code_challenge={}&code_challenge_method=S256&redirect_uri={}{internal}&redirect_from=kirocli",
        AUTH_PORTAL_URL,
        state,
        challenge,
        urlencoding::encode(&redirect_base),
        internal = if is_internal { "&from_amazon_internal=true" } else { "" },
    );

    crate::util::open::open_url_async(&auth_url)
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to open browser: {}", e)))?;

    let callback = wait_for_auth_callback(listener, state.clone()).await?;

    match callback.login_option.as_str() {
        "google" | "github" => {
            let provider = if callback.login_option == "google" {
                SocialProvider::Google
            } else {
                SocialProvider::Github
            };

            let code = callback.code.ok_or(AuthError::OAuthMissingCode)?;
            let redirect_uri = format!(
                "http://localhost:{}{}?login_option={}",
                port,
                callback.path,
                urlencoding::encode(&callback.login_option)
            );

            exchange_social_token(db, provider, &verifier, &code, &redirect_uri).await?;
            Ok(PortalResult::Social(provider))
        },
        "internal" => {
            let issuer_url = callback
                .issuer_url
                .ok_or_else(|| AuthError::OAuthCustomError("Missing issuer_url for internal auth".into()))?;
            let sso_region = callback
                .sso_region
                .ok_or_else(|| AuthError::OAuthCustomError("Missing sso_region for internal auth".into()))?;
            // DO NOT register here. Let caller run start_pkce_authorization(issuer_url, sso_region).
            Ok(PortalResult::Internal {
                issuer_url,
                idc_region: sso_region,
            })
        },
        "awsidc" => {
            let issuer_url: String = callback
                .issuer_url
                .ok_or_else(|| AuthError::OAuthCustomError("Missing issuer_url for awsIdc auth".into()))?;
            let sso_region = callback
                .sso_region
                .ok_or_else(|| AuthError::OAuthCustomError("Missing sso_region for awsIdc auth".into()))?;
            // DO NOT register here. Let caller run start_pkce_authorization(issuer_url, sso_region).
            Ok(PortalResult::AwsIdc {
                issuer_url,
                idc_region: sso_region,
            })
        },
        "builderid" => {
            let issuer_url = callback
                .issuer_url
                .ok_or_else(|| AuthError::OAuthCustomError("Missing issuer_url for builderId auth".into()))?;
            let sso_region = callback
                .sso_region
                .ok_or_else(|| AuthError::OAuthCustomError("Missing sso_region for builderId auth".into()))?;
            // DO NOT register here. Let caller run start_pkce_authorization(issuer_url, sso_region).
            Ok(PortalResult::BuilderId {
                issuer_url,
                idc_region: sso_region,
            })
        },
        other => Err(AuthError::OAuthCustomError(format!("Unknown login_option: {}", other))),
    }
}

async fn wait_for_auth_callback(
    listener: TcpListener,
    expected_state: String,
) -> Result<AuthPortalCallback, AuthError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AuthPortalCallback>(1);
    // Accept a single connection
    let server_handle = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            let service = AuthCallbackService { tx: tx.clone() };
            let _ = http1::Builder::new().serve_connection(io, service).await;
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
                let query_params = uri
                    .query()
                    .map(|q| {
                        q.split('&')
                            .filter_map(|kv| kv.split_once('='))
                            .map(|(k, v)| (k.to_string(), urlencoding::decode(v).unwrap_or_default().to_string()))
                            .collect::<HashMap<_, _>>()
                    })
                    .unwrap_or_default();

                let callback = AuthPortalCallback {
                    login_option: query_params.get("login_option").cloned().unwrap_or_default(),
                    code: query_params.get("code").cloned(),
                    issuer_url: query_params.get("issuer_url").cloned(),
                    sso_region: query_params.get("idc_region").cloned(),
                    state: query_params.get("state").cloned().unwrap_or_default(),
                    path: path.to_string(),
                };

                debug!(
                    login_option=%callback.login_option,
                    code_present=%callback.code.is_some(),
                    issuer_url=?callback.issuer_url,
                    state=%callback.state,
                    "Parsed portal callback query"
                );

                let _ = tx.send(callback).await;

                let redirect_url = format!("{}?auth_status=success&redirect_from=kirocli", AUTH_PORTAL_URL);

                Ok(Response::builder()
                    .status(302)
                    .header("Location", redirect_url)
                    .header("Cache-Control", "no-store")
                    .body(Full::new(Bytes::from("")))
                    .expect("valid response"))
            } else {
                // Invalid path - redirect back to portal with error
                info!(%path, "Invalid callback path, redirecting to portal");

                let redirect_url = format!(
                    "{}?auth_status=error&redirect_from=kirocli&error_message={}",
                    AUTH_PORTAL_URL,
                    urlencoding::encode("Invalid callback path")
                );

                Ok(Response::builder()
                    .status(302)
                    .header("Location", redirect_url)
                    .header("Cache-Control", "no-store")
                    .body(Full::new(Bytes::from("")))
                    .expect("valid response"))
            }
        })
    }
}

async fn exchange_social_token(
    database: &mut Database,
    provider: SocialProvider,
    code_verifier: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<(), AuthError> {
    use reqwest::Client;
    use time::OffsetDateTime;

    use crate::auth::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
    use crate::auth::social::SocialToken;

    #[derive(Deserialize)]
    struct TokenResp {
        #[serde(rename = "accessToken")]
        access_token: String,
        #[serde(rename = "refreshToken")]
        refresh_token: String,
        #[serde(rename = "expiresIn")]
        expires_in: u64,
        #[serde(rename = "profileArn")]
        profile_arn: Option<String>,
    }

    let client = Client::new();
    let token_request = serde_json::json!({
        "code": code,
        "code_verifier": code_verifier,
        "redirect_uri": redirect_uri,
    });

    let response = client
        .post(format!("{}/oauth/token", SOCIAL_AUTH_SERVICE_ENDPOINT))
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&token_request)
        .send()
        .await?;

    if response.status().is_success() {
        let tr: TokenResp = response.json().await?;
        let token = SocialToken {
            access_token: Secret(tr.access_token),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(tr.expires_in as i64),
            refresh_token: Some(Secret(tr.refresh_token)),
            provider,
            profile_arn: tr.profile_arn,
        };
        token.save(database).await?;
        token.save_profile_if_any(database).await?;
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        error!("Token exchange failed: {} - {}", status, body);
        Err(AuthError::SocialAuthProviderFailure(format!(
            "Token exchange failed: {}",
            body
        )))
    }
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
    Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "Failed to bind to any port").into())
}
