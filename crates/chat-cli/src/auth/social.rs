use std::fmt;
use std::time::Duration;

use aws_sdk_ssooidc::config::{ConfigBag, RuntimeComponents};
use aws_smithy_runtime_api::client::identity::http::Token;
use aws_smithy_runtime_api::client::identity::{Identity, IdentityFuture, ResolveIdentity};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use eyre::{Result, bail};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace};

use crate::auth::AuthError;
use crate::auth::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
use crate::database::{Database, Secret};
use crate::os::Os;
use crate::util::open::open_url_async;

const CALLBACK_PORTS: &[u16] = &[49153, 50153, 51153, 52153, 53153];
const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum SocialProvider {
    #[serde(rename = "google")]
    #[value(name = "google")]
    Google,
    #[serde(rename = "github")]
    #[value(name = "github")]
    Github,
}

impl fmt::Display for SocialProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocialProvider::Google => write!(f, "Google"),
            SocialProvider::Github => write!(f, "GitHub"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialToken {
    pub access_token: Secret,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub refresh_token: Option<Secret>,
    pub provider: SocialProvider,
    pub profile_arn: Option<String>,
}

impl SocialToken {
    const SECRET_KEY: &'static str = "codewhisperer:social:token";

    pub async fn load(database: &Database) -> Result<Option<Self>, AuthError> {
        if cfg!(test) {
            return Ok(Some(Self {
                access_token: Secret("test_access_token".to_string()),
                expires_at: OffsetDateTime::now_utc() + time::Duration::minutes(60),
                refresh_token: Some(Secret("test_refresh_token".to_string())),
                provider: SocialProvider::Google,
                profile_arn: None,
            }));
        }

        trace!("loading social token from the secret store");
        match database.get_secret(Self::SECRET_KEY).await {
            Ok(Some(secret)) => {
                let token: Option<Self> = serde_json::from_str(&secret.0)?;
                match token {
                    Some(mut token) => {
                        if token.is_expired() {
                            trace!("token is expired, refreshing");
                            token = token.refresh_token(database).await?;
                        }
                        trace!(?token, "found a valid social token");
                        Ok(Some(token))
                    }
                    None => {
                        debug!("social secret stored in the database was empty");
                        Ok(None)
                    }
                }
            }
            Ok(None) => {
                debug!("no social secret found in the database");
                Ok(None)
            }
            Err(err) => {
                error!(%err, "Error getting social token from keychain");
                Err(err)?
            }
        }
    }

    pub async fn save(&self, database: &Database) -> Result<(), AuthError> {
        database
            .set_secret(Self::SECRET_KEY, &serde_json::to_string(self)?)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, database: &Database) -> Result<(), AuthError> {
        database.delete_secret(Self::SECRET_KEY).await?;
        Ok(())
    }

    pub fn is_expired(&self) -> bool {
        let now = OffsetDateTime::now_utc();
        (now + time::Duration::minutes(1)) > self.expires_at
    }

    pub async fn refresh_token(&self, database: &Database) -> Result<Self, AuthError> {
        let Some(refresh_token) = &self.refresh_token else {
            error!("no refresh token was found for social login");
            self.delete(database).await?;
            return Err(AuthError::NoToken);
        };

        debug!("Refreshing social access token");

        let client = Client::new();
        let response = client
            .post(format!("{}/refreshToken", SOCIAL_AUTH_SERVICE_ENDPOINT))
            .json(&serde_json::json!({
                "refreshToken": refresh_token.0
            }))
            .send()
            .await?;

        if response.status().is_success() {
            let token_response: TokenResponse = response.json().await?;
            let new_token = Self {
                access_token: Secret(token_response.access_token),
                expires_at: OffsetDateTime::now_utc()
                    + time::Duration::seconds(token_response.expires_in as i64),
                refresh_token: Some(Secret(token_response.refresh_token)),
                provider: self.provider,
                profile_arn: token_response.profile_arn.or(self.profile_arn.clone()),
            };

            new_token.save(database).await?;
            Ok(new_token)
        } else {
            let status = response.status();
            error!("Failed to refresh social token: {}", response.status());
            self.delete(database).await?;
            Err(AuthError::HttpStatus(status))
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresIn")]
    expires_in: u64,
    #[serde(rename = "profileArn")]
    profile_arn: Option<String>,
}

type CodeSender = std::sync::Arc<mpsc::Sender<Result<String, AuthError>>>;
type ServiceError = AuthError;
type ServiceResponse = Response<Full<Bytes>>;
type ServiceFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ServiceResponse, ServiceError>> + Send>,
>;

/// OAuth callback server (reused pattern from pkce.rs)
#[derive(Debug, Clone)]
struct SocialCallbackService {
    code_tx: CodeSender,
    host: String,
}

impl SocialCallbackService {
    async fn handle_oauth_callback(
        code_tx: CodeSender,
        host: String,
        req: Request<Incoming>,
    ) -> Result<ServiceResponse, AuthError> {
        let query_params = req
            .uri()
            .query()
            .map(|query| {
                query
                    .split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .collect::<std::collections::HashMap<_, _>>()
            })
            .ok_or_else(|| {
                AuthError::SocialAuthProviderFailure(
                    "query parameters are missing".to_string(),
                )
            })?;

        // Handle error responses from identity provider
        if let Some(error) = query_params.get("error") {
            let error_description = query_params
                .get("error_description")
                .map(|s| urlencoding::decode(s).unwrap_or_default().to_string())
                .unwrap_or_default();

            let auth_error = match *error {
                "access_denied" => {
                    info!("User denied access through identity provider");
                    AuthError::SocialAuthProviderDeniedAccess
                }
                _ => {
                    error!("Identity provider error: {} - {}", error, error_description);
                    AuthError::SocialAuthProviderFailure(format!(
                        "{}: {}",
                        error, error_description
                    ))
                }
            };

            let _ = code_tx.send(Err(auth_error)).await;
            return Self::redirect_to_index(&host, &format!("?error={}", urlencoding::encode(error)));
        }

        // Extract authorization code
        let code = query_params.get("code").ok_or_else(|| {
            AuthError::SocialAuthProviderFailure("missing code in callback".to_string())
        })?;

        let _ = code_tx.send(Ok((*code).to_string())).await;
        Self::redirect_to_index(&host, "")
    }

    fn redirect_to_index(host: &str, query_params: &str) -> Result<ServiceResponse, AuthError> {
        Ok(Response::builder()
            .status(302)
            .header("Location", format!("http://{}/index.html{}", host, query_params))
            .body("".into())
            .expect("valid builder will not panic"))
    }
}

impl Service<Request<Incoming>> for SocialCallbackService {
    type Error = ServiceError;
    type Future = ServiceFuture;
    type Response = ServiceResponse;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let code_tx = std::sync::Arc::clone(&self.code_tx);
        let host = self.host.clone();
        Box::pin(async move {
            debug!(?req, "Handling OAuth callback");
            match req.uri().path() {
                "/oauth/callback" | "/oauth/callback/" => {
                    Self::handle_oauth_callback(code_tx, host, req).await
                }
                "/index.html" => Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html")
                    .header("Connection", "close")
                    .body(include_str!("./index.html").into())
                    .expect("valid builder will not panic")),
                _ => Ok(Response::builder()
                    .status(404)
                    .body("".into())
                    .expect("valid builder will not panic")),
            }
        })
    }
}

/// Start social login flow with optional invitation code
pub async fn start_social_login(
    os: &mut Os,
    provider: SocialProvider,
    invitation_code: Option<String>,
) -> Result<()> {
    info!("Starting social login with {}", provider);

    // Generate PKCE challenge
    let verifier = generate_random_string(32);
    let mut hasher = Sha256::new();
    hasher.update(&verifier);
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    // Build login URL
    let state = generate_random_string(16);
    
    // Start callback server first to get redirect_uri
    let (code_tx, mut code_rx) = mpsc::channel(1);
    let code_tx_arc = std::sync::Arc::new(code_tx);
    
    let mut listener = None;
    let mut redirect_uri = String::new();
    
    for port in CALLBACK_PORTS {
        match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
            Ok(l) => {
                let addr = l.local_addr()?;
                redirect_uri = format!("http://localhost:{}/oauth/callback", addr.port());
                listener = Some(l);
                break;
            }
            Err(e) => {
                debug!("Failed to bind to port {}: {}", port, e);
            }
        }
    }
    
    let listener = listener.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AddrInUse, "Failed to bind to any port")
    })?;
    
    let host = listener.local_addr()?.to_string();
    info!("OAuth callback server listening on {}", redirect_uri);

    let login_url = format!(
        "{}/login?idp={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}",
        SOCIAL_AUTH_SERVICE_ENDPOINT,
        match provider {
            SocialProvider::Google => "Google",
            SocialProvider::Github => "Github",
        },
        urlencoding::encode(&redirect_uri),
        challenge,
        state
    );

    // Open browser
    open_url_async(&login_url).await?;

    // Serve multiple connections to handle both /oauth/callback and /index.html
    let server_handle = tokio::spawn(async move {
        // Handle up to 2 connections (callback + index.html)
        for _ in 0..2 {
            if let Ok((stream, _)) = listener.accept().await {
                let stream = TokioIo::new(stream);
                let service = SocialCallbackService {
                    code_tx: code_tx_arc.clone(),
                    host: host.clone(),
                };
                
                tokio::spawn(async move {
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(stream, service)
                        .await
                    {
                        debug!(?err, "Error serving connection");
                    }
                });
            }
        }
    });

    // Wait for authorization code
    let code = tokio::select! {
        code = code_rx.recv() => {
            match code {
                Some(Ok(c)) => c,
                Some(Err(e)) => {
                    return Err(e.into());
                }
                None => {
                    return Err(AuthError::OAuthMissingCode.into());
                }
            }
        },
        _ = tokio::time::sleep(DEFAULT_AUTHORIZATION_TIMEOUT) => {
            return Err(AuthError::OAuthTimeout.into());
        }
    };

    debug!("Received authorization code");

    // Exchange code for token
    let client = Client::new();
    let mut token_request = serde_json::json!({
        "code": code,
        "code_verifier": verifier,
        "redirect_uri": redirect_uri,
    });

    if let Some(inv_code) = invitation_code {
        token_request["invitation_code"] = serde_json::Value::String(inv_code);
        debug!("Including invitation code in token exchange");
    }

    let response = client
        .post(&format!("{}/oauth/token", SOCIAL_AUTH_SERVICE_ENDPOINT))
        .header("Content-Type", "application/json")
        .header("User-Agent", "q-cli")
        .json(&token_request)
        .send()
        .await?;

    if response.status().is_success() {
        let token_response: TokenResponse = response.json().await?;

        let token = SocialToken {
            access_token: Secret(token_response.access_token),
            expires_at: OffsetDateTime::now_utc()
                + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider,
            profile_arn: token_response.profile_arn,
        };

        token.save(&os.database).await?;
        info!("Successfully logged in with {}", provider);
        
        // Wait for the browser to load index.html before exiting
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Allow server to finish serving index.html
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
        
        Ok(())
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        // Map specific HTTP errors to user-friendly messages
        let auth_error = match status.as_u16() {
            401 if error_text.contains("signups are temporarily paused") 
                || error_text.contains("invitation") => {
                AuthError::SocialInvalidInvitationCode
            }
            401 | 403 => AuthError::SocialAuthProviderDeniedAccess,
            _ => {
                error!("Failed to exchange code for token: {} - {}", status, error_text);
                AuthError::SocialAuthProviderFailure(format!(
                    "Token exchange failed: {}",
                    error_text
                ))
            }
        };

        Err(auth_error.into())
    }
}

fn generate_random_string(len: usize) -> String {
    use rand::Rng;
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();

    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..charset.len());
            charset[idx] as char
        })
        .collect()
}

/// Social bearer token resolver for AWS SDK
#[derive(Debug, Clone)]
pub struct SocialBearerResolver;

impl ResolveIdentity for SocialBearerResolver {
    fn resolve_identity<'a>(
        &'a self,
        _runtime_components: &'a RuntimeComponents,
        _config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a> {
        IdentityFuture::new_boxed(Box::pin(async {
            let database = Database::new().await?;
            match SocialToken::load(&database).await? {
                Some(token) => Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                )),
                None => Err(AuthError::NoToken.into()),
            }
        }))
    }
}

pub async fn is_social_logged_in(database: &Database) -> bool {
    matches!(SocialToken::load(database).await, Ok(Some(_)))
}

pub async fn logout_social(database: &Database) -> Result<(), AuthError> {
    database.delete_secret(SocialToken::SECRET_KEY).await?;
    Ok(())
}