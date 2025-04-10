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

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub use aws_sdk_ssooidc::client::Client;
pub use aws_sdk_ssooidc::operation::create_token::CreateTokenOutput;
pub use aws_sdk_ssooidc::operation::register_client::RegisterClientOutput;
pub use aws_types::region::Region;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
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

use crate::builder_id::*;
use crate::consts::*;
use crate::secret_store::SecretStore;
use crate::{
    Error,
    Result,
    START_URL,
};

const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(60 * 3);

/// Starts the PKCE authorization flow, using [`START_URL`] and [`OIDC_BUILDER_ID_REGION`] as the
/// default issuer URL and region. Returns the [`PkceClient`] to use to finish the flow.
pub async fn start_pkce_authorization(
    start_url: Option<String>,
    region: Option<String>,
) -> Result<(Client, PkceRegistration)> {
    let issuer_url = start_url.as_deref().unwrap_or(START_URL);
    let region = region.clone().map_or(OIDC_BUILDER_ID_REGION, Region::new);
    let client = client(region.clone());
    let registration = PkceRegistration::register(&client, region, issuer_url.to_string(), None).await?;
    Ok((client, registration))
}

/// Represents a client used for registering with AWS IAM OIDC.
#[async_trait::async_trait]
pub trait PkceClient {
    /// The scopes that the client will request
    fn scopes() -> Vec<String>;

    async fn register_client(&self, redirect_uri: String, issuer_url: String) -> Result<RegisterClientResponse>;

    async fn create_token(&self, args: CreateTokenArgs) -> Result<CreateTokenResponse>;
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
    fn scopes() -> Vec<String> {
        SCOPES.iter().map(|s| (*s).to_owned()).collect()
    }

    async fn register_client(&self, redirect_uri: String, issuer_url: String) -> Result<RegisterClientResponse> {
        let mut register = self
            .register_client()
            .client_name(CLIENT_NAME)
            .client_type(CLIENT_TYPE)
            .issuer_url(issuer_url.clone())
            .redirect_uris(redirect_uri.clone())
            .grant_types("authorization_code")
            .grant_types("refresh_token");
        for scope in Self::scopes() {
            register = register.scopes(scope);
        }
        let output = register.send().await?;
        Ok(RegisterClientResponse { output })
    }

    async fn create_token(&self, args: CreateTokenArgs) -> Result<CreateTokenResponse> {
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
}

impl PkceRegistration {
    pub async fn register(
        client: &impl PkceClient,
        region: Region,
        issuer_url: String,
        timeout: Option<Duration>,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let redirect_uri = format!("http://{}/oauth/callback", listener.local_addr()?);
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = rand::rng()
            .sample_iter(rand::distr::Alphanumeric)
            .take(10)
            .collect::<Vec<_>>();
        let state = String::from_utf8(state).unwrap_or("state".to_string());

        let response = client.register_client(redirect_uri.clone(), issuer_url.clone()).await?;

        let query = PkceQueryParams {
            client_id: response.client_id().to_string(),
            redirect_uri: redirect_uri.clone(),
            // Scopes must be space delimited.
            scopes: SCOPES.join(" "),
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
        })
    }

    /// Hosts a local HTTP server to listen for browser redirects. If a [`SecretStore`] is passed,
    /// then the access and refresh tokens will be saved.
    ///
    /// Only the first connection will be served.
    pub async fn finish<C: PkceClient>(self, client: &C, secret_store: Option<&SecretStore>) -> Result<()> {
        let code = tokio::select! {
            code = Self::recv_code(self.listener, self.state) => {
                code?
            },
            _ = tokio::time::sleep(self.timeout) => {
                return Err(Error::OAuthTimeout);
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
            OAuthFlow::PKCE,
            Some(C::scopes()),
        );

        let device_registration = DeviceRegistration::from_output(
            self.registered_client.output,
            &self.region,
            OAuthFlow::PKCE,
            C::scopes(),
        );

        let Some(secret_store) = secret_store else {
            return Ok(());
        };

        if let Err(err) = device_registration.save(secret_store).await {
            error!(?err, "Failed to store pkce registration to secret store");
        }

        if let Err(err) = token.save(secret_store).await {
            error!(?err, "Failed to store builder id token");
        };

        Ok(())
    }

    async fn recv_code(listener: TcpListener, expected_state: String) -> Result<String> {
        let (code_tx, mut code_rx) = tokio::sync::mpsc::channel::<Result<(String, String)>>(1);
        let (stream, _) = listener.accept().await?;
        let stream = TokioIo::new(stream); // Wrapper to implement Hyper IO traits for Tokio types.
        let host = listener.local_addr()?.to_string();
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(stream, PkceHttpService {
                    code_tx: std::sync::Arc::new(code_tx),
                    host,
                })
                .await
            {
                error!(?err, "Error occurred serving the connection");
            }
        });
        match code_rx.recv().await {
            Some(Ok((code, state))) => {
                debug!(code = "<redacted>", state, "Received code and state");
                if state != expected_state {
                    return Err(Error::OAuthStateMismatch {
                        actual: state,
                        expected: expected_state,
                    });
                }
                // Give time for the user to be redirected to index.html.
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(code)
            },
            Some(Err(err)) => {
                // Give time for the user to be redirected to index.html.
                tokio::time::sleep(Duration::from_millis(200)).await;
                Err(err)
            },
            None => Err(Error::OAuthMissingCode),
        }
    }
}

type CodeSender = std::sync::Arc<tokio::sync::mpsc::Sender<Result<(String, String)>>>;
type ServiceError = Error;
type ServiceResponse = Response<Full<Bytes>>;
type ServiceFuture = Pin<Box<dyn Future<Output = Result<ServiceResponse, ServiceError>> + Send>>;

#[derive(Debug, Clone)]
struct PkceHttpService {
    /// [`tokio::sync::mpsc::Sender`] for a (code, state) pair.
    code_tx: CodeSender,

    /// The host being served - ie, the hostname and port.
    /// Used for responding with redirects.
    host: String,
}

impl PkceHttpService {
    /// Handles the browser redirect to `"http://{host}/oauth/callback"` which contains either the
    /// code and state query params, or an error query param. Redirects to "/index.html".
    ///
    /// The [`Request`] doesn't actually contain the host, hence the `host` argument.
    async fn handle_oauth_callback(
        code_tx: CodeSender,
        host: String,
        req: Request<Incoming>,
    ) -> Result<ServiceResponse> {
        let query_params = req
            .uri()
            .query()
            .map(|query| {
                query
                    .split('&')
                    .filter_map(|kv| kv.split_once('='))
                    .collect::<std::collections::HashMap<_, _>>()
            })
            .ok_or(Error::OAuthCustomError("query parameters are missing".into()))?;

        // Error handling: if something goes wrong at the authorization endpoint, the
        // client will be redirected to the redirect url with "error" and
        // "error_description" query parameters.
        if let Some(error) = query_params.get("error") {
            let error_description = query_params.get("error_description").unwrap_or(&"");
            let _ = code_tx
                .send(Err(Error::OAuthCustomError(format!(
                    "error occurred during authorization: {:?}, {:?}",
                    error, error_description
                ))))
                .await;
            return Self::redirect_to_index(&host, &format!("?error={}", error));
        } else {
            let code = query_params.get("code");
            let state = query_params.get("state");
            if let (Some(code), Some(state)) = (code, state) {
                let _ = code_tx.send(Ok(((*code).to_string(), (*state).to_string()))).await;
            } else {
                let _ = code_tx
                    .send(Err(Error::OAuthCustomError(
                        "missing code and/or state in the query parameters".into(),
                    )))
                    .await;
                return Self::redirect_to_index(&host, "?error=missing%20required%20query%20parameters");
            }
        }

        Self::redirect_to_index(&host, "")
    }

    fn redirect_to_index(host: &str, query_params: &str) -> Result<ServiceResponse> {
        Ok(Response::builder()
            .status(302)
            .header("Location", format!("http://{}/index.html{}", host, query_params))
            .body("".into())
            .expect("is valid builder, should not panic"))
    }
}

impl Service<Request<Incoming>> for PkceHttpService {
    type Error = ServiceError;
    type Future = ServiceFuture;
    type Response = ServiceResponse;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let code_tx: CodeSender = std::sync::Arc::clone(&self.code_tx);
        let host = self.host.clone();
        Box::pin(async move {
            debug!(?req, "Handling connection");
            match req.uri().path() {
                "/oauth/callback" | "/oauth/callback/" => Self::handle_oauth_callback(code_tx, host, req).await,
                "/index.html" => Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html")
                    .header("Connection", "close")
                    .body(include_str!("./index.html").into())
                    .expect("valid builder will not panic")),
                _ => Ok(Response::builder()
                    .status(404)
                    .body("".into())
                    .expect("valid builder will not panic")),
            }
        })
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
fn generate_code_verifier() -> String {
    URL_SAFE.encode(rand::random::<[u8; 32]>()).replace('=', "")
}

/// Base64 URL encoded sha256 hash of the code verifier.
///
/// Reference: https://datatracker.ietf.org/doc/html/rfc7636#section-4.2
fn generate_code_challenge(code_verifier: &str) -> String {
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
    use crate::scope::is_scopes;

    #[derive(Debug, Clone)]
    struct TestPkceClient;

    #[async_trait::async_trait]
    impl PkceClient for TestPkceClient {
        fn scopes() -> Vec<String> {
            vec!["scope:1".to_string(), "scope:2".to_string()]
        }

        async fn register_client(&self, _: String, _: String) -> Result<RegisterClientResponse> {
            Ok(RegisterClientResponse {
                output: RegisterClientOutput::builder()
                    .client_id("test_client_id")
                    .client_secret("test_client_secret")
                    .build(),
            })
        }

        async fn create_token(&self, _: CreateTokenArgs) -> Result<CreateTokenResponse> {
            Ok(CreateTokenResponse {
                output: CreateTokenOutput::builder().build(),
            })
        }
    }

    #[ignore = "not in ci"]
    #[tokio::test]
    async fn test_pkce_flow_e2e() {
        tracing_subscriber::fmt::init();
        let start_url = AMZN_START_URL.into();
        let region = Region::new("us-east-1");
        let client = client(region.clone());
        let registration = PkceRegistration::register(&client, region.clone(), start_url, None)
            .await
            .unwrap();
        println!("{:?}", registration);
        if fig_util::open_url_async(&registration.url).await.is_err() {
            panic!("unable to open the URL");
        }
        println!("Waiting for authorization to complete...");
        let secret_store = SecretStore::new().await.unwrap();
        registration.finish(&client, Some(&secret_store)).await.unwrap();
        println!("Authorization successful");
    }

    #[tokio::test]
    async fn test_pkce_flow_completes_successfully() {
        // tracing_subscriber::fmt::init();
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, region, issuer_url, None)
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
        let registration = PkceRegistration::register(&client, region, issuer_url, None)
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
            Err(Error::OAuthStateMismatch { actual: _, expected: _ })
        ));
    }

    #[tokio::test]
    async fn test_pkce_flow_with_authorization_redirect_error() {
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, region, issuer_url, None)
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
            Err(Error::OAuthCustomError(_))
        ));
    }

    #[tokio::test]
    async fn test_pkce_flow_with_timeout() {
        let region = Region::new("us-east-1");
        let issuer_url = START_URL.into();
        let client = TestPkceClient {};
        let registration = PkceRegistration::register(&client, region, issuer_url, Some(Duration::from_millis(100)))
            .await
            .unwrap();

        assert!(matches!(
            registration.finish(&client, None).await,
            Err(Error::OAuthTimeout)
        ));
    }

    #[tokio::test]
    async fn verify_gen_code_challenge() {
        let code_verifier = generate_code_verifier();
        println!("{:?}", code_verifier);

        let code_challenge = generate_code_challenge(&code_verifier);
        println!("{:?}", code_challenge);
        assert!(code_challenge.len() >= 43);
    }

    #[test]
    fn verify_client_scopes() {
        assert!(is_scopes(&Client::scopes()));
    }
}
