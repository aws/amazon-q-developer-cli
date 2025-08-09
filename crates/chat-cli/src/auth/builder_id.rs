//! # Builder ID
//!
//!  SSO flow (RFC: <https://tools.ietf.org/html/rfc8628>)
//!    1. Get a client id (SSO-OIDC identifier, formatted per RFC6749).
//!       - Code: [DeviceRegistration::register]
//!          - Calls [Client::register_client]
//!       - RETURNS: [DeviceRegistration]
//!       - Client registration is valid for potentially months and creates state server-side, so
//!         the client SHOULD cache them to disk.
//!    2. Start device authorization.
//!       - Code: [start_device_authorization]
//!          - Calls [Client::start_device_authorization]
//!       - RETURNS (RFC: <https://tools.ietf.org/html/rfc8628#section-3.2>):
//!         [StartDeviceAuthorizationResponse]
//!    3. Poll for the access token
//!       - Code: [poll_create_token]
//!          - Calls [Client::create_token]
//!       - RETURNS: [PollCreateToken]
//!    4. (Repeat) Tokens SHOULD be refreshed if expired and a refresh token is available.
//!        - Code: [BuilderIdToken::refresh_token]
//!          - Calls [Client::create_token]
//!        - RETURNS: [BuilderIdToken]

use aws_sdk_ssooidc::client::Client;
use aws_sdk_ssooidc::config::retry::RetryConfig;
use aws_sdk_ssooidc::config::{
    BehaviorVersion,
    ConfigBag,
    RuntimeComponents,
    SharedAsyncSleep,
};
use aws_sdk_ssooidc::error::SdkError;
use aws_sdk_ssooidc::operation::create_token::CreateTokenOutput;
use aws_sdk_ssooidc::operation::register_client::RegisterClientOutput;
use aws_smithy_async::rt::sleep::TokioSleep;
use aws_smithy_runtime_api::client::identity::http::Token;
use aws_smithy_runtime_api::client::identity::{
    Identity,
    IdentityFuture,
    ResolveIdentity,
};
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_types::region::Region;
use eyre::{
    Result,
    eyre,
};
use time::OffsetDateTime;
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use crate::api_client::stalled_stream_protection_config;
use crate::auth::AuthError;
use crate::auth::consts::*;
use crate::auth::scope::is_scopes;
use crate::aws_common::app_name;
use crate::database::{
    Database,
    Secret,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OAuthFlow {
    DeviceCode,
    // This must remain backwards compatible
    #[serde(alias = "PKCE")]
    Pkce,
}

/// Indicates if an expiration time has passed, there is a small 1 min window that is removed
/// so the token will not expire in transit
fn is_expired(expiration_time: &OffsetDateTime) -> bool {
    let now = time::OffsetDateTime::now_utc();
    &(now + time::Duration::minutes(1)) > expiration_time
}

pub(crate) fn oidc_url(region: &Region) -> String {
    format!("https://oidc.{region}.amazonaws.com")
}

pub fn client(region: Region) -> Client {
    Client::new(
        &aws_types::SdkConfig::builder()
            .http_client(crate::aws_common::http_client::client())
            .behavior_version(BehaviorVersion::v2025_01_17())
            .endpoint_url(oidc_url(&region))
            .region(region)
            .retry_config(RetryConfig::standard().with_max_attempts(3))
            .sleep_impl(SharedAsyncSleep::new(TokioSleep::new()))
            .stalled_stream_protection(stalled_stream_protection_config())
            .app_name(app_name())
            .build(),
    )
}

/// Represents an OIDC registered client, resulting from the "register client" API call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceRegistration {
    pub client_id: String,
    pub client_secret: Secret,
    #[serde(with = "time::serde::rfc3339::option")]
    pub client_secret_expires_at: Option<time::OffsetDateTime>,
    pub region: String,
    pub oauth_flow: OAuthFlow,
    pub scopes: Option<Vec<String>>,
}

impl DeviceRegistration {
    const SECRET_KEY: &'static str = "codewhisperer:odic:device-registration";

    pub fn from_output(
        output: RegisterClientOutput,
        region: &Region,
        oauth_flow: OAuthFlow,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            client_id: output.client_id.unwrap_or_default(),
            client_secret: output.client_secret.unwrap_or_default().into(),
            client_secret_expires_at: time::OffsetDateTime::from_unix_timestamp(output.client_secret_expires_at).ok(),
            region: region.to_string(),
            oauth_flow,
            scopes: Some(scopes),
        }
    }

    /// Loads the OIDC registered client from the secret store, deleting it if it is expired.
    async fn load_from_secret_store(database: &Database, region: &Region) -> Result<Option<Self>, AuthError> {
        trace!(?region, "loading device registration from secret store");
        let device_registration = database.get_secret(Self::SECRET_KEY).await?;

        if let Some(device_registration) = device_registration {
            // check that the data is not expired, assume it is invalid if not present
            let device_registration: Self = serde_json::from_str(&device_registration.0)?;

            if let Some(client_secret_expires_at) = device_registration.client_secret_expires_at {
                let is_expired = is_expired(&client_secret_expires_at);
                let registration_region_is_valid = device_registration.region == region.as_ref();
                trace!(
                    ?is_expired,
                    ?registration_region_is_valid,
                    "checking if device registration is valid"
                );
                if !is_expired && registration_region_is_valid {
                    return Ok(Some(device_registration));
                }
            } else {
                warn!("no expiration time found for the client secret");
            }
        }

        // delete the data if its expired or invalid
        if let Err(err) = database.delete_secret(Self::SECRET_KEY).await {
            error!(?err, "Failed to delete device registration from keychain");
        }

        Ok(None)
    }

    /// Loads the client saved in the secret store if available, otherwise registers a new client
    /// and saves it in the secret store.
    pub async fn init_device_code_registration(
        database: &Database,
        client: &Client,
        region: &Region,
    ) -> Result<Self, AuthError> {
        match Self::load_from_secret_store(database, region).await {
            Ok(Some(registration)) if registration.oauth_flow == OAuthFlow::DeviceCode => match &registration.scopes {
                Some(scopes) if is_scopes(scopes) => return Ok(registration),
                _ => warn!("Invalid scopes in device registration, ignoring"),
            },
            // If it doesn't exist or is for another OAuth flow,
            // then continue with creating a new one.
            Ok(None | Some(_)) => {},
            Err(err) => {
                error!(?err, "Failed to read device registration from keychain");
            },
        };

        let mut register = client
            .register_client()
            .client_name(CLIENT_NAME)
            .client_type(CLIENT_TYPE);
        for scope in SCOPES {
            register = register.scopes(*scope);
        }
        let output = register.send().await?;

        let device_registration = Self::from_output(
            output,
            region,
            OAuthFlow::DeviceCode,
            SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        );

        if let Err(err) = device_registration.save(database).await {
            error!(?err, "Failed to write device registration to keychain");
        }

        Ok(device_registration)
    }

    /// Saves to the passed secret store.
    pub async fn save(&self, secret_store: &Database) -> Result<(), AuthError> {
        secret_store
            .set_secret(Self::SECRET_KEY, &serde_json::to_string(&self)?)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartDeviceAuthorizationResponse {
    /// Device verification code.
    pub device_code: String,
    /// User verification code.
    pub user_code: String,
    /// Verification URI on the authorization server.
    pub verification_uri: String,
    /// User verification URI on the authorization server.
    pub verification_uri_complete: String,
    /// Lifetime (seconds) of `device_code` and `user_code`.
    pub expires_in: i32,
    /// Minimum time (seconds) the client SHOULD wait between polling intervals.
    pub interval: i32,
    pub region: String,
    pub start_url: String,
}

/// Init a builder id request
pub async fn start_device_authorization(
    database: &Database,
    start_url: Option<String>,
    region: Option<String>,
) -> Result<StartDeviceAuthorizationResponse, AuthError> {
    let region = region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);
    let client = client(region.clone());

    let DeviceRegistration {
        client_id,
        client_secret,
        ..
    } = DeviceRegistration::init_device_code_registration(database, &client, &region).await?;

    let output = client
        .start_device_authorization()
        .client_id(&client_id)
        .client_secret(&client_secret.0)
        .start_url(start_url.as_deref().unwrap_or(START_URL))
        .send()
        .await?;

    Ok(StartDeviceAuthorizationResponse {
        device_code: output.device_code.unwrap_or_default(),
        user_code: output.user_code.unwrap_or_default(),
        verification_uri: output.verification_uri.unwrap_or_default(),
        verification_uri_complete: output.verification_uri_complete.unwrap_or_default(),
        expires_in: output.expires_in,
        interval: output.interval,
        region: region.to_string(),
        start_url: start_url.unwrap_or_else(|| START_URL.to_owned()),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    BuilderId,
    IamIdentityCenter,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuilderIdToken {
    pub access_token: Secret,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: time::OffsetDateTime,
    pub refresh_token: Option<Secret>,
    pub region: Option<String>,
    pub start_url: Option<String>,
    pub oauth_flow: OAuthFlow,
    pub scopes: Option<Vec<String>>,
}

impl BuilderIdToken {
    const SECRET_KEY: &'static str = "codewhisperer:odic:token";

    #[cfg(test)]
    fn test() -> Self {
        Self {
            access_token: Secret("test_access_token".to_string()),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::minutes(60),
            refresh_token: Some(Secret("test_refresh_token".to_string())),
            region: Some(OIDC_BUILDER_ID_REGION.to_string()),
            start_url: Some(START_URL.to_string()),
            oauth_flow: OAuthFlow::DeviceCode,
            scopes: Some(SCOPES.iter().map(|s| (*s).to_owned()).collect()),
        }
    }

    /// Load the token from the keychain, refresh the token if it is expired and return it
    pub async fn load(database: &Database) -> Result<Option<Self>, AuthError> {
        // Can't use #[cfg(test)] without breaking lints, and we don't want to require
        // authentication in order to run ChatSession tests. Hence, adding this here with cfg!(test)
        if cfg!(test) {
            return Ok(Some(Self {
                access_token: Secret("test_access_token".to_string()),
                expires_at: time::OffsetDateTime::now_utc() + time::Duration::minutes(60),
                refresh_token: Some(Secret("test_refresh_token".to_string())),
                region: Some(OIDC_BUILDER_ID_REGION.to_string()),
                start_url: Some(START_URL.to_string()),
                oauth_flow: OAuthFlow::DeviceCode,
                scopes: Some(SCOPES.iter().map(|s| (*s).to_owned()).collect()),
            }));
        }

        trace!("loading builder id token from the secret store");
        match database.get_secret(Self::SECRET_KEY).await {
            Ok(Some(secret)) => {
                let token: Option<Self> = serde_json::from_str(&secret.0)?;
                match token {
                    Some(token) => {
                        let region = token.region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);
                        let client = client(region.clone());

                        if token.is_expired() {
                            trace!("token is expired, refreshing");
                            token.refresh_token(&client, database, &region).await
                        } else {
                            trace!(?token, "found a valid token");
                            Ok(Some(token))
                        }
                    },
                    None => {
                        debug!("secret stored in the database was empty");
                        Ok(None)
                    },
                }
            },
            Ok(None) => {
                debug!("no secret found in the database");
                Ok(None)
            },
            Err(err) => {
                error!(%err, "Error getting builder id token from keychain");
                Err(err)?
            },
        }
    }

    /// Refresh the access token
    pub async fn refresh_token(
        &self,
        client: &Client,
        database: &Database,
        region: &Region,
    ) -> Result<Option<Self>, AuthError> {
        let Some(refresh_token) = &self.refresh_token else {
            warn!("no refresh token was found");
            // if the token is expired and has no refresh token, delete it
            if let Err(err) = self.delete(database).await {
                error!(?err, "Failed to delete builder id token");
            }

            return Ok(None);
        };

        trace!("loading device registration from secret store");
        let registration = match DeviceRegistration::load_from_secret_store(database, region).await? {
            Some(registration) if registration.oauth_flow == self.oauth_flow => registration,
            // If the OIDC client registration is for a different oauth flow or doesn't exist, then
            // we can't refresh the token.
            Some(registration) => {
                warn!(
                    "Unable to refresh token: Stored client registration has oauth flow: {:?} but current access token has oauth flow: {:?}",
                    registration.oauth_flow, self.oauth_flow
                );
                return Ok(None);
            },
            None => {
                warn!("Unable to refresh token: No registered client was found");
                return Ok(None);
            },
        };

        debug!("Refreshing access token");
        match client
            .create_token()
            .client_id(registration.client_id)
            .client_secret(registration.client_secret.0)
            .refresh_token(&refresh_token.0)
            .grant_type(REFRESH_GRANT_TYPE)
            .send()
            .await
        {
            Ok(output) => {
                let token: BuilderIdToken = Self::from_output(
                    output,
                    region.clone(),
                    self.start_url.clone(),
                    self.oauth_flow,
                    self.scopes.clone(),
                );
                debug!("Refreshed access token, new token: {:?}", token);

                if let Err(err) = token.save(database).await {
                    error!(?err, "Failed to store builder id access token");
                };

                Ok(Some(token))
            },
            Err(err) => {
                let display_err = DisplayErrorContext(&err);
                error!("Failed to refresh builder id access token: {}", display_err);

                // if the error is the client's fault, clear the token
                if let SdkError::ServiceError(service_err) = &err {
                    if !service_err.err().is_slow_down_exception() {
                        if let Err(err) = self.delete(database).await {
                            error!(?err, "Failed to delete builder id token");
                        }
                    }
                }

                Err(err.into())
            },
        }
    }

    /// If the time has passed the `expires_at` time
    ///
    /// The token is marked as expired 1 min before it actually does to account for the potential a
    /// token expires while in transit
    pub fn is_expired(&self) -> bool {
        is_expired(&self.expires_at)
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

    pub(crate) fn from_output(
        output: CreateTokenOutput,
        region: Region,
        start_url: Option<String>,
        oauth_flow: OAuthFlow,
        scopes: Option<Vec<String>>,
    ) -> Self {
        Self {
            access_token: output.access_token.unwrap_or_default().into(),
            expires_at: time::OffsetDateTime::now_utc() + time::Duration::seconds(output.expires_in as i64),
            refresh_token: output.refresh_token.map(|t| t.into()),
            region: Some(region.to_string()),
            start_url,
            oauth_flow,
            scopes,
        }
    }

    pub fn token_type(&self) -> TokenType {
        match &self.start_url {
            Some(url) if url == START_URL => TokenType::BuilderId,
            None => TokenType::BuilderId,
            Some(_) => TokenType::IamIdentityCenter,
        }
    }

    /// Check if the token is for the internal amzn start URL (`https://amzn.awsapps.com/start`),
    /// this implies the user will use midway for private specs
    #[allow(dead_code)]
    pub fn is_amzn_user(&self) -> bool {
        matches!(&self.start_url, Some(url) if url == AMZN_START_URL)
    }
}

pub enum PollCreateToken {
    Pending,
    Complete,
    Error(AuthError),
}

/// Poll for the create token response
pub async fn poll_create_token(
    database: &Database,
    device_code: String,
    start_url: Option<String>,
    region: Option<String>,
) -> PollCreateToken {
    let region = region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);
    let client = client(region.clone());

    let DeviceRegistration {
        client_id,
        client_secret,
        scopes,
        ..
    } = match DeviceRegistration::init_device_code_registration(database, &client, &region).await {
        Ok(res) => res,
        Err(err) => {
            return PollCreateToken::Error(err);
        },
    };

    match client
        .create_token()
        .grant_type(DEVICE_GRANT_TYPE)
        .device_code(device_code)
        .client_id(client_id)
        .client_secret(client_secret.0)
        .send()
        .await
    {
        Ok(output) => {
            let token: BuilderIdToken =
                BuilderIdToken::from_output(output, region, start_url, OAuthFlow::DeviceCode, scopes);

            if let Err(err) = token.save(database).await {
                error!(?err, "Failed to store builder id token");
            };

            PollCreateToken::Complete
        },
        Err(SdkError::ServiceError(service_error)) if service_error.err().is_authorization_pending_exception() => {
            PollCreateToken::Pending
        },
        Err(err) => {
            error!(?err, "Failed to poll for builder id token");
            PollCreateToken::Error(err.into())
        },
    }
}

pub async fn is_logged_in(database: &mut Database) -> bool {
    // Check for BuilderId if not using Sigv4
    if std::env::var("AMAZON_Q_SIGV4").is_ok_and(|v| !v.is_empty()) {
        debug!("logged in using sigv4 credentials");
        return true;
    }

    match BuilderIdToken::load(database).await {
        Ok(Some(_)) => true,
        Ok(None) => {
            info!("not logged in - no valid token found");
            false
        },
        Err(err) => {
            warn!(?err, "failed to try to load a builder id token");
            false
        },
    }
}

pub async fn logout(database: &mut Database) -> Result<(), AuthError> {
    let Ok(secret_store) = Database::new().await else {
        return Ok(());
    };

    let (builder_res, device_res) = tokio::join!(
        secret_store.delete_secret(BuilderIdToken::SECRET_KEY),
        secret_store.delete_secret(DeviceRegistration::SECRET_KEY),
    );

    let profile_res = database.unset_auth_profile();

    builder_res?;
    device_res?;
    profile_res?;

    Ok(())
}

pub async fn get_start_url_and_region(database: &Database) -> (Option<String>, Option<String>) {
    // NOTE: Database provides direct methods to access the start_url and region, but they are not
    // guaranteed to be up to date in the chat session. Example: login is changed mid-chat session.
    let token = BuilderIdToken::load(database).await;
    match token {
        Ok(Some(t)) => (t.start_url, t.region),
        _ => (None, None),
    }
}

#[derive(Debug, Clone)]
pub struct BearerResolver;

impl ResolveIdentity for BearerResolver {
    fn resolve_identity<'a>(
        &'a self,
        _runtime_components: &'a RuntimeComponents,
        _config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a> {
        IdentityFuture::new_boxed(Box::pin(async {
            let database = Database::new().await?;
            match BuilderIdToken::load(&database).await? {
                Some(token) => Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                )),
                None => Err(AuthError::NoToken.into()),
            }
        }))
    }
}

pub async fn is_idc_user(database: &Database) -> Result<bool> {
    if cfg!(test) {
        return Ok(false);
    }
    if let Ok(Some(token)) = BuilderIdToken::load(database).await {
        Ok(token.token_type() == TokenType::IamIdentityCenter)
    } else {
        Err(eyre!("No auth token found - is the user signed in?"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const US_EAST_1: Region = Region::from_static("us-east-1");
    const US_WEST_2: Region = Region::from_static("us-west-2");

    #[test]
    fn test_oauth_flow_deser() {
        assert_eq!(OAuthFlow::Pkce, serde_json::from_str("\"PKCE\"").unwrap());
        assert_eq!(OAuthFlow::Pkce, serde_json::from_str("\"Pkce\"").unwrap());
    }

    #[tokio::test]
    async fn test_client() {
        println!("{:?}", client(US_EAST_1));
        println!("{:?}", client(US_WEST_2));
    }

    #[test]
    fn oidc_url_snapshot() {
        insta::assert_snapshot!(oidc_url(&US_EAST_1), @"https://oidc.us-east-1.amazonaws.com");
        insta::assert_snapshot!(oidc_url(&US_WEST_2), @"https://oidc.us-west-2.amazonaws.com");
    }

    #[test]
    fn test_is_expired() {
        let mut token = BuilderIdToken::test();
        assert!(!token.is_expired());

        token.expires_at = time::OffsetDateTime::now_utc() - time::Duration::seconds(60);
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_type() {
        let mut token = BuilderIdToken::test();
        assert_eq!(token.token_type(), TokenType::BuilderId);

        token.start_url = None;
        assert_eq!(token.token_type(), TokenType::BuilderId);

        token.start_url = Some("https://amzn.awsapps.com/start".into());
        assert_eq!(token.token_type(), TokenType::IamIdentityCenter);
    }
}
