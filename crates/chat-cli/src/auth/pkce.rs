//! # OAuth 2.0 Proof Key for Code Exchange
//!
//! This module implements the PKCE integration with AWS OIDC according to their
//! developer guide.
//!
//! The benefit of PKCE over device code is to simplify the user experience by not
//! requiring the user to validate the generated code across the browser and the
//! device.
//!
//! SSO flow (RFC: <https://datatracker.ietf.org/doc/html/rfc7636>)
//!   1. Register an OIDC client
//!      - Code: [PkceRegistration::register]
//!   2. Host a local HTTP server to handle the redirect
//!      - Code: [PkceRegistration::finish]
//!   3. Open the [PkceRegistration::url] in the browser, and approve the request.
//!   4. Exchange the code for access and refresh tokens.
//!      - This completes the future returned by [PkceRegistration::finish].
//!
//! Once access/refresh tokens are received, there is no difference between PKCE
//! and device code (as already implemented in [crate::builder_id]).

use std::time::Duration;

pub use aws_sdk_ssooidc::client::Client;
pub use aws_sdk_ssooidc::operation::create_token::CreateTokenOutput;
pub use aws_sdk_ssooidc::operation::register_client::RegisterClientOutput;
pub use aws_types::region::Region;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use percent_encoding::{
    NON_ALPHANUMERIC,
    utf8_percent_encode,
};
use rand::Rng;
use tokio::net::TcpListener;
use tracing::{
    debug,
    error,
};

use crate::auth::builder_id::*;
use crate::auth::consts::*;
use crate::auth::oauth_callback::wait_for_callback;
use crate::auth::scope::get_scopes;
use crate::auth::{
    AuthError,
    START_URL,
};
use crate::database::Database;

const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(60 * 3);

/// Starts the PKCE authorization flow, using [`START_URL`] and [`OIDC_BUILDER_ID_REGION`] as the
/// default issuer URL and region. Returns the [`PkceClient`] to use to finish the flow.
pub async fn start_pkce_authorization(
    start_url: Option<String>,
    region: Option<String>,
    database: &Database,
) -> Result<(Client, PkceRegistration), AuthError> {
    let issuer_url = start_url.as_deref().unwrap_or(START_URL);
    let region = region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);
    let client = client(region.clone());
    let registration =
        PkceRegistration::register(&client, region, issuer_url.to_string(), None, get_scopes(database)).await?;
    Ok((client, registration))
}

/// Represents a client used for registering with AWS IAM OIDC.
#[async_trait::async_trait]
pub trait PkceClient {
    async fn register_client(
        &self,
        redirect_uri: String,
        issuer_url: String,
        scopes: &[String],
    ) -> Result<RegisterClientResponse, AuthError>;

    async fn create_token(&self, args: CreateTokenArgs) -> Result<CreateTokenResponse, AuthError>;
}

#[derive(Debug, Clone)]
pub struct RegisterClientResponse {
    pub output: RegisterClientOutput,
}

impl RegisterClientResponse {
    pub fn client_id(&self) -> &str {
        self.output.client_id().unwrap_or_default()
    }

    pub fn client_secret(&self) -> &str {
        self.output.client_secret().unwrap_or_default()
    }
}

#[derive(Debug)]
pub struct CreateTokenResponse {
    pub output: CreateTokenOutput,
}

#[derive(Debug)]
pub struct CreateTokenArgs {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub code_verifier: String,
    pub code: String,
}

#[async_trait::async_trait]
impl PkceClient for Client {
    async fn register_client(
        &self,
        redirect_uri: String,
        issuer_url: String,
        scopes: &[String],
    ) -> Result<RegisterClientResponse, AuthError> {
        let mut register = self
            .register_client()
            .client_name(CLIENT_NAME)
            .client_type(CLIENT_TYPE)
            .issuer_url(issuer_url.clone())
            .redirect_uris(redirect_uri.clone())
            .grant_types("authorization_code")
            .grant_types("refresh_token");
        for scope in scopes {
            register = register.scopes(scope.clone());
        }
        let output = register.send().await?;
        Ok(RegisterClientResponse { output })
    }

    async fn create_token(&self, args: CreateTokenArgs) -> Result<CreateTokenResponse, AuthError> {
        let output = self
            .create_token()
            .client_id(args.client_id.clone())
            .client_secret(args.client_secret.clone())
            .grant_type("authorization_code")
            .redirect_uri(args.redirect_uri)
            .code_verifier(args.code_verifier)
            .code(args.code)
            .send()
            .await?;
        Ok(CreateTokenResponse { output })
    }
}

/// Represents an active PKCE registration flow. To execute the flow, you should (in order):
/// 1. Call [`PkceRegistration::register`] to register an AWS OIDC client and receive the URL to be
///    opened by the browser.
/// 2. Call [`PkceRegistration::finish`] to host a local server to handle redirects, and trade the
///    authorization code for an access token.
#[derive(Debug)]
pub struct PkceRegistration {
    /// URL to be opened by the user's browser.
    pub url: String,
    registered_client: RegisterClientResponse,
    /// Configured URI that the authorization server will redirect the client to.
    pub redirect_uri: String,
    code_verifier: String,
    /// Random value generated for every authentication attempt.
    ///
    /// <https://stackoverflow.com/questions/26132066/what-is-the-purpose-of-the-state-parameter-in-oauth-authorization-request>
    pub state: String,
    /// Listener for hosting the local HTTP server.
    listener: TcpListener,
    region: Region,
    /// Interchangeable with the "start URL" concept in the device code flow.
    issuer_url: String,
    /// Time to wait for [`Self::finish`] to complete. Default is [`DEFAULT_AUTHORIZATION_TIMEOUT`].
    timeout: Duration,
    /// The resolved OIDC scopes for this registration.
    scopes: Vec<String>,
}

impl PkceRegistration {
    pub async fn register<C: PkceClient>(
        client: &C,
        region: Region,
        issuer_url: String,
        timeout: Option<Duration>,
        scopes: Vec<String>,
    ) -> Result<Self, AuthError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let redirect_uri = format!("http://{}/oauth/callback", listener.local_addr()?);
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = rand::rng()
            .sample_iter(rand::distr::Alphanumeric)
            .take(10)
            .collect::<Vec<_>>();
        let state = String::from_utf8(state).unwrap_or("state".to_string());

        let response = client
            .register_client(redirect_uri.clone(), issuer_url.clone(), &scopes)
            .await?;

        let query = PkceQueryParams {
            client_id: response.client_id().to_string(),
            redirect_uri: redirect_uri.clone(),
            // Scopes must be space delimited.
            scopes: scopes.join(" "),
            state: state.clone(),
            code_challenge: code_challenge.clone(),
            code_challenge_method: "S256".to_string(),
        };
        let url = format!("{}/authorize?{}", oidc_url(&region), query.as_query_params());

        Ok(Self {
            url,
            registered_client: response,
            code_verifier,
            state,
            listener,
            redirect_uri,
            region,
            issuer_url,
            timeout: timeout.unwrap_or(DEFAULT_AUTHORIZATION_TIMEOUT),
            scopes,
        })
    }

    /// Hosts a local HTTP server to listen for browser redirects. If a [`Database`] is passed,
    /// then the access and refresh tokens will be saved.
    ///
    /// Only the first connection will be served.
    pub async fn finish<C: PkceClient>(self, client: &C, database: Option<&mut Database>) -> Result<(), AuthError> {
        let code = tokio::select! {
            code = wait_for_callback(self.listener, self.state, self.timeout, 1) => {
                code?
            },
            _ = tokio::time::sleep(self.timeout) => {
                return Err(AuthError::OAuthTimeout);
            }
        };

        let response = client
            .create_token(CreateTokenArgs {
                client_id: self.registered_client.client_id().to_string(),
                client_secret: self.registered_client.client_secret().to_string(),
                redirect_uri: self.redirect_uri,
                code_verifier: self.code_verifier,
                code,
            })
            .await?;

        // Tokens are redacted in the log output.
        debug!(?response, "Received create_token response");

        let token = BuilderIdToken::from_output(
            response.output,
            self.region.clone(),
            Some(self.issuer_url),
            OAuthFlow::Pkce,
            Some(self.scopes.clone()),
        );

        let device_registration = DeviceRegistration::from_output(
            self.registered_client.output,
            &self.region,
            OAuthFlow::Pkce,
            self.scopes,
        );

        if let Some(database) = database {
            if let Err(err) = device_registration.save(database).await {
                error!(?err, "Failed to store pkce registration to secret store");
            }

            if let Err(err) = token.save(database).await {
                error!(?err, "Failed to store builder id token");
            };
        }

        Ok(())
    }
}

/// Query params for the initial GET request that starts the PKCE flow. Use
/// [`PkceQueryParams::as_query_params`] to get a URL-safe string.
#[derive(Debug, Clone, serde::Serialize)]
struct PkceQueryParams {
    client_id: String,
    redirect_uri: String,
    scopes: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}

macro_rules! encode {
    ($expr:expr) => {
        utf8_percent_encode(&$expr, NON_ALPHANUMERIC)
    };
}

impl PkceQueryParams {
    fn as_query_params(&self) -> String {
        [
            "response_type=code".to_string(),
            format!("client_id={}", encode!(self.client_id)),
            format!("redirect_uri={}", encode!(self.redirect_uri)),
            format!("scopes={}", encode!(self.scopes)),
            format!("state={}", encode!(self.state)),
            format!("code_challenge={}", encode!(self.code_challenge)),
            format!("code_challenge_method={}", encode!(self.code_challenge_method)),
        ]
        .join("&")
    }
}

/// Generates a random 43-octet URL safe string according to the RFC recommendation.
///
/// Reference: https://datatracker.ietf.org/doc/html/rfc7636#section-4.1
pub fn generate_code_verifier() -> String {
    URL_SAFE.encode(rand::random::<[u8; 32]>()).replace('=', "")
}

/// Base64 URL encoded sha256 hash of the code verifier.
///
/// Reference: https://datatracker.ietf.org/doc/html/rfc7636#section-4.2
pub fn generate_code_challenge(code_verifier: &str) -> String {
    use sha2::{
        Digest,
        Sha256,
    };
    let mut hasher = Sha256::new();
    hasher.update(code_verifier);
    URL_SAFE.encode(hasher.finalize()).replace('=', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::consts::{
        DEFAULT_SCOPE_PREFIX,
        SCOPE_SUFFIXES,
    };
    use crate::auth::scope::scopes_match;

    fn test_scopes() -> Vec<String> {
        SCOPE_SUFFIXES
            .iter()
            .map(|s| format!("{}{}", DEFAULT_SCOPE_PREFIX, s))
            .collect()
    }

    #[derive(Debug, Clone)]
    struct TestPkceClient;

    #[async_trait::async_trait]
    impl PkceClient for TestPkceClient {
        async fn register_client(
            &self,
            _: String,
            _: String,
            _: &[String],
        ) -> Result<RegisterClientResponse, AuthError> {
            Ok(RegisterClientResponse {
                output: RegisterClientOutput::builder()
                    .client_id("test_client_id")
                    .client_secret("test_client_secret")
                    .build(),
            })
        }

        async fn create_token(&self, _: CreateTokenArgs) -> Result<CreateTokenResponse, AuthError> {
            Ok(CreateTokenResponse {
                output: CreateTokenOutput::builder().build(),
            })
        }
    }

    #[ignore = "not in ci"]
    #[tokio::test]
    async fn test_pkce_flow_e2e() {
        tracing_subscriber::fmt::init();

        let start_url = "https://amzn.awsapps.com/start".to_string();
        let region = Region::new("us-east-1");
        let client = client(region.clone());
        let registration = PkceRegistration::register(&client, region.clone(), start_url, None, test_scopes())
            .await
            .unwrap();
        println!("{registration:?}");
        if crate::util::open::open_url_async(&registration.url).await.is_err() {
            panic!("unable to open the URL");
        }
        println!("Waiting for authorization to complete...");

        registration.finish(&client, None).await.unwrap();
        println!("Authorization successful");
    }

    #[tokio::test]
    async fn test_pkce_flow_completes_successfully() {
        // tracing_subscriber::fmt::init();
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, region, issuer_url, None, test_scopes())
            .await
            .unwrap();

        let redirect_uri = registration.redirect_uri.clone();
        let state = registration.state.clone();
        tokio::spawn(async move {
            // Let registration.finish be called to handle the request.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            reqwest::get(format!("{}/?code={}&state={}", redirect_uri, "code", state))
                .await
                .unwrap();
        });

        registration.finish(&client, None).await.unwrap();
    }

    #[tokio::test]
    async fn test_pkce_flow_with_state_mismatch_throws_err() {
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, region, issuer_url, None, test_scopes())
            .await
            .unwrap();

        let redirect_uri = registration.redirect_uri.clone();
        tokio::spawn(async move {
            // Let registration.finish be called to handle the request.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            reqwest::get(format!("{}/?code={}&state={}", redirect_uri, "code", "not_my_state"))
                .await
                .unwrap();
        });

        assert!(matches!(
            registration.finish(&client, None).await,
            Err(AuthError::OAuthStateMismatch { actual: _, expected: _ })
        ));
    }

    #[tokio::test]
    async fn test_pkce_flow_with_authorization_redirect_error() {
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, region, issuer_url, None, test_scopes())
            .await
            .unwrap();

        let redirect_uri = registration.redirect_uri.clone();
        tokio::spawn(async move {
            // Let registration.finish be called to handle the request.
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            reqwest::get(format!(
                "{}/?error={}&error_description={}",
                redirect_uri, "error code", "something bad happened?"
            ))
            .await
            .unwrap();
        });

        assert!(matches!(
            registration.finish(&client, None).await,
            Err(AuthError::OAuthCustomError(_))
        ));
    }

    #[tokio::test]
    async fn test_pkce_flow_with_timeout() {
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(
            &client,
            region,
            issuer_url,
            Some(Duration::from_millis(100)),
            test_scopes(),
        )
        .await
        .unwrap();

        assert!(matches!(
            registration.finish(&client, None).await,
            Err(AuthError::OAuthTimeout)
        ));
    }

    #[tokio::test]
    async fn verify_gen_code_challenge() {
        let code_verifier = generate_code_verifier();
        println!("{code_verifier:?}");

        let code_challenge = generate_code_challenge(&code_verifier);
        println!("{code_challenge:?}");
        assert!(code_challenge.len() >= 43);
    }

    #[test]
    fn verify_client_scopes() {
        let scopes = test_scopes();
        assert!(scopes_match(&scopes, &scopes));
    }
}
