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
use aws_types::request_id::RequestId;
use fig_aws_common::app_name;
use fig_telemetry_core::{
    Event,
    EventType,
    TelemetryResult,
};
use fig_util::directories::credential_file_path;
use time::OffsetDateTime;
use tracing::{
    debug,
    error,
    warn,
};

use crate::consts::*;
use crate::scope::is_scopes;
use crate::secret_store::{
    Secret,
    SecretStore,
};
use crate::{
    Error,
    Result,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OAuthFlow {
    DeviceCode,
    PKCE,
}

impl std::fmt::Display for OAuthFlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            OAuthFlow::DeviceCode => write!(f, "DeviceCode"),
            OAuthFlow::PKCE => write!(f, "PKCE"),
        }
    }
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

pub(crate) fn client(region: Region) -> Client {
    let retry_config = RetryConfig::standard().with_max_attempts(3);
    let sdk_config = aws_types::SdkConfig::builder()
        .http_client(fig_aws_common::http_client::client())
        .behavior_version(BehaviorVersion::v2024_03_28())
        .endpoint_url(oidc_url(&region))
        .region(region)
        .retry_config(retry_config)
        .sleep_impl(SharedAsyncSleep::new(TokioSleep::new()))
        .app_name(app_name())
        .build();
    Client::new(&sdk_config)
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
    async fn load_from_secret_store(secret_store: &SecretStore, region: &Region) -> Result<Option<Self>> {
        let device_registration = secret_store.get(Self::SECRET_KEY).await?;

        if let Some(device_registration) = device_registration {
            // check that the data is not expired, assume it is invalid if not present
            let device_registration: Self = serde_json::from_str(&device_registration.0)?;

            if let Some(client_secret_expires_at) = device_registration.client_secret_expires_at {
                if !is_expired(&client_secret_expires_at) && device_registration.region == region.as_ref() {
                    return Ok(Some(device_registration));
                }
            }
        }

        // delete the data if its expired or invalid
        if let Err(err) = secret_store.delete(Self::SECRET_KEY).await {
            error!(?err, "Failed to delete device registration from keychain");
        }

        Ok(None)
    }

    /// Loads the client saved in the secret store if available, otherwise registers a new client
    /// and saves it in the secret store.
    pub async fn init_device_code_registration(
        client: &Client,
        secret_store: &SecretStore,
        region: &Region,
    ) -> Result<Self> {
        match Self::load_from_secret_store(secret_store, region).await {
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

        if let Err(err) = device_registration.save(secret_store).await {
            error!(?err, "Failed to write device registration to keychain");
        }

        Ok(device_registration)
    }

    /// Saves to the passed secret store.
    pub async fn save(&self, secret_store: &SecretStore) -> Result<()> {
        secret_store
            .set(Self::SECRET_KEY, &serde_json::to_string(&self)?)
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
    secret_store: &SecretStore,
    start_url: Option<String>,
    region: Option<String>,
) -> Result<StartDeviceAuthorizationResponse> {
    let region = region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);
    let client = client(region.clone());

    let DeviceRegistration {
        client_id,
        client_secret,
        ..
    } = DeviceRegistration::init_device_code_registration(&client, secret_store, &region).await?;

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
    pub async fn load(secret_store: &SecretStore, force_refresh: bool) -> Result<Option<Self>> {
        match secret_store.get(Self::SECRET_KEY).await {
            Ok(Some(secret)) => {
                let token: Option<Self> = serde_json::from_str(&secret.0)?;
                match token {
                    Some(token) => {
                        let region = token.region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);

                        let client = client(region.clone());
                        // if token is expired try to refresh
                        let token = if token.is_expired() || force_refresh {
                            token.refresh_token(&client, secret_store, &region).await?
                        } else {
                            Some(token)
                        };

                        if let Some(ref token) = token {
                            if let Err(err) = write_credentials_to_file(token).await {
                                error!(?err, "Failed to write credentials to file");
                            }
                        }

                        Ok(token)
                    },
                    None => Ok(None),
                }
            },
            Ok(None) => Ok(None),
            Err(err) => {
                error!(%err, "Error getting builder id token from keychain");
                Err(err)
            },
        }
    }

    /// Refresh the access token
    pub async fn refresh_token(
        &self,
        client: &Client,
        secret_store: &SecretStore,
        region: &Region,
    ) -> Result<Option<Self>> {
        let Some(refresh_token) = &self.refresh_token else {
            // if the token is expired and has no refresh token, delete it
            if let Err(err) = self.delete(secret_store).await {
                error!(?err, "Failed to delete builder id token");
            }

            return Ok(None);
        };

        let registration = match DeviceRegistration::load_from_secret_store(secret_store, region).await? {
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
                fig_telemetry_core::send_event(
                    Event::new(EventType::RefreshCredentials {
                        request_id: output.request_id().unwrap_or_default().into(),
                        result: TelemetryResult::Succeeded,
                        reason: None,
                        oauth_flow: registration.oauth_flow.to_string(),
                    })
                    .with_credential_start_url(self.start_url.clone().unwrap_or_else(|| START_URL.to_owned())),
                )
                .await;

                let token: BuilderIdToken = Self::from_output(
                    output,
                    region.clone(),
                    self.start_url.clone(),
                    self.oauth_flow,
                    self.scopes.clone(),
                );
                debug!("Refreshed access token, new token: {:?}", token);

                if let Err(err) = token.save(secret_store).await {
                    error!(?err, "Failed to store builder id access token");
                };

                Ok(Some(token))
            },
            Err(err) => {
                let display_err = DisplayErrorContext(&err);
                error!("Failed to refresh builder id access token: {}", display_err);

                // if the error is the client's fault, clear the token
                if let SdkError::ServiceError(service_err) = &err {
                    fig_telemetry_core::send_event(
                        Event::new(EventType::RefreshCredentials {
                            request_id: err.request_id().unwrap_or_default().into(),
                            result: TelemetryResult::Failed,
                            reason: Some(display_err.to_string()),
                            oauth_flow: registration.oauth_flow.to_string(),
                        })
                        .with_credential_start_url(self.start_url.clone().unwrap_or_else(|| START_URL.to_owned())),
                    )
                    .await;
                    if !service_err.err().is_slow_down_exception() {
                        if let Err(err) = self.delete(secret_store).await {
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
    pub async fn save(&self, secret_store: &SecretStore) -> Result<()> {
        secret_store
            .set(Self::SECRET_KEY, &serde_json::to_string(self)?)
            .await?;
        Ok(())
    }

    /// Delete the token from the keychain
    pub async fn delete(&self, secret_store: &SecretStore) -> Result<()> {
        secret_store.delete(Self::SECRET_KEY).await?;
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
    pub fn is_amzn_user(&self) -> bool {
        matches!(&self.start_url, Some(url) if url == AMZN_START_URL)
    }
}

pub enum PollCreateToken {
    Pending,
    Complete(BuilderIdToken),
    Error(Error),
}

/// Poll for the create token response
pub async fn poll_create_token(
    secret_store: &SecretStore,
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
    } = match DeviceRegistration::init_device_code_registration(&client, secret_store, &region).await {
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

            if let Err(err) = token.save(secret_store).await {
                error!(?err, "Failed to store builder id token");
            };

            PollCreateToken::Complete(token)
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

/// Write credentials to ~/.aws/amazonq/creds.json
pub async fn write_credentials_to_file(token: &BuilderIdToken) -> Result<()> {
    let creds = serde_json::json!({
       "access_token": token.access_token.0.clone(),
       "region": token.region.clone(),
    });

    let path = credential_file_path()?;
    std::fs::write(&path, serde_json::to_string_pretty(&creds)?)?;

    debug!("Wrote credentials to {}", path.display());

    Ok(())
}

pub async fn builder_id_token() -> Result<Option<BuilderIdToken>> {
    let secret_store = SecretStore::new().await?;
    BuilderIdToken::load(&secret_store, false).await
}

pub async fn refresh_token() -> Result<Option<BuilderIdToken>> {
    let secret_store = SecretStore::new().await?;
    BuilderIdToken::load(&secret_store, true).await
}

pub async fn is_amzn_user() -> Result<bool> {
    Ok(builder_id_token().await?.is_some_and(|t| t.is_amzn_user()))
}

pub async fn is_logged_in() -> bool {
    matches!(builder_id_token().await, Ok(Some(_)))
}

pub async fn logout() -> Result<()> {
    let Ok(secret_store) = SecretStore::new().await else {
        return Ok(());
    };

    let (builder_res, device_res) = tokio::join!(
        secret_store.delete(BuilderIdToken::SECRET_KEY),
        secret_store.delete(DeviceRegistration::SECRET_KEY),
    );

    builder_res?;
    device_res?;

    Ok(())
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
            let secret_store = SecretStore::new().await?;
            let token = BuilderIdToken::load(&secret_store, false).await?;
            match token {
                Some(token) => Ok(Identity::new(
                    Token::new(token.access_token.0, Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                )),
                None => Err(Error::NoToken.into()),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const US_EAST_1: Region = Region::from_static("us-east-1");
    const US_WEST_2: Region = Region::from_static("us-west-2");

    macro_rules! test_ser_deser {
        ($ty:ident, $variant:expr, $text:expr) => {
            let quoted = format!("\"{}\"", $text);
            assert_eq!(quoted, serde_json::to_string(&$variant).unwrap());
            assert_eq!($variant, serde_json::from_str(&quoted).unwrap());

            assert_eq!($text, format!("{}", $variant));
        };
    }

    #[test]
    fn test_oauth_flow_ser_deser() {
        test_ser_deser!(OAuthFlow, OAuthFlow::DeviceCode, "DeviceCode");
        test_ser_deser!(OAuthFlow, OAuthFlow::PKCE, "PKCE");
    }

    #[test]
    fn test_client() {
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
        assert!(!token.is_amzn_user());

        token.start_url = None;
        assert_eq!(token.token_type(), TokenType::BuilderId);
        assert!(!token.is_amzn_user());

        token.start_url = Some(AMZN_START_URL.into());
        assert_eq!(token.token_type(), TokenType::IamIdentityCenter);
        assert!(token.is_amzn_user());
    }

    #[ignore = "not in ci"]
    #[tokio::test]
    async fn logout_test() {
        logout().await.unwrap();
    }

    #[ignore = "login flow"]
    #[tokio::test]
    async fn test_login() {
        let start_url = Some(AMZN_START_URL.into());
        let region = Some("us-east-1".into());

        // let start_url = None;
        // let region = None;

        let secret_store = SecretStore::new().await.unwrap();
        let res: StartDeviceAuthorizationResponse =
            start_device_authorization(&secret_store, start_url.clone(), region.clone())
                .await
                .unwrap();

        println!("{:?}", res);

        loop {
            match poll_create_token(
                &secret_store,
                res.device_code.clone(),
                start_url.clone(),
                region.clone(),
            )
            .await
            {
                PollCreateToken::Pending => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                },
                PollCreateToken::Complete(token) => {
                    println!("{:?}", token);
                    break;
                },
                PollCreateToken::Error(err) => {
                    println!("{}", err);
                    break;
                },
            }
        }
    }

    #[ignore = "not in ci"]
    #[tokio::test]
    async fn test_load() {
        use std::fs;
        use std::path::Path;

        // Create a secret store
        let secret_store = SecretStore::new().await.unwrap();

        // Load the token
        let token_result = BuilderIdToken::load(&secret_store, false).await;
        println!("{:?}", token_result);

        // If a token was successfully loaded
        if let Ok(Some(token)) = token_result {
            // Check that the credentials file exists
            let creds_path = credential_file_path().unwrap();
            assert!(Path::new(&creds_path).exists(), "Credentials file was not created");

            // Read the credentials file and parse it
            let creds_content = fs::read_to_string(&creds_path).unwrap();
            let creds: serde_json::Value = serde_json::from_str(&creds_content).unwrap();

            // Verify the credentials file contains the expected data
            assert_eq!(
                creds["access_token"], token.access_token.0,
                "Access token in file doesn't match"
            );
            assert_eq!(
                creds["region"].as_str(),
                token.region.as_deref(),
                "Region in file doesn't match"
            );

            println!("Credentials were successfully written to {}", creds_path.display());
        } else {
            panic!("No token was loaded, failing test!");
        }
    }

    #[ignore = "not in ci"]
    #[tokio::test]
    async fn test_refresh() {
        let token = refresh_token().await.unwrap().unwrap();
        println!("{:?}", token);
    }
}
