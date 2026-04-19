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
use eyre::Result;
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use crate::auth::AuthError;
use crate::auth::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
pub use crate::auth::oauth_callback::CALLBACK_PORTS;
use crate::database::settings::Setting;
use crate::database::{
    Database,
    Secret,
};

const USER_AGENT: &str = "Kiro-CLI";
const DEFAULT_SOCIAL_PROFILE_ARN: &str = "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum SocialProvider {
    #[serde(rename = "google", alias = "Google")]
    #[value(name = "google")]
    Google,
    #[serde(rename = "github", alias = "Github", alias = "GitHub")]
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
    const SECRET_KEY: &'static str = "kirocli:social:token";

    pub async fn load(database: &Database) -> Result<Option<Self>, AuthError> {
        if cfg!(test) {
            return Ok(Some(Self {
                access_token: Secret("test_access_token".to_string()),
                expires_at: OffsetDateTime::now_utc() + time::Duration::minutes(60),
                refresh_token: Some(Secret("test_refresh_token".to_string())),
                provider: SocialProvider::Google,
                profile_arn: Some("arn:aws:iam::123456789012:profile/TestProfile".to_string()),
            }));
        }

        trace!("loading social token from the secret store");
        match database.get_secret(Self::SECRET_KEY).await {
            Ok(Some(secret)) => {
                let token: Option<Self> = serde_json::from_str(&secret.0)?;
                match token {
                    Some(mut token) => {
                        // Reject legacy tokens that were saved without a profile ARN
                        if token.profile_arn.as_ref().is_none_or(|arn| arn.is_empty()) {
                            debug!("social token has no profile ARN, treating as invalid");
                            token.delete(database).await.ok();
                            return Ok(None);
                        }
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
                profile_name: "Social_Default_Profile".to_string(),
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
        let refresh_token = self.refresh_token.as_ref().ok_or_else(|| {
            error!("No refresh token available for social login");
            AuthError::NoToken
        })?;

        debug!("Refreshing social access token for provider: {}", self.provider);

        let client = Client::new();
        let response = client
            .post(format!("{}/refreshToken", get_kiro_auth_endpoint(database)))
            .header("Content-Type", "application/json")
            .header("User-Agent", USER_AGENT)
            .json(&serde_json::json!({
                "refreshToken": refresh_token.0
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            error!("Failed to refresh social token: {}", status);

            // Clean up invalid token
            self.delete(database).await.ok();

            return Err(AuthError::HttpStatus(status));
        }

        let token_response: TokenResponse = response.json().await?;
        let profile_arn = if token_response.profile_arn.is_empty() {
            self.profile_arn.clone()
        } else {
            Some(token_response.profile_arn)
        };

        if profile_arn.as_ref().is_none_or(|arn| arn.is_empty()) {
            error!(
                "Social token refresh for {} returned no profile ARN and no existing ARN available",
                self.provider
            );
            self.delete(database).await.ok();
            return Err(AuthError::MissingProfileArn);
        }

        let new_token = Self {
            access_token: Secret(token_response.access_token),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider: self.provider,
            profile_arn,
        };

        new_token.save(database).await?;
        debug!("Successfully refreshed social token");

        Ok(new_token)
    }

    pub async fn exchange_social_token(
        database: &mut Database,
        provider: SocialProvider,
        code_verifier: &str,
        code: &str,
        redirect_uri: &str,
    ) -> Result<(), AuthError> {
        debug!("Exchanging authorization code for {} token", provider);

        let client = Client::new();
        let token_request = serde_json::json!({
            "code": code,
            "code_verifier": code_verifier,
            "redirect_uri": redirect_uri,
        });

        let response = client
            .post(format!("{}/oauth/token", get_kiro_auth_endpoint(database)))
            .header("Content-Type", "application/json")
            .header("User-Agent", USER_AGENT)
            .json(&token_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

            error!("Token exchange failed: {} - {}", status, body);
            return Err(AuthError::SocialAuthProviderFailure(format!(
                "Token exchange failed: {body}"
            )));
        }
        let token_response: TokenResponse = response.json().await?;

        if token_response.profile_arn.is_empty() {
            error!(
                "Social login for {} failed: auth service returned an empty profile ARN",
                provider
            );
            return Err(AuthError::MissingProfileArn);
        }

        let token = Self {
            access_token: Secret(token_response.access_token),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider,
            profile_arn: Some(token_response.profile_arn),
        };

        token.save(database).await?;
        token.save_profile_if_any(database).await?;

        info!("Successfully obtained and saved {} access token", provider);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    #[serde(rename = "profileArn")]
    profile_arn: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SocialBearerResolver;

impl ResolveIdentity for SocialBearerResolver {
    fn resolve_identity<'a>(
        &'a self,
        _runtime_components: &'a RuntimeComponents,
        _config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a> {
        IdentityFuture::new_boxed(Box::pin(async {
            let database = Database::new_default().await?;
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
    // Read the refresh token before deleting, so we can revoke it server-side.
    let refresh_token = database
        .get_secret(SocialToken::SECRET_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Option<SocialToken>>(&s.0).ok().flatten())
        .and_then(|t| t.refresh_token);

    // Delete local token first — user is immediately logged out.
    database.delete_secret(SocialToken::SECRET_KEY).await?;

    // Then revoke the token server-side. Failures are non-fatal.
    if let Some(refresh_token) = refresh_token {
        let endpoint = get_kiro_auth_endpoint(database);
        match Client::new()
            .post(format!("{endpoint}/logout"))
            .header("Content-Type", "application/json")
            .header("User-Agent", USER_AGENT)
            .json(&serde_json::json!({ "refreshToken": refresh_token.0 }))
            .send()
            .await
        {
            Ok(resp) if !resp.status().is_success() => {
                debug!("server-side token revocation returned {}", resp.status());
            },
            Err(err) => {
                debug!(%err, "server-side token revocation failed");
            },
            _ => {},
        }
    }
    Ok(())
}

/// Get the Kiro auth service endpoint from setting, or use prod as default
fn get_kiro_auth_endpoint(database: &Database) -> String {
    database
        .settings
        .get(Setting::ApiKiroAuthService)
        .and_then(|v| v.as_str())
        .map_or_else(|| SOCIAL_AUTH_SERVICE_ENDPOINT.to_string(), |s| s.to_string())
}

/// Response from the device authorization endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorizationResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    #[serde(rename = "expiresInMilliseconds")]
    pub expires_in_ms: u64,
    #[serde(rename = "intervalInMilliseconds")]
    pub interval_ms: u64,
}

/// Possible outcomes when polling for device authorization
#[derive(Debug)]
pub enum DevicePollResult {
    Pending,
    Complete { provider: SocialProvider },
    Expired,
    Error(String),
}

/// Response from the device poll endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePollResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    identity_provider: Option<SocialProvider>,
    expires_in: Option<u64>,
    profile_arn: Option<String>,
    status: String,
}

/// Step 1: Initiate device authorization – returns codes for the user to enter.
pub async fn initiate_social_device_authorization(
    database: &Database,
    provider: SocialProvider,
) -> Result<DeviceAuthorizationResponse, AuthError> {
    let client = Client::new();
    let url = format!("{}/oauth/device/authorization", get_kiro_auth_endpoint(database));

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&serde_json::json!({
            "clientId": USER_AGENT,
            // API expects PascalCase enum values; serde uses lowercase for token storage compatibility
            "loginProvider": match provider {
                SocialProvider::Google => "Google",
                SocialProvider::Github => "Github",
            },
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!(%status, %body, "Device authorization request failed");
        return Err(AuthError::SocialAuthProviderFailure(format!(
            "Device authorization failed: HTTP {status} - {body}"
        )));
    }

    let auth: DeviceAuthorizationResponse = resp.json().await?;
    info!(user_code = %auth.user_code, verification_uri = %auth.verification_uri, "Device authorization initiated");
    Ok(auth)
}

/// Step 2: Poll for device token. Returns [`DevicePollResult`].
pub async fn poll_device_token(
    database: &mut Database,
    device_code: &str,
    provider: SocialProvider,
) -> Result<DevicePollResult, AuthError> {
    let client = Client::new();
    let url = format!("{}/oauth/device/poll", get_kiro_auth_endpoint(database));

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&serde_json::json!({
            "deviceCode": device_code,
            "clientId": USER_AGENT,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(%status, %body, "Device poll returned error status");
        return Ok(DevicePollResult::Error(format!("HTTP {status}: {body}")));
    }

    let poll: DevicePollResponse = resp.json().await?;

    match poll.status.as_str() {
        "authorization_pending" => Ok(DevicePollResult::Pending),
        "expired_token" => Ok(DevicePollResult::Expired),
        "invalid_token" => Ok(DevicePollResult::Error("invalid_token".into())),
        "authorized" => {
            let access_token = poll
                .access_token
                .ok_or_else(|| AuthError::SocialAuthProviderFailure("Missing accessToken in poll response".into()))?;
            let refresh_token = poll
                .refresh_token
                .ok_or_else(|| AuthError::SocialAuthProviderFailure("Missing refreshToken in poll response".into()))?;
            let provider = poll.identity_provider.unwrap_or(provider);

            let token = SocialToken {
                access_token: Secret(access_token),
                expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(poll.expires_in.unwrap_or(3600) as i64),
                refresh_token: Some(Secret(refresh_token)),
                provider,
                profile_arn: Some(
                    poll.profile_arn
                        .unwrap_or_else(|| DEFAULT_SOCIAL_PROFILE_ARN.to_string()),
                ),
            };
            token.save(database).await?;
            token.save_profile_if_any(database).await?;
            info!("Social device flow login completed successfully");

            Ok(DevicePollResult::Complete { provider })
        },
        other => Ok(DevicePollResult::Error(other.to_string())),
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_social_provider_display() {
        assert_eq!(SocialProvider::Google.to_string(), "Google");
        assert_eq!(SocialProvider::Github.to_string(), "GitHub");
    }

    #[test]
    fn test_social_token_is_expired() {
        let mut token = SocialToken {
            access_token: Secret("a".into()),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(120),
            refresh_token: Some(Secret("r".into())),
            provider: SocialProvider::Google,
            profile_arn: None,
        };
        assert!(!token.is_expired(), "fresh token should not be expired");

        token.expires_at = OffsetDateTime::now_utc() - time::Duration::seconds(1);
        assert!(token.is_expired(), "past token should be expired");
    }

    #[test]
    fn test_token_response_deser() {
        // matches camelCase keys from the social auth service
        let json = r#"
        {
          "accessToken": "acc",
          "refreshToken": "ref",
          "expiresIn": 3600,
          "profileArn": "arn:aws:iam::123456789012:role/Demo"
        }
        "#;

        let tr: TokenResponse = serde_json::from_str(json).expect("deser ok");
        assert_eq!(tr.access_token, "acc");
        assert_eq!(tr.refresh_token, "ref");
        assert_eq!(tr.expires_in, 3600);
        assert_eq!(tr.profile_arn.as_str(), "arn:aws:iam::123456789012:role/Demo");
    }

    #[test]
    fn test_token_response_missing_profile_arn_fails() {
        let json = r#"
        {
          "accessToken": "acc",
          "refreshToken": "ref",
          "expiresIn": 3600
        }
        "#;

        let result: Result<TokenResponse, _> = serde_json::from_str(json);
        assert!(result.is_err(), "missing profileArn should fail deserialization");
    }

    #[test]
    fn test_token_response_empty_profile_arn_deserializes() {
        let json = r#"
        {
          "accessToken": "acc",
          "refreshToken": "ref",
          "expiresIn": 3600,
          "profileArn": ""
        }
        "#;

        let tr: TokenResponse = serde_json::from_str(json).expect("deser ok");
        assert_eq!(tr.profile_arn.as_str(), "");
    }

    #[test]
    fn test_social_token_load_returns_profile_arn() {
        // cfg!(test) stub in load() should return a token with a profile ARN
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let database = crate::database::Database::new_default().await.unwrap();
            let token = SocialToken::load(&database).await.unwrap();
            assert!(token.is_some(), "test stub should return Some");
            let token = token.unwrap();
            assert!(token.profile_arn.is_some(), "test stub should have a profile ARN");
            assert!(
                !token.profile_arn.as_ref().unwrap().is_empty(),
                "test stub profile ARN should not be empty"
            );
        });
    }

    #[test]
    fn test_deserialize_device_authorization_response() {
        let json = r#"{
            "deviceCode": "abc-123",
            "userCode": "ABCD-EFGH",
            "verificationUri": "https://example.com/device",
            "verificationUriComplete": "https://example.com/device?code=ABCD-EFGH",
            "expiresInMilliseconds": 600000,
            "intervalInMilliseconds": 5000
        }"#;
        let resp: DeviceAuthorizationResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.device_code, "abc-123");
        assert_eq!(resp.user_code, "ABCD-EFGH");
        assert_eq!(resp.verification_uri, "https://example.com/device");
        assert_eq!(resp.expires_in_ms, 600000);
        assert_eq!(resp.interval_ms, 5000);
    }

    #[test]
    fn test_device_poll_response_deser() {
        let json = r#"
        {
          "accessToken": "at",
          "refreshToken": "rt",
          "identityProvider": "google",
          "expiresIn": 7200,
          "profileArn": "arn:aws:iam::123456789012:role/Demo",
          "status": "complete"
        }
        "#;

        let resp: DevicePollResponse = serde_json::from_str(json).expect("deser ok");
        assert_eq!(resp.access_token.as_deref(), Some("at"));
        assert_eq!(resp.refresh_token.as_deref(), Some("rt"));
        assert_eq!(resp.identity_provider, Some(SocialProvider::Google));
        assert_eq!(resp.expires_in, Some(7200));
        assert_eq!(resp.status, "complete");
    }

    #[test]
    fn test_device_poll_response_pending() {
        let json = r#"{ "status": "authorization_pending" }"#;

        let resp: DevicePollResponse = serde_json::from_str(json).expect("deser ok");
        assert_eq!(resp.status, "authorization_pending");
        assert!(resp.access_token.is_none());
        assert!(resp.identity_provider.is_none());
    }
}
