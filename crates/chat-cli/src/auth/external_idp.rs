//! External Identity Provider (Enterprise SSO) authentication
//!
//! This module implements OAuth 2.0 Authorization Code + PKCE flow for External IdPs.
//! Unlike BuilderID/IdC where we use AWS OIDC, here we directly interact with
//! the customer's Identity Provider (e.g., Azure EntraID, Okta, Auth0).
//!
//! Flow:
//! 1. Portal returns IdP metadata (issuer_url, client_id, scopes, etc.)
//! 2. CLI fetches OIDC discovery document from {issuer_url}/.well-known/openid-configuration
//! 3. CLI generates PKCE params and opens browser to IdP authorization endpoint
//! 4. CLI receives auth code via local callback server
//! 5. CLI exchanges code for tokens at IdP token endpoint
//! 6. Tokens are stored with IdP metadata for future refresh

use std::time::Duration;

use rand::Rng;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use crate::auth::AuthError;
use crate::auth::oauth_callback::{
    CALLBACK_PORTS,
    bind_callback_port,
    wait_for_callback,
};
use crate::auth::pkce::{
    generate_code_challenge,
    generate_code_verifier,
};
use crate::database::{
    Database,
    Secret,
};

const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(600);

/// Metadata returned by the portal for External IdP authentication
#[derive(Debug, Clone)]
pub struct ExternalIdpMetadata {
    /// OIDC issuer URL of customer's IdP
    pub issuer_url: String,
    /// OAuth2 client ID (public client, no secret)
    pub client_id: String,
    /// Space-separated OAuth2 scopes
    pub scopes: String,
    /// Pre-filled username/email for IdP login
    pub login_hint: Option<String>,
    /// Resource identifier for providers like Auth0
    pub audience: Option<String>,
}

/// OIDC Discovery document endpoints
#[derive(Debug, Clone, Deserialize)]
pub struct OidcDiscovery {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub issuer: String,
}

/// External IdP token stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalIdpToken {
    pub access_token: Secret,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: time::OffsetDateTime,
    pub refresh_token: Option<Secret>,
    pub issuer_url: String,
    pub token_endpoint: String,
    pub client_id: String,
}

impl ExternalIdpToken {
    const SECRET_KEY: &'static str = "kirocli:external-idp:token";

    pub fn is_expired(&self) -> bool {
        (time::OffsetDateTime::now_utc() + time::Duration::minutes(1)) > self.expires_at
    }

    /// Load token from database, refresh if expired
    pub async fn load(database: &Database) -> Result<Option<Self>, AuthError> {
        trace!("Loading external IdP token from secret store");
        match database.get_secret(Self::SECRET_KEY).await {
            Ok(Some(secret)) => {
                let token: Option<Self> = serde_json::from_str(&secret.0)?;
                match token {
                    Some(token) if token.is_expired() => {
                        trace!("External IdP token is expired, refreshing");
                        token.refresh_token(database).await
                    },
                    Some(token) => Ok(Some(token)),
                    None => Ok(None),
                }
            },
            Ok(None) => Ok(None),
            Err(err) => {
                error!(%err, "Error getting external IdP token");
                Err(err)?
            },
        }
    }

    /// Refresh the access token directly with the customer's IdP
    pub async fn refresh_token(&self, database: &Database) -> Result<Option<Self>, AuthError> {
        let Some(refresh_token) = &self.refresh_token else {
            warn!("No refresh token available");
            let _ = self.delete(database).await;
            return Ok(None);
        };

        debug!("Refreshing external IdP access token");
        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh_token.0),
            ("client_id", &self.client_id),
        ];

        match client
            .post(&self.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                let token_response: TokenResponse = response
                    .json()
                    .await
                    .map_err(|e| AuthError::OAuthCustomError(format!("Failed to parse token response: {e}")))?;
                let new_token = Self {
                    access_token: Secret(token_response.access_token),
                    expires_at: time::OffsetDateTime::now_utc()
                        + time::Duration::seconds(token_response.expires_in.unwrap_or(3600)),
                    refresh_token: token_response
                        .refresh_token
                        .map(Secret)
                        .or_else(|| self.refresh_token.clone()),
                    issuer_url: self.issuer_url.clone(),
                    token_endpoint: self.token_endpoint.clone(),
                    client_id: self.client_id.clone(),
                };
                let _ = new_token.save(database).await;
                Ok(Some(new_token))
            },
            Ok(response) => {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                error!(%status, %error_text, "Failed to refresh external IdP token");
                if status.is_client_error() {
                    let _ = self.delete(database).await;
                }
                Err(AuthError::OAuthCustomError(format!("Token refresh failed: {status}")))
            },
            Err(err) => Err(AuthError::OAuthCustomError(format!("Network error: {err}"))),
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
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default, deserialize_with = "deserialize_expires_in")]
    expires_in: Option<i64>,
}

/// Deserialize expires_in as number or string
fn deserialize_expires_in<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;
    match Option::<Value>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_i64()),
        Some(Value::String(s)) => s.parse().map(Some).map_err(serde::de::Error::custom),
        Some(_) => Ok(None),
    }
}

/// Fetch OIDC discovery document from the IdP
pub async fn fetch_oidc_discovery(issuer_url: &str) -> Result<OidcDiscovery, AuthError> {
    let discovery_url = format!("{}/.well-known/openid-configuration", issuer_url.trim_end_matches('/'));
    info!(%discovery_url, "Fetching OIDC discovery document");

    let client = reqwest::Client::new();
    let response = client
        .get(&discovery_url)
        .send()
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to fetch OIDC discovery: {e}")))?;

    if !response.status().is_success() {
        return Err(AuthError::OAuthCustomError(format!(
            "OIDC discovery failed: {}",
            response.status()
        )));
    }

    let discovery: OidcDiscovery = response
        .json()
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to parse OIDC discovery: {e}")))?;
    debug!(?discovery, "Fetched OIDC discovery document");
    Ok(discovery)
}

/// Start External IdP OAuth flow - CLI initiates the full OAuth flow
/// `exclude_port` - port to skip (the one used by portal server)
pub async fn start_external_idp_auth(
    database: &mut Database,
    metadata: ExternalIdpMetadata,
    exclude_port: Option<u16>,
) -> Result<(), AuthError> {
    info!(issuer_url = %metadata.issuer_url, "Starting External IdP authentication");

    // Step 1: Fetch OIDC discovery
    let discovery = fetch_oidc_discovery(&metadata.issuer_url).await?;

    // Step 2: Generate PKCE parameters
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state: String = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    // Step 3: Start local callback server (skip the port used by portal)
    let listener = bind_callback_port(CALLBACK_PORTS, exclude_port).await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{}/oauth/callback", port);

    info!(%port, %redirect_uri, "Started callback server for External IdP");

    // Step 4: Build authorization URL
    let auth_url = build_authorization_url(&AuthorizationUrlParams {
        authorization_endpoint: &discovery.authorization_endpoint,
        client_id: &metadata.client_id,
        redirect_uri: &redirect_uri,
        code_challenge: &code_challenge,
        state: &state,
        scopes: &metadata.scopes,
        login_hint: metadata.login_hint.as_deref(),
        audience: metadata.audience.as_deref(),
    });

    // Open browser
    crate::util::open::open_url_async(&auth_url)
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to open browser: {e}")))?;

    // Step 5: Wait for callback
    let auth_code = wait_for_callback(listener, state, DEFAULT_AUTHORIZATION_TIMEOUT, 3).await?;

    // Step 6: Exchange code for tokens
    let token = exchange_code_for_token(
        &discovery.token_endpoint,
        &metadata.client_id,
        &redirect_uri,
        &code_verifier,
        &auth_code,
        &discovery.issuer,
    )
    .await?;

    // Step 7: Save token
    token.save(database).await?;

    info!("External IdP authentication completed successfully");
    Ok(())
}

struct AuthorizationUrlParams<'a> {
    authorization_endpoint: &'a str,
    client_id: &'a str,
    redirect_uri: &'a str,
    code_challenge: &'a str,
    state: &'a str,
    scopes: &'a str,
    login_hint: Option<&'a str>,
    audience: Option<&'a str>,
}

fn build_authorization_url(params: &AuthorizationUrlParams<'_>) -> String {
    let mut url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}&scope={}",
        params.authorization_endpoint,
        urlencoding::encode(params.client_id),
        urlencoding::encode(params.redirect_uri),
        urlencoding::encode(params.code_challenge),
        urlencoding::encode(params.state),
        urlencoding::encode(params.scopes),
    );
    if let Some(hint) = params.login_hint {
        url.push_str(&format!("&login_hint={}", urlencoding::encode(hint)));
    }
    if let Some(aud) = params.audience {
        url.push_str(&format!("&audience={}", urlencoding::encode(aud)));
    }
    info!("Authorization URL: {}", url);
    url
}

async fn exchange_code_for_token(
    token_endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
    issuer_url: &str,
) -> Result<ExternalIdpToken, AuthError> {
    info!("Exchanging authorization code for tokens");

    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Token exchange failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(AuthError::OAuthCustomError(format!(
            "Token exchange failed: {status} - {error_text}"
        )));
    }

    let response_text = response
        .text()
        .await
        .map_err(|e| AuthError::OAuthCustomError(format!("Failed to read response: {e}")))?;

    debug!(%response_text, "Token endpoint response");

    let token_response: TokenResponse = serde_json::from_str(&response_text).map_err(|e| {
        AuthError::OAuthCustomError(format!("Failed to parse token response: {e}, body: {response_text}"))
    })?;

    Ok(ExternalIdpToken {
        access_token: Secret(token_response.access_token),
        expires_at: time::OffsetDateTime::now_utc()
            + time::Duration::seconds(token_response.expires_in.unwrap_or(3600)),
        refresh_token: token_response.refresh_token.map(Secret),
        issuer_url: issuer_url.to_string(),
        token_endpoint: token_endpoint.to_string(),
        client_id: client_id.to_string(),
    })
}

pub async fn logout_external_idp(database: &Database) -> Result<(), AuthError> {
    database.delete_secret(ExternalIdpToken::SECRET_KEY).await?;
    Ok(())
}

pub async fn is_external_idp_logged_in(database: &Database) -> bool {
    matches!(ExternalIdpToken::load(database).await, Ok(Some(_)))
}
