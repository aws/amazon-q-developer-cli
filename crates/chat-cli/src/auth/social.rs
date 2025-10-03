use std::fmt;
use std::time::Duration;

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
use eyre::Result;
use rand::Rng;
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tracing::{
    debug,
    error,
    info,
    trace,
};

use crate::auth::AuthError;
use crate::auth::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
use crate::auth::pkce::{
    PkceRegistration,
    generate_code_challenge,
    generate_code_verifier,
};
use crate::database::{
    Database,
    Secret,
};
use crate::os::Os;
use crate::util::open::open_url_async;

const CALLBACK_PORTS: &[u16] = &[49153, 50153, 51153, 52153, 53153, 4649, 6588, 9091, 8008, 3128];
const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(300);
const SIGN_UP_PAUSED_MESSAGE: &str = "New signups are temporarily paused.";
const USER_AGENT: &str = "Kiro-CLI";

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

    pub async fn save(&self, database: &Database) -> Result<(), AuthError> {
        database
            .set_secret(Self::SECRET_KEY, &serde_json::to_string(self)?)
            .await?;
        Ok(())
    }

    pub async fn save_profile_if_any(&self, database: &mut Database) -> Result<(), AuthError> {
        if let Some(profile_arn) = &self.profile_arn {
            database.set_auth_profile(&crate::database::AuthProfile {
                arn: profile_arn.clone(),
                profile_name: "Social_default_Profile".to_string(),
            })?;
        }
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

#[derive(serde::Deserialize)]
struct ErrBody {
    message: Option<String>,
}

/// Start social login flow with optional invitation code
pub async fn start_social_login(os: &mut Os, provider: SocialProvider, invitation_code: Option<String>) -> Result<()> {
    info!("Starting social login with {}", provider);

    // PKCE
    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);
    let state = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(10)
        .collect::<Vec<_>>();
    let state = String::from_utf8(state).unwrap_or("state".to_string());
    // Bind to allowed port on 127.0.0.1 but always use localhost in redirect_uri
    let listener = bind_allowed_port(CALLBACK_PORTS).await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{}/oauth/callback", port);
    info!("OAuth callback server listening on {}", redirect_uri);

    // Provider login URL
    let idp = match provider {
        SocialProvider::Google => "Google",
        SocialProvider::Github => "Github",
    };
    let login_url = format!(
        "{}/login?idp={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}",
        SOCIAL_AUTH_SERVICE_ENDPOINT,
        idp,
        urlencoding::encode(&redirect_uri),
        challenge,
        state
    );

    // Open browser
    open_url_async(&login_url).await?;

    // Wait for callback using PKCE's shared server (serves /oauth/callback and /index.html, validates
    // state)
    let code_fut = PkceRegistration::recv_code_with_extra_accepts(listener, state.clone(), 2);
    let code = tokio::time::timeout(DEFAULT_AUTHORIZATION_TIMEOUT, code_fut)
        .await
        .map_err(|_| AuthError::OAuthTimeout)??;

    debug!("Received authorization code");

    // Token exchange
    let client = Client::new();
    let mut token_request = serde_json::json!({
        "code": code,
        "code_verifier": verifier,
        "redirect_uri": redirect_uri,
    });

    // Align with other product: invitationCode (camelCase)
    let had_invitation_code = invitation_code.is_some();
    if let Some(inv) = invitation_code {
        token_request["invitationCode"] = serde_json::Value::String(inv);
        debug!("Including invitationCode in token exchange");
    }

    let response = client
        .post(format!("{}/oauth/token", SOCIAL_AUTH_SERVICE_ENDPOINT))
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
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
        token.save_profile_if_any(&mut os.database).await?;
        info!("Successfully logged in with {}", provider);

        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

        let error_text = serde_json::from_str::<ErrBody>(&body)
            .ok()
            .and_then(|e| e.message)
            .unwrap_or_else(|| body.clone());

        let signups_paused = error_text == SIGN_UP_PAUSED_MESSAGE;

        let auth_error = match status.as_u16() {
            401 if signups_paused && !had_invitation_code => AuthError::OAuthCustomError("SIGN_IN_BLOCKED".into()),
            401 if signups_paused && had_invitation_code => AuthError::SocialInvalidInvitationCode,
            401 | 403 => AuthError::SocialAuthProviderDeniedAccess,
            _ => {
                error!("Failed to exchange code for token: {} - {}", status, error_text);
                AuthError::SocialAuthProviderFailure(format!("Token exchange failed: {}", error_text))
            },
        };

        Err(auth_error.into())
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
