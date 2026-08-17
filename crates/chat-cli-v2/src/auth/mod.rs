pub mod builder_id;
mod consts;
pub mod external_idp;
pub mod kas_token;
pub mod oauth_callback;
pub mod pkce;
pub mod refresh_coordinator;
mod scope;

pub mod portal;
pub mod social;
use aws_sdk_ssooidc::config::{
    ConfigBag,
    RuntimeComponents,
};
use aws_sdk_ssooidc::error::SdkError;
use aws_sdk_ssooidc::operation::create_token::CreateTokenError;
use aws_sdk_ssooidc::operation::register_client::RegisterClientError;
use aws_sdk_ssooidc::operation::start_device_authorization::StartDeviceAuthorizationError;
use aws_smithy_runtime_api::client::identity::http::Token;
use aws_smithy_runtime_api::client::identity::{
    Identity,
    IdentityFuture,
    ResolveIdentity,
};
pub use builder_id::{
    is_builder_id_logged_in,
    logout,
};
pub use consts::START_URL;
use thiserror::Error;

use crate::aws_common::SdkErrorDisplay;
use crate::database::Database;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    Ssooidc(Box<aws_sdk_ssooidc::Error>),
    #[error("{}", SdkErrorDisplay(.0))]
    SdkRegisterClient(Box<SdkError<RegisterClientError>>),
    #[error("{}", SdkErrorDisplay(.0))]
    SdkCreateToken(Box<SdkError<CreateTokenError>>),
    #[error("{}", SdkErrorDisplay(.0))]
    SdkStartDeviceAuthorization(Box<SdkError<StartDeviceAuthorizationError>>),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TimeComponentRange(#[from] time::error::ComponentRange),
    #[error(transparent)]
    Directories(#[from] crate::util::paths::DirectoryError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    DbOpenError(#[from] crate::database::DbOpenError),
    #[error("No token")]
    NoToken,
    #[error("OAuth state mismatch. Actual: {} | Expected: {}", .actual, .expected)]
    OAuthStateMismatch { actual: String, expected: String },
    #[error("Timeout waiting for authentication to complete")]
    OAuthTimeout,
    #[error("No code received on redirect")]
    OAuthMissingCode,
    #[error("OAuth error: {0}")]
    OAuthCustomError(String),
    #[error(transparent)]
    DatabaseError(#[from] crate::database::DatabaseError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("HTTP error: {0}")]
    HttpStatus(reqwest::StatusCode),
    #[error("Authentication failed: {0}")]
    SocialAuthProviderFailure(String),
    #[error("Social login failed: no profile ARN returned by the auth service")]
    MissingProfileArn,
    #[error("No profile selected. Please log in with `kiro-cli login` and select a profile.")]
    ProfileNotSelected,
}

impl From<aws_sdk_ssooidc::Error> for AuthError {
    fn from(value: aws_sdk_ssooidc::Error) -> Self {
        Self::Ssooidc(Box::new(value))
    }
}

impl From<SdkError<RegisterClientError>> for AuthError {
    fn from(value: SdkError<RegisterClientError>) -> Self {
        Self::SdkRegisterClient(Box::new(value))
    }
}

impl From<SdkError<CreateTokenError>> for AuthError {
    fn from(value: SdkError<CreateTokenError>) -> Self {
        Self::SdkCreateToken(Box::new(value))
    }
}

impl From<SdkError<StartDeviceAuthorizationError>> for AuthError {
    fn from(value: SdkError<StartDeviceAuthorizationError>) -> Self {
        Self::SdkStartDeviceAuthorization(Box::new(value))
    }
}

/// The source of the current authentication credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSource {
    BuilderId,
    Social,
    ExternalIdp,
    ApiKey,
}

/// Returns the active authentication source, if any.
///
/// Checks stored credentials first (BuilderId, Social, ExternalIdp),
/// then falls back to the KIRO_API_KEY environment variable.
pub async fn active_auth_source(db: &mut Database) -> Option<AuthSource> {
    if is_builder_id_logged_in(db).await {
        return Some(AuthSource::BuilderId);
    }
    if social::is_social_logged_in(&*db).await {
        return Some(AuthSource::Social);
    }
    if external_idp::is_external_idp_logged_in(&*db).await {
        return Some(AuthSource::ExternalIdp);
    }
    if crate::util::env_var::get_api_key().is_some() {
        return Some(AuthSource::ApiKey);
    }
    None
}

/// The token store that currently holds a credential, in [`UnifiedBearerResolver`]
/// priority order (external IdP, then Builder ID / IdC, then social) — i.e. the
/// source that supplied the token the last request was built with. Reads raw
/// secrets rather than `load()` so an expired token still identifies its source.
pub(crate) async fn active_stored_source(database: &Database) -> Option<AuthSource> {
    if external_idp::stored_token(database).await.is_some() {
        return Some(AuthSource::ExternalIdp);
    }
    if builder_id::stored_token(database).await.is_some() {
        return Some(AuthSource::BuilderId);
    }
    if social::stored_token(database).await.is_some() {
        return Some(AuthSource::Social);
    }
    None
}

/// Forces a network refresh of the active token, bypassing the not-yet-expired
/// cache. Used for mid-turn 401 recovery, where the cached token was accepted at
/// request-build time but has since been rejected by the backend.
///
/// Pinned to the source that supplied the rejected token (the highest-priority
/// stored credential): refreshing a lower-priority credential instead would
/// "recover" the turn under a different identity than the session resolved at
/// startup. Returns `true` only if that credential was refreshed; `false` when
/// no token is stored (e.g. API-key auth), the active token has no refresh
/// material, or its refresh fails — the retry then re-fails and surfaces the
/// auth error normally.
pub async fn force_refresh_active_token() -> bool {
    let database = match Database::new().await {
        Ok(db) => db,
        Err(err) => {
            tracing::warn!(%err, "force_refresh_active_token: failed to open auth store");
            return false;
        },
    };
    force_refresh_active_token_in(&database).await
}

/// [`force_refresh_active_token`] against a caller-supplied store, for tests
/// (the test store is per-handle in-memory, so the seeded handle must be reused).
pub(crate) async fn force_refresh_active_token_in(database: &Database) -> bool {
    let refreshed = match active_stored_source(database).await {
        Some(AuthSource::ExternalIdp) => external_idp::ExternalIdpToken::coordinated_refresh_inner(database, true)
            .await
            .map(|t| t.is_some()),
        Some(AuthSource::BuilderId) => builder_id::BuilderIdToken::coordinated_refresh_inner(database, None, true)
            .await
            .map(|t| t.is_some()),
        Some(AuthSource::Social) => social::SocialToken::coordinated_refresh_inner(database, true)
            .await
            .map(|t| t.is_some()),
        Some(AuthSource::ApiKey) | None => Ok(false),
    };
    match refreshed {
        Ok(refreshed) => refreshed,
        Err(err) => {
            tracing::warn!(%err, "force_refresh_active_token: refresh of the active credential failed");
            false
        },
    }
}

/// Unified bearer token resolver that tries external IdP, social, and builder ID tokens.
///
/// Clients using this resolver must pair it with `IdentityCache::no_cache()`:
/// resolution reads the token store, so per-request resolution is what makes a
/// mid-turn forced refresh (which rewrites the store) visible to the retry.
/// The SDK's default lazy cache would keep serving the rejected token until
/// shortly before its recorded expiry.
#[derive(Debug, Clone)]
pub struct UnifiedBearerResolver {
    /// Pinned store handle: per-request resolution must not pay `Database::new()`
    /// (settings load, pool construction, migration transaction) on every API
    /// call, and a store-open failure must not kill a chat request mid-turn.
    /// `get_secret` still queries the table on each call, so a refresh written
    /// by this or any other process is observed on the next request.
    database: Database,
}

impl UnifiedBearerResolver {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

impl ResolveIdentity for UnifiedBearerResolver {
    fn resolve_identity<'a>(
        &'a self,
        _runtime_components: &'a RuntimeComponents,
        _config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a> {
        IdentityFuture::new_boxed(Box::pin(async {
            let database = &self.database;

            if let Ok(Some(token)) = external_idp::ExternalIdpToken::load(database).await {
                return Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                ));
            }

            if let Ok(Some(token)) = builder_id::BuilderIdToken::load(database, None).await {
                return Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                ));
            }

            if let Ok(Some(token)) = social::SocialToken::load(database).await {
                return Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                ));
            }

            if let Some(api_key) = crate::util::env_var::get_api_key() {
                return Ok(Identity::new(Token::new(api_key, None), None));
            }

            Err(AuthError::NoToken.into())
        }))
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::builder_id::{
        BuilderIdToken,
        OAuthFlow,
    };
    use super::external_idp::ExternalIdpToken;
    use super::*;
    use crate::database::Secret;

    fn builder_id_token() -> BuilderIdToken {
        BuilderIdToken {
            access_token: Secret("builder-access-tok".into()),
            expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
            refresh_token: Some(Secret("builder-refresh-tok".into())),
            region: Some("us-east-1".into()),
            start_url: None,
            oauth_flow: OAuthFlow::DeviceCode,
            scopes: None,
        }
    }

    fn unrefreshable_external_idp_token() -> ExternalIdpToken {
        ExternalIdpToken {
            access_token: Secret("idp-access-tok".into()),
            expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
            refresh_token: None,
            issuer_url: "https://idp.example.com".into(),
            token_endpoint: "https://idp.example.com/token".into(),
            client_id: "client-123".into(),
            scopes: "openid offline_access".into(),
        }
    }

    /// The forced refresh must stay pinned to the source that supplied the
    /// rejected token: an unrefreshable active credential reports failure
    /// instead of "recovering" by refreshing a lower-priority identity.
    #[tokio::test]
    async fn force_refresh_pins_to_active_source_and_fails_closed() {
        let database = Database::new().await.unwrap();
        database
            .set_secret(
                ExternalIdpToken::SECRET_KEY,
                &serde_json::to_string(&unrefreshable_external_idp_token()).unwrap(),
            )
            .await
            .unwrap();
        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&builder_id_token()).unwrap(),
            )
            .await
            .unwrap();

        assert!(
            !force_refresh_active_token_in(&database).await,
            "an unrefreshable active credential must not report success via a lower-priority one"
        );
        let builder = builder_id::stored_token(&database).await.expect("still stored");
        assert_eq!(
            builder.access_token.0, "builder-access-tok",
            "the lower-priority credential must be left untouched"
        );
    }

    #[tokio::test]
    async fn force_refresh_reports_false_with_no_stored_token() {
        let database = Database::new().await.unwrap();
        assert!(!force_refresh_active_token_in(&database).await);
    }

    /// Source detection must follow `UnifiedBearerResolver` priority, since it
    /// identifies the provider whose token the failed request was built with.
    #[tokio::test]
    async fn active_stored_source_follows_resolver_priority() {
        let database = Database::new().await.unwrap();
        assert_eq!(active_stored_source(&database).await, None);

        database
            .set_secret(
                BuilderIdToken::SECRET_KEY,
                &serde_json::to_string(&builder_id_token()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(active_stored_source(&database).await, Some(AuthSource::BuilderId));

        database
            .set_secret(
                ExternalIdpToken::SECRET_KEY,
                &serde_json::to_string(&unrefreshable_external_idp_token()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(active_stored_source(&database).await, Some(AuthSource::ExternalIdp));
    }
}
