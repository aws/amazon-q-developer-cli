pub mod builder_id;
mod consts;
pub mod pkce;
mod scope;

use aws_sdk_ssooidc::error::SdkError;
use aws_sdk_ssooidc::operation::create_token::CreateTokenError;
use aws_sdk_ssooidc::operation::register_client::RegisterClientError;
use aws_sdk_ssooidc::operation::start_device_authorization::StartDeviceAuthorizationError;
pub use builder_id::{
    builder_id_token,
    is_logged_in,
    logout,
    refresh_token,
};
pub use consts::START_URL;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    Ssooidc(#[from] Box<aws_sdk_ssooidc::Error>),
    #[error(transparent)]
    SdkRegisterClient(#[from] SdkError<RegisterClientError>),
    #[error(transparent)]
    SdkCreateToken(#[from] SdkError<CreateTokenError>),
    #[error(transparent)]
    SdkStartDeviceAuthorization(#[from] SdkError<StartDeviceAuthorizationError>),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TimeComponentRange(#[from] time::error::ComponentRange),
    #[error(transparent)]
    Directories(#[from] crate::util::directories::DirectoryError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    DbOpenError(#[from] crate::database::DbOpenError),
    #[error(transparent)]
    Setting(#[from] crate::database::DatabaseError),
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
