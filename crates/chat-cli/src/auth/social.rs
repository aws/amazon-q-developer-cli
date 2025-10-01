use std::fmt;

use aws_sdk_ssooidc::config::{
    ConfigBag,
    RuntimeComponents,
};
use aws_smithy_runtime_api::client::identity::http::Token;
use aws_smithy_runtime_api::client::identity::{
    Identity,
    IdentityFuture,
    ResolveIdentity,
};
use eyre::{
    Result,
    bail,
};
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{
    debug,
    error,
    info,
    trace,
};

use crate::auth::AuthError;
use crate::auth::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
use crate::database::{
    Database,
    Secret,
};
use crate::os::Os;
use crate::util::open::open_url_async;

const CALLBACK_PORTS: &[u16] = &[49153, 50153, 51153, 52153, 53153];

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

    /// Load the social token from the keychain
    pub async fn load(database: &Database) -> Result<Option<Self>, AuthError> {
        // For testing
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
                    },
                    None => {
                        debug!("social secret stored in the database was empty");
                        Ok(None)
                    },
                }
            },
            Ok(None) => {
                debug!("no social secret found in the database");
                Ok(None)
            },
            Err(err) => {
                error!(%err, "Error getting social token from keychain");
                Err(err)?
            },
        }
    }

    /// Save the token to the keychain
    pub async fn save(&self, database: &Database) -> Result<(), AuthError> {
        database
            .set_secret(Self::SECRET_KEY, &serde_json::to_string(self)?)
            .await?;
        Ok(())
    }

    /// Delete the token from the keychain
    pub async fn delete(&self, database: &Database) -> Result<(), AuthError> {
        database.delete_secret(Self::SECRET_KEY).await?;
        Ok(())
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        let now = OffsetDateTime::now_utc();
        (now + time::Duration::minutes(1)) > self.expires_at
    }

    /// Refresh the access token
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
                expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
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
            return Err(AuthError::HttpStatus(status));
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

/// OAuth callback server
struct CallbackServer {
    listener: TcpListener,
    tx: mpsc::Sender<String>,
}

impl CallbackServer {
    async fn new(ports: &[u16]) -> Result<(Self, mpsc::Receiver<String>, String)> {
        let (tx, rx) = mpsc::channel(1);

        for port in ports {
            match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
                Ok(listener) => {
                    let addr = listener.local_addr()?;
                    let redirect_uri = format!("http://localhost:{}/oauth/callback", addr.port());
                    info!("OAuth callback server listening on {}", redirect_uri);

                    return Ok((Self { listener, tx }, rx, redirect_uri));
                },
                Err(e) => {
                    debug!("Failed to bind to port {}: {}", port, e);
                    continue;
                },
            }
        }

        bail!("Failed to bind to any available port");
    }

    async fn handle_callback(self) {
        tokio::spawn(async move {
            if let Ok((stream, _)) = self.listener.accept().await {
                let mut buf = vec![0; 1024];

                use tokio::io::{
                    AsyncReadExt,
                    AsyncWriteExt,
                };
                let mut stream = tokio::net::TcpStream::from_std(stream.into_std().unwrap()).unwrap();

                if let Ok(n) = stream.read(&mut buf).await {
                    let request = String::from_utf8_lossy(&buf[..n]);

                    // Extract code from query params
                    if let Some(code) = extract_code_from_request(&request) {
                        let _ = self.tx.send(code).await;

                        // Send success response
                        let response = "HTTP/1.1 200 OK\r\n\
                            Content-Type: text/html\r\n\
                            \r\n\
                            <html><body>\
                            <h2>Login successful!</h2>\
                            <p>You can close this window and return to the terminal.</p>\
                            </body></html>";

                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                }
            }
        });
    }
}

fn extract_code_from_request(request: &str) -> Option<String> {
    // Parse GET request for code parameter
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let parts: Vec<&str> = lines[0].split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let path = parts[1];
    if !path.starts_with("/oauth/callback?") {
        return None;
    }

    let query = &path[16..]; // Skip "/oauth/callback?"
    for param in query.split('&') {
        let kv: Vec<&str> = param.split('=').collect();
        if kv.len() == 2 && kv[0] == "code" {
            return Some(kv[1].to_string());
        }
    }

    None
}

/// Start social login flow with optional invitation code
pub async fn start_social_login(os: &mut Os, provider: SocialProvider, invitation_code: Option<String>) -> Result<()> {
    info!("Starting social login with {}", provider);

    // Start callback server
    let (server, mut rx, redirect_uri) = CallbackServer::new(CALLBACK_PORTS).await?;

    // Generate PKCE challenge
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use sha2::{
        Digest,
        Sha256,
    };

    let verifier = generate_random_string(32);
    let mut hasher = Sha256::new();
    hasher.update(&verifier);
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    // Build login URL
    let state = generate_random_string(16);
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

    // Start handling callback
    server.handle_callback().await;

    // Wait for authorization code
    let code = tokio::time::timeout(
        std::time::Duration::from_secs(300), // 5 minute timeout
        rx.recv(),
    )
    .await
    .map_err(|_| eyre::eyre!("Login timeout"))?
    .ok_or_else(|| eyre::eyre!("No authorization code received"))?;

    debug!("Received authorization code");

    // Exchange code for token - include invitation_code if provided
    let client = Client::new();
    let mut token_request = serde_json::json!({
        "code": code,
        "code_verifier": verifier,
        "redirect_uri": redirect_uri,
    });

    // Add invitation_code if provided
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
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider,
            profile_arn: token_response.profile_arn,
        };

        token.save(&os.database).await?;
        info!("Successfully logged in with {}", provider);
        Ok(())
    } else {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        error!("Failed to exchange code for token: {} - {}", status, error_text);
        bail!("Failed to exchange code for token: {} - {}", status, error_text);
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

/// Check if user is logged in with social auth
pub async fn is_social_logged_in(database: &Database) -> bool {
    matches!(SocialToken::load(database).await, Ok(Some(_)))
}

/// Logout social auth
pub async fn logout_social(database: &Database) -> Result<(), AuthError> {
    database.delete_secret(SocialToken::SECRET_KEY).await?;
    Ok(())
}
