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
};

use crate::auth::AuthError;
use crate::auth::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
use crate::database::{
    Database,
    Secret,
};

// NOTE: We use a fixed set of callback ports (not random) because:
// - IdP/Cognito only accepts pre-registered redirect URIs.
// - This list must match the Cognito allowlist
// - Bind only on loopback (127.0.0.1); never expose externally.
// - If all ports are in use, show a clear error.
// IMPORTANT: Do not change without auth service coordination.
pub const CALLBACK_PORTS: &[u16] = &[3128, 4649, 6588, 8008, 9091, 49153, 50153, 51153, 52153, 53153];
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
            .post(format!("{}/refreshToken", SOCIAL_AUTH_SERVICE_ENDPOINT))
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
            self.delete(database).await?;

            return Err(AuthError::HttpStatus(status));
        }

        let token_response: TokenResponse = response.json().await?;
        let new_token = Self {
            access_token: Secret(token_response.access_token),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider: self.provider,
            profile_arn: token_response.profile_arn.or_else(|| self.profile_arn.clone()),
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

        // Send request
        let response = client
            .post(format!("{}/oauth/token", SOCIAL_AUTH_SERVICE_ENDPOINT))
            .header("Content-Type", "application/json")
            .header("User-Agent", USER_AGENT)
            .json(&token_request)
            .send()
            .await?;

        // Handle response
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

            error!("Token exchange failed: {} - {}", status, body);
            return Err(AuthError::SocialAuthProviderFailure(format!(
                "Token exchange failed: {}",
                body
            )));
        }
        let token_response: TokenResponse = response.json().await?;
        let token = Self {
            access_token: Secret(token_response.access_token),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider,
            profile_arn: token_response.profile_arn,
        };

        token.save(database).await?;
        token.save_profile_if_any(database).await?;

        info!("Successfully obtained and saved {} access token", provider);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresIn")]
    expires_in: u64,
    #[serde(rename = "profileArn")]
    profile_arn: Option<String>,
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
        assert_eq!(tr.profile_arn.as_deref(), Some("arn:aws:iam::123456789012:role/Demo"));
    }
}
