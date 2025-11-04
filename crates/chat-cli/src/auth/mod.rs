pub mod builder_id;
mod consts;
pub mod pkce;
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
/// Unified bearer token resolver that tries both social and builder ID tokens
#[derive(Debug, Clone)]
pub struct UnifiedBearerResolver;

impl ResolveIdentity for UnifiedBearerResolver {
    fn resolve_identity<'a>(
        &'a self,
        _runtime_components: &'a RuntimeComponents,
        _config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a> {
        IdentityFuture::new_boxed(Box::pin(async {
            let database = Database::new().await?;

            if let Ok(Some(token)) = builder_id::BuilderIdToken::load(&database, None).await {
                return Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                ));
            }

            if let Ok(Some(token)) = social::SocialToken::load(&database).await {
                return Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                ));
            }
            Err(AuthError::NoToken.into())
        }))
    }
}
