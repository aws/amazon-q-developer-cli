pub mod builder_id;
mod consts;
pub mod pkce;
mod scope;

use aws_sdk_ssooidc::error::SdkError;
use aws_sdk_ssooidc::operation::create_token::CreateTokenError;
use aws_sdk_ssooidc::operation::register_client::RegisterClientError;
use aws_sdk_ssooidc::operation::start_device_authorization::StartDeviceAuthorizationError;
pub use builder_id::{
    is_logged_in,
    logout,
};
pub use consts::START_URL;
use thiserror::Error;

use crate::agent::util::error::UtilError;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    Ssooidc(Box<aws_sdk_ssooidc::Error>),
    #[error(transparent)]
    SdkRegisterClient(Box<SdkError<RegisterClientError>>),
    #[error(transparent)]
    SdkCreateToken(Box<SdkError<CreateTokenError>>),
    #[error(transparent)]
    SdkStartDeviceAuthorization(Box<SdkError<StartDeviceAuthorizationError>>),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TimeComponentRange(#[from] time::error::ComponentRange),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Util(#[from] UtilError),
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
