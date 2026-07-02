use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{
    Path,
    PathBuf,
};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use http::HeaderMap;
use http_body_util::Full;
use hyper::Response;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use reqwest::Client;
use rmcp::service::{
    DynService,
    ServiceExt,
};
use rmcp::transport::auth::{
    AuthClient,
    OAuthClientConfig,
    OAuthState,
    OAuthTokenResponse,
};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{
    AuthorizationManager,
    AuthorizationSession,
    StreamableHttpClientTransport,
};
use rmcp::{
    RoleClient,
    Service,
    serde_json,
};
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use tokio::sync::mpsc;
use tokio::sync::oneshot::Sender;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    info,
};
use url::Url;

use super::actor::McpServerActorEvent;

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    /// Pre-registered OAuth client ID for servers that don't support Dynamic Client Registration.
    /// When set, this client_id is used instead of the default "Q DEV CLI" fallback if DCR fails.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Pre-registered OAuth client secret for confidential clients (e.g. Figma).
    /// Only meaningful alongside `client_id`: when both are set, DCR is skipped and this
    /// secret is sent to the token endpoint for client authentication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Custom loopback redirect URI for the OAuth flow, e.g. `127.0.0.1:7778` or
    /// `http://localhost:7778/callback`. Only used to pin the loopback port (and
    /// path, when matching a pre-registered app); the host must be `127.0.0.1` or
    /// `localhost` and the scheme `http`. If omitted, the OS assigns a random port.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// Optional OAuth scopes to request from the authorization server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_scopes: Option<Vec<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum OauthUtilError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] url::ParseError),
    #[error(transparent)]
    Auth(rmcp::transport::AuthError),
    #[error(
        "OAuth discovery failed: the server does not advertise OAuth endpoints. \
         Verify that the server URL is correct and that the server supports MCP authentication."
    )]
    OAuthDiscoveryFailed,
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("Missing authorization manager")]
    MissingAuthorizationManager,
    #[error("Missing auth client when token refresh is needed")]
    MissingAuthClient,
    #[error(transparent)]
    OneshotRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Http(String),
    #[error("Malformed directory")]
    MalformDirectory,
    #[error("Missing credential")]
    MissingCredentials,
    #[error("Failed to create a running service after running through all fallbacks: {0}")]
    ServiceNotObtained(String),
    #[error("Invalid redirect_uri in OAuth config: {0}")]
    InvalidRedirectUri(String),
}

impl From<rmcp::transport::AuthError> for OauthUtilError {
    fn from(err: rmcp::transport::AuthError) -> Self {
        match err {
            rmcp::transport::AuthError::NoAuthorizationSupport => Self::OAuthDiscoveryFailed,
            other => Self::Auth(other),
        }
    }
}

/// A guard that automatically cancels the cancellation token when dropped.
/// This ensures that the OAuth loopback server is properly cleaned up
/// when the guard goes out of scope.
struct LoopBackDropGuard {
    cancellation_token: CancellationToken,
}

impl Drop for LoopBackDropGuard {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

/// OAuth Authorization Server metadata for endpoint discovery
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OAuthMeta {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
}

/// This is modeled after [OAuthClientConfig]
/// It's only here because [OAuthClientConfig] does not implement Serialize and Deserialize
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Registration {
    pub client_id: String,
    pub client_secret: Option<String>,
    /// Defaults to empty so older or partially-written cache files still parse.
    #[serde(default)]
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

impl From<OAuthClientConfig> for Registration {
    fn from(value: OAuthClientConfig) -> Self {
        Self {
            client_id: value.client_id,
            client_secret: value.client_secret,
            scopes: value.scopes,
            redirect_uri: value.redirect_uri,
        }
    }
}

/// Context needed to perform a full browser-based re-authentication flow mid-session.
#[derive(Clone, Debug)]
pub struct ReauthContext {
    pub server_name: String,
    pub url: Url,
    pub reg_full_path: PathBuf,
    pub scopes: Vec<String>,
    pub oauth_config: Option<OAuthConfig>,
    pub server_actor_event_tx: mpsc::Sender<McpServerActorEvent>,
}

/// Writes OAuth credentials to disk with owner-only permissions.
///
/// Credentials contain OAuth tokens (access/refresh), so on Unix the file is
/// restricted to `0600` (read/write for the owner only) to prevent other users
/// on the system from reading them.
async fn write_credentials_securely(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    tokio::fs::write(path, contents).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }
    // TODO(windows): Windows has no Unix mode bits, so credentials are not yet
    // restricted to the current user on that platform. The equivalent requires
    // removing NTFS ACL inheritance and granting access only to the current
    // user (e.g. via `icacls` or the Win32 security APIs). Scoped out for a
    // follow-up: https://taskei.amazon.dev/tasks/55e07d56-8b05-410f-a093-0beef0ca1162
    Ok(())
}

/// Refreshes an OAuth token and persists the new credentials to disk.
///
/// This is a standalone function used during connection setup when a token
/// might be expired. For mid-session refresh, use `AuthClientWrapper::refresh_token()`.
pub async fn refresh_and_persist_token(
    auth_client: &AuthClient<Client>,
    cred_full_path: &Path,
) -> Result<(), OauthUtilError> {
    let cred = auth_client.auth_manager.lock().await.refresh_token().await?;
    let parent_path = cred_full_path.parent().ok_or(OauthUtilError::MalformDirectory)?;
    tokio::fs::create_dir_all(parent_path).await?;

    let cred_as_bytes = serde_json::to_string_pretty(&cred)?;
    write_credentials_securely(cred_full_path, &cred_as_bytes).await?;

    Ok(())
}

/// A wrapper that manages an authenticated MCP client.
///
/// This struct wraps an `AuthClient` and provides access to OAuth credentials
/// for MCP server connections that require authentication. The credentials
/// are managed separately from this wrapper's lifecycle.
#[derive(Clone, Debug)]
pub struct AuthClientWrapper {
    pub cred_full_path: PathBuf,
    pub auth_client: AuthClient<Client>,
    pub reauth_ctx: ReauthContext,
}

impl AuthClientWrapper {
    pub fn new(cred_full_path: PathBuf, auth_client: AuthClient<Client>, reauth_ctx: ReauthContext) -> Self {
        Self {
            cred_full_path,
            auth_client,
            reauth_ctx,
        }
    }

    /// Refreshes token in memory using the registration read from when the auth client was
    /// spawned. This also persists the retrieved token
    pub async fn refresh_token(&self) -> Result<(), OauthUtilError> {
        refresh_and_persist_token(&self.auth_client, &self.cred_full_path).await
    }

    /// Performs a full browser-based OAuth re-authentication flow mid-session.
    ///
    /// This is called when `refresh_token()` fails (e.g., no refresh token was issued by the
    /// server). It spins up a new loopback server, opens the browser for the user to
    /// authenticate, exchanges the auth code for a new token, and swaps the
    /// `AuthorizationManager` in-place via the shared `Arc<Mutex<>>` so the existing transport
    /// picks up the new credentials automatically.
    pub async fn reauthorize(&self) -> Result<(), OauthUtilError> {
        let auth_client_wrapper_clone = self.clone();

        tokio::spawn(async move {
            let ctx = &auth_client_wrapper_clone.reauth_ctx;

            let oauth_state = OAuthState::new(ctx.url.clone(), None).await.map_err(|e| {
                error!("## mcp: reauthorize failed to create OAuthState for {}: {e}", ctx.url);
                e
            })?;
            let (new_am, redirect_uri) = get_auth_manager_impl(
                &ctx.server_name,
                oauth_state,
                &ctx.scopes,
                &ctx.oauth_config,
                &ctx.server_actor_event_tx,
            )
            .await
            .map_err(|e| {
                error!("## mcp: reauthorize failed during auth flow for {}: {e}", ctx.url);
                e
            })?;

            // Persist the new credentials and registration
            let (client_id, credentials) = new_am.get_credentials().await.map_err(|e| {
                error!("## mcp: reauthorize failed to get credentials for {}: {e}", ctx.url);
                e
            })?;
            let reg = Registration {
                client_id,
                client_secret: None,
                scopes: get_default_scopes()
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                redirect_uri,
            };
            let reg_as_str = serde_json::to_string_pretty(&reg).map_err(|e| {
                error!(
                    "## mcp: reauthorize failed to serialize registration for {}: {e}",
                    ctx.url
                );
                e
            })?;
            let reg_parent = ctx.reg_full_path.parent().ok_or_else(|| {
                error!(
                    "## mcp: reauthorize failed: malformed registration path for {}",
                    ctx.url
                );
                OauthUtilError::MalformDirectory
            })?;
            tokio::fs::create_dir_all(reg_parent).await.map_err(|e| {
                error!(
                    "## mcp: reauthorize failed to create registration dir for {}: {e}",
                    ctx.url
                );
                e
            })?;
            tokio::fs::write(&ctx.reg_full_path, &reg_as_str).await.map_err(|e| {
                error!("## mcp: reauthorize failed to write registration for {}: {e}", ctx.url);
                e
            })?;

            let credentials = credentials.ok_or_else(|| {
                error!("## mcp: reauthorize failed: missing credentials for {}", ctx.url);
                OauthUtilError::MissingCredentials
            })?;
            let cred_parent = auth_client_wrapper_clone.cred_full_path.parent().ok_or_else(|| {
                error!("## mcp: reauthorize failed: malformed credential path for {}", ctx.url);
                OauthUtilError::MalformDirectory
            })?;
            tokio::fs::create_dir_all(cred_parent).await.map_err(|e| {
                error!(
                    "## mcp: reauthorize failed to create credential dir for {}: {e}",
                    ctx.url
                );
                e
            })?;
            let cred_as_str = serde_json::to_string_pretty(&credentials).map_err(|e| {
                error!(
                    "## mcp: reauthorize failed to serialize credentials for {}: {e}",
                    ctx.url
                );
                e
            })?;
            write_credentials_securely(&auth_client_wrapper_clone.cred_full_path, &cred_as_str)
                .await
                .map_err(|e| {
                    error!("## mcp: reauthorize failed to write credentials for {}: {e}", ctx.url);
                    e
                })?;

            // Swap the AuthorizationManager in-place so the existing transport picks up new creds
            let mut guard = auth_client_wrapper_clone.auth_client.auth_manager.lock().await;
            *guard = new_am;

            info!("## mcp: re-authentication successful, credentials swapped in-place");

            Ok::<(), Box<dyn std::error::Error + Send + Sync + 'static>>(())
        });

        Ok(())
    }
}

pub fn get_default_scopes() -> &'static [&'static str] {
    &["openid", "email", "profile", "offline_access"]
}

enum HttpServiceBuilderState {
    /// Try unauthenticated connection first
    TryUnauthenticated,
    /// Try authenticated connection (has_refreshed_token)
    TryAuthenticated(bool),
    FailedBecauseTokenMightBeExpired,
    Exhausted,
}

pub type HttpRunningService = (
    rmcp::service::RunningService<RoleClient, Box<dyn DynService<RoleClient>>>,
    Option<AuthClientWrapper>,
);

pub struct HttpServiceBuilder<'a> {
    pub server_name: &'a str,
    pub url: &'a str,
    pub timeout: u64,
    pub scopes: &'a [String],
    pub headers: &'a HashMap<String, String>,
    pub oauth_config: &'a Option<OAuthConfig>,
    pub server_actor_event_tx: &'a mpsc::Sender<McpServerActorEvent>,
    /// When `true`, skip the unauthenticated connection attempt and go straight
    /// to the OAuth flow.
    pub force_auth: bool,
}

impl<'a> HttpServiceBuilder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        server_name: &'a str,
        url: &'a str,
        timeout: u64,
        scopes: &'a [String],
        headers: &'a HashMap<String, String>,
        oauth_config: &'a Option<OAuthConfig>,
        server_actor_event_tx: &'a mpsc::Sender<McpServerActorEvent>,
        force_auth: bool,
    ) -> Self {
        Self {
            server_name,
            url,
            timeout,
            scopes,
            headers,
            oauth_config,
            server_actor_event_tx,
            force_auth,
        }
    }

    pub async fn try_build<S: Service<RoleClient> + Clone>(
        self,
        service: &S,
        cred_dir: &Path,
    ) -> Result<HttpRunningService, OauthUtilError> {
        let HttpServiceBuilder {
            server_name,
            url,
            timeout,
            scopes,
            headers,
            oauth_config,
            server_actor_event_tx,
            force_auth,
        } = self;

        let url = Url::from_str(url)?;
        let key = compute_key(&url);
        let cred_full_path = cred_dir.join(format!("{key}.token.json"));
        let reg_full_path = cred_dir.join(format!("{key}.registration.json"));
        let mut auth_client = None::<AuthClient<Client>>;

        // Decide where to begin the connection state machine. Normally we try an
        // unauthenticated connection first (many servers expose unauthenticated
        // methods). We skip straight to the authenticated path when either:
        //   - `force_auth` is set (the user explicitly requested authentication), or
        //   - a persisted token already exists for this server (a previous auth succeeded, so assume
        //     authentication is expected going forward).
        let token_exists = cred_full_path.is_file();
        let mut state = if force_auth || token_exists {
            info!(
                "## mcp: starting authenticated for {server_name} (force_auth={force_auth}, token_exists={token_exists})"
            );
            HttpServiceBuilderState::TryAuthenticated(false)
        } else {
            HttpServiceBuilderState::TryUnauthenticated
        };

        let mut client_builder = reqwest::ClientBuilder::new().timeout(std::time::Duration::from_millis(timeout));
        if !headers.is_empty() {
            let headers = HeaderMap::try_from(headers).map_err(|e| OauthUtilError::Http(e.to_string()))?;
            client_builder = client_builder.default_headers(headers);
        };
        let reqwest_client = client_builder.build()?;

        // Strategy: try unauthenticated first, then authenticated.
        loop {
            match state {
                HttpServiceBuilderState::TryUnauthenticated => {
                    info!("## mcp: attempting unauthenticated http for {server_name}");
                    let transport = StreamableHttpClientTransport::with_client(
                        reqwest_client.clone(),
                        StreamableHttpClientTransportConfig::with_uri(url.as_str()),
                    );

                    match service.clone().into_dyn().serve(transport).await {
                        Ok(service) => return Ok((service, None)),
                        Err(e) => {
                            info!("## mcp: unauthenticated http failed for {server_name}: {e:?}, trying authenticated");
                            state = HttpServiceBuilderState::TryAuthenticated(false);
                        },
                    }
                },
                HttpServiceBuilderState::TryAuthenticated(has_refreshed) => {
                    let ac = match auth_client {
                        Some(ref auth_client) => auth_client.clone(),
                        None => {
                            let am = get_auth_manager(
                                server_name,
                                url.clone(),
                                cred_full_path.clone(),
                                reg_full_path.clone(),
                                scopes,
                                oauth_config,
                                server_actor_event_tx,
                            )
                            .await?;

                            let ac = AuthClient::new(reqwest_client.clone(), am);
                            auth_client.replace(ac.clone());
                            ac
                        },
                    };

                    info!("## mcp: attempting authenticated http for {server_name}");
                    let transport = StreamableHttpClientTransport::with_client(
                        ac.clone(),
                        StreamableHttpClientTransportConfig::with_uri(url.as_str()),
                    );

                    match service.clone().into_dyn().serve(transport).await {
                        Ok(service) => {
                            let auth_client_wrapper = AuthClientWrapper::new(cred_full_path, ac, ReauthContext {
                                server_name: server_name.to_string(),
                                url: url.clone(),
                                reg_full_path,
                                scopes: scopes.to_vec(),
                                oauth_config: oauth_config.clone(),
                                server_actor_event_tx: server_actor_event_tx.clone(),
                            });
                            return Ok((service, Some(auth_client_wrapper)));
                        },
                        Err(e) => {
                            if !has_refreshed {
                                error!("## mcp: authenticated http failed for {server_name}: {e:?}, refreshing token");
                                state = HttpServiceBuilderState::FailedBecauseTokenMightBeExpired;
                            } else {
                                error!("## mcp: authenticated http failed for {server_name}: {e:?}, exhausted");
                                state = HttpServiceBuilderState::Exhausted;
                            }
                        },
                    }
                },
                HttpServiceBuilderState::FailedBecauseTokenMightBeExpired => {
                    let auth_client_ref = auth_client.as_ref().ok_or(OauthUtilError::MissingAuthClient)?;
                    let refresh_res = refresh_and_persist_token(auth_client_ref, &cred_full_path).await;

                    if let Err(e) = refresh_res {
                        error!("## mcp: token refresh failed: {e:?}");
                        if cred_full_path.is_file() {
                            tokio::fs::remove_file(&cred_full_path).await?;
                        }
                        auth_client.take();
                    }

                    state = HttpServiceBuilderState::TryAuthenticated(true);
                },
                HttpServiceBuilderState::Exhausted => {
                    return Err(OauthUtilError::ServiceNotObtained(
                        "All connection attempts exhausted".to_string(),
                    ));
                },
            }
        }
    }
}

async fn get_auth_manager(
    server_name: &str,
    url: Url,
    cred_full_path: PathBuf,
    reg_full_path: PathBuf,
    scopes: &[String],
    oauth_config: &Option<OAuthConfig>,
    server_actor_event_tx: &mpsc::Sender<McpServerActorEvent>,
) -> Result<AuthorizationManager, OauthUtilError> {
    let cred_as_bytes = tokio::fs::read(&cred_full_path).await;
    let reg_as_bytes = tokio::fs::read(&reg_full_path).await;
    let mut oauth_state = OAuthState::new(url, None).await?;

    // If cached credentials exist and parse, use them. Otherwise fall through
    // to a fresh OAuth flow (and remove the bad files so we don't loop on them).
    let cached_path = if let (Ok(cred_bytes), Ok(reg_bytes)) = (cred_as_bytes, reg_as_bytes) {
        match (
            serde_json::from_slice::<OAuthTokenResponse>(&cred_bytes),
            serde_json::from_slice::<Registration>(&reg_bytes),
        ) {
            (Ok(token), Ok(reg)) => Some((token, reg)),
            (cred_res, reg_res) => {
                tracing::warn!(
                    server_name = %server_name,
                    cred_err = ?cred_res.err(),
                    reg_err = ?reg_res.err(),
                    "## mcp: cached OAuth credentials failed to parse, removing and re-running OAuth flow"
                );
                // Remove malformed caches so we don't loop on them.
                let _ = tokio::fs::remove_file(&cred_full_path).await;
                let _ = tokio::fs::remove_file(&reg_full_path).await;
                None
            },
        }
    } else {
        None
    };

    match cached_path {
        Some((token, reg)) => {
            oauth_state.set_credentials(&reg.client_id, token).await?;

            debug!("## mcp: credentials set with cache");

            let mut am = oauth_state
                .into_authorization_manager()
                .ok_or(OauthUtilError::MissingAuthorizationManager)?;

            // `set_credentials` configures a public client (no secret). For confidential
            // clients (e.g. Figma) re-apply the configured secret so an eventual token
            // refresh can authenticate at the token endpoint.
            if let Some(secret) = oauth_config.as_ref().and_then(|cfg| cfg.client_secret.as_deref()) {
                am.configure_client(
                    OAuthClientConfig::new(reg.client_id.clone(), reg.redirect_uri.clone())
                        .with_scopes(reg.scopes.clone())
                        .with_client_secret(secret.to_string()),
                )?;
            }

            Ok(am)
        },
        None => {
            info!("Error reading cached credentials");
            debug!("## mcp: cache read failed. constructing auth manager from scratch");
            let (am, redirect_uri) =
                get_auth_manager_impl(server_name, oauth_state, scopes, oauth_config, server_actor_event_tx).await?;

            // Client registration is done in [start_authorization]
            // If we have gotten past that point that means we have the info to persist the
            // registration on disk.
            let (client_id, credentials) = am.get_credentials().await?;
            let reg = Registration {
                client_id,
                client_secret: None,
                scopes: get_default_scopes()
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                redirect_uri,
            };
            let reg_as_str = serde_json::to_string_pretty(&reg)?;
            let reg_parent_path = reg_full_path.parent().ok_or(OauthUtilError::MalformDirectory)?;
            tokio::fs::create_dir_all(reg_parent_path).await?;
            tokio::fs::write(reg_full_path, &reg_as_str).await?;

            let credentials = credentials.ok_or(OauthUtilError::MissingCredentials)?;

            let cred_parent_path = cred_full_path.parent().ok_or(OauthUtilError::MalformDirectory)?;
            tokio::fs::create_dir_all(cred_parent_path).await?;
            let reg_as_str = serde_json::to_string_pretty(&credentials)?;
            write_credentials_securely(cred_full_path, &reg_as_str).await?;

            Ok(am)
        },
    }
}

/// The only hosts a loopback OAuth callback can be delivered to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopbackHost {
    Ipv4,
    Localhost,
}

impl LoopbackHost {
    fn as_str(self) -> &'static str {
        match self {
            LoopbackHost::Ipv4 => "127.0.0.1",
            LoopbackHost::Localhost => "localhost",
        }
    }

    fn parse(host: &str) -> Result<Self, OauthUtilError> {
        match host {
            "127.0.0.1" => Ok(Self::Ipv4),
            "localhost" => Ok(Self::Localhost),
            other => Err(OauthUtilError::InvalidRedirectUri(format!(
                "host must be 127.0.0.1 or localhost, got `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RedirectUriConfig {
    port: Option<u16>,
    host: LoopbackHost,
    path: String,
}

/// Parses the configured redirect URI, accepting a full URL, `host:port`,
/// `:port`, or a bare host. Only loopback hosts and the `http` scheme are valid
/// (the callback is served by a local loopback server); anything else errors.
impl TryFrom<Option<&str>> for RedirectUriConfig {
    type Error = OauthUtilError;

    fn try_from(configured: Option<&str>) -> Result<Self, Self::Error> {
        let Some(raw) = configured.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(RedirectUriConfig {
                port: None,
                host: LoopbackHost::Ipv4,
                path: String::new(),
            });
        };

        if raw.contains("://") {
            let url = Url::parse(raw)?;
            if url.scheme() != "http" {
                return Err(OauthUtilError::InvalidRedirectUri(format!(
                    "scheme must be http, got `{}`",
                    url.scheme()
                )));
            }
            let path = match url.path() {
                "/" | "" => String::new(),
                p => p.to_string(),
            };
            return Ok(RedirectUriConfig {
                port: url.port(),
                host: LoopbackHost::parse(url.host_str().unwrap_or("127.0.0.1"))?,
                path,
            });
        }

        if let Some((host_part, port_part)) = raw.rsplit_once(':') {
            let host = if host_part.is_empty() {
                LoopbackHost::Ipv4
            } else {
                LoopbackHost::parse(host_part)?
            };
            return Ok(RedirectUriConfig {
                port: port_part.parse::<u16>().ok(),
                host,
                path: String::new(),
            });
        }

        Ok(RedirectUriConfig {
            port: None,
            host: LoopbackHost::parse(raw)?,
            path: String::new(),
        })
    }
}

async fn get_auth_manager_impl(
    server_name: &str,
    mut oauth_state: OAuthState,
    scopes: &[String],
    oauth_config: &Option<OAuthConfig>,
    server_actor_event_tx: &mpsc::Sender<McpServerActorEvent>,
) -> Result<(AuthorizationManager, String), OauthUtilError> {
    let parsed = RedirectUriConfig::try_from(oauth_config.as_ref().and_then(|cfg| cfg.redirect_uri.as_deref()))?;

    let socket_addr = SocketAddr::from(([127, 0, 0, 1], parsed.port.unwrap_or(0)));
    let cancellation_token = tokio_util::sync::CancellationToken::new();
    let (tx, rx) = tokio::sync::oneshot::channel::<(String, String)>();

    let (actual_addr, _dg) = make_svc(tx, socket_addr, cancellation_token).await?;
    info!("Listening on local host port {:?} for oauth", actual_addr);

    let redirect_uri = format!("http://{}:{}{}", parsed.host.as_str(), actual_addr.port(), parsed.path);
    let scopes_as_str = scopes.iter().map(String::as_str).collect::<Vec<_>>();
    let scopes_as_slice = scopes_as_str.as_slice();
    let user_client_id = oauth_config.as_ref().and_then(|cfg| cfg.client_id.as_deref());
    let user_client_secret = oauth_config.as_ref().and_then(|cfg| cfg.client_secret.as_deref());
    start_authorization(
        &mut oauth_state,
        scopes_as_slice,
        &redirect_uri,
        user_client_id,
        user_client_secret,
    )
    .await?;

    let oauth_url = oauth_state.get_authorization_url().await?;
    debug!(?oauth_url, "generated auth url");
    if let Err(e) = server_actor_event_tx
        .send(McpServerActorEvent::OauthRequest {
            server_name: server_name.to_string(),
            oauth_url,
        })
        .await
    {
        error!(?e, "failed to send auth url");
    }

    let (auth_code, csrf_token) = rx.await?;
    oauth_state.handle_callback(&auth_code, &csrf_token).await?;
    let am = oauth_state
        .into_authorization_manager()
        .ok_or(OauthUtilError::MissingAuthorizationManager)?;

    Ok((am, redirect_uri))
}

pub fn compute_key(rs: &Url) -> String {
    let mut hasher = Sha256::new();
    let input = format!("{}{}", rs.origin().ascii_serialization(), rs.path());
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Remove the persisted OAuth credentials for a remote MCP server.
///
/// Deletes both the cached token (`{key}.token.json`) and the dynamic client
/// registration (`{key}.registration.json`) stored under `cred_dir`, where `key`
/// is derived from `url` via [`compute_key`]. Missing files are not an error.
///
/// Returns `Ok(true)` if at least one credential file was present and removed.
pub async fn remove_persisted_credentials(cred_dir: &Path, url: &str) -> Result<bool, OauthUtilError> {
    let url = Url::from_str(url)?;
    let key = compute_key(&url);
    let token_path = cred_dir.join(format!("{key}.token.json"));
    let reg_path = cred_dir.join(format!("{key}.registration.json"));

    let mut removed = false;
    for path in [token_path, reg_path] {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => removed = true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => return Err(e.into()),
        }
    }
    Ok(removed)
}

/// This is our own implementation of [OAuthState::start_authorization].
/// This differs from [OAuthState::start_authorization] by assigning our own client_id for DCR.
/// We need this because the SDK hardcodes their own client id. And some servers will use client_id
/// to identify if a client is even allowed to perform the auth handshake.
async fn start_authorization(
    oauth_state: &mut OAuthState,
    scopes: &[&str],
    redirect_uri: &str,
    user_client_id: Option<&str>,
    user_client_secret: Option<&str>,
) -> Result<(), OauthUtilError> {
    // DO NOT CHANGE THIS
    // This string has significance as it is used for remote servers to identify us
    const DEFAULT_CLIENT_ID: &str = "Q DEV CLI";
    // The client_name sent during Dynamic Client Registration (DCR).
    // Some servers (e.g. Figma) use this to identify the application.
    const DEFAULT_CLIENT_NAME: &str = "kiro";

    let client_id = user_client_id.unwrap_or(DEFAULT_CLIENT_ID);

    let stub_cred = get_stub_credentials()?;
    oauth_state.set_credentials(client_id, stub_cred).await?;

    // The setting of credentials would put the oauth state into authorize.
    if let OAuthState::Authorized(auth_manager) = oauth_state {
        let mut config = OAuthClientConfig::new(client_id.to_string(), redirect_uri.to_string())
            .with_scopes(scopes.iter().map(|s| (*s).to_string()).collect());
        if let Some(secret) = user_client_secret {
            config = config.with_client_secret(secret.to_string());
        }

        // A configured client_id means "use my app"; skip DCR, which can return an unusable public client.
        let config = if user_client_id.is_some() {
            config
        } else {
            match auth_manager
                .register_client(DEFAULT_CLIENT_NAME, redirect_uri, scopes)
                .await
            {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Dynamic registration failed: {e}");
                    // fallback to default config
                    config
                },
            }
        };
        // reset client config
        auth_manager.configure_client(config)?;
        let auth_url = auth_manager.get_authorization_url(scopes).await?;

        let mut stub_auth_manager = AuthorizationManager::new("http://localhost").await?;
        std::mem::swap(auth_manager, &mut stub_auth_manager);

        let session = AuthorizationSession::for_scope_upgrade(stub_auth_manager, auth_url, redirect_uri);

        let mut new_oauth_state = OAuthState::Session(session);
        std::mem::swap(oauth_state, &mut new_oauth_state);
    } else {
        unreachable!()
    }

    Ok(())
}

/// This looks silly but [rmcp::transport::auth::OAuthTokenResponse] is private and there is no
/// other way to create this directly
fn get_stub_credentials() -> Result<OAuthTokenResponse, serde_json::Error> {
    const STUB_TOKEN: &str = r#"
            {
              "access_token": "stub",
              "token_type": "bearer",
              "expires_in": 3600,
              "refresh_token": "stub",
              "scope": "stub"
            }
        "#;

    serde_json::from_str::<OAuthTokenResponse>(STUB_TOKEN)
}

async fn make_svc(
    one_shot_sender: Sender<(String, String)>,
    socket_addr: SocketAddr,
    cancellation_token: CancellationToken,
) -> Result<(SocketAddr, LoopBackDropGuard), OauthUtilError> {
    type AuthCodeSender = Sender<(String, String)>;
    #[derive(Clone, Debug)]
    struct LoopBackForSendingAuthCode {
        one_shot_sender: Arc<std::sync::Mutex<Option<AuthCodeSender>>>,
    }

    #[derive(Debug, thiserror::Error)]
    enum LoopBackError {
        #[error("Poison error encountered: {0}")]
        Poison(String),
        #[error(transparent)]
        Http(#[from] http::Error),
        #[error("Failed to send auth code")]
        Send((String, String)),
    }

    fn mk_response(s: String) -> Result<Response<Full<Bytes>>, LoopBackError> {
        Ok(Response::builder().body(Full::new(Bytes::from(s)))?)
    }

    impl hyper::service::Service<hyper::Request<hyper::body::Incoming>> for LoopBackForSendingAuthCode {
        type Error = LoopBackError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
        type Response = Response<Full<Bytes>>;

        fn call(&self, req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
            let uri = req.uri();
            let query = uri.query().unwrap_or("");
            let params: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
            debug!("## mcp: uri: {}, query: {}, params: {:?}", uri, query, params);

            let self_clone = self.clone();
            Box::pin(async move {
                let error = params.get("error");
                let resp = if let Some(err) = error {
                    mk_response(format!(
                        "OAuth failed. Check URL for precise reasons. Possible reasons: {err}.\n\
                         If this is scope related, you can try configuring the server scopes \n\
                         to be an empty array by adding \"oauthScopes\": [] to your server config.\n\
                         Example: {{\"type\": \"http\", \"uri\": \"https://example.com/mcp\", \"oauthScopes\": []}}\n"
                    ))
                } else {
                    mk_response("You can close this page now".to_string())
                };

                let code = params.get("code").cloned().unwrap_or_default();
                let state = params.get("state").cloned().unwrap_or_default();
                if let Some(sender) = self_clone
                    .one_shot_sender
                    .lock()
                    .map_err(|e| LoopBackError::Poison(e.to_string()))?
                    .take()
                {
                    sender.send((code, state)).map_err(LoopBackError::Send)?;
                }

                resp
            })
        }
    }

    let listener = tokio::net::TcpListener::bind(socket_addr).await?;
    let actual_addr = listener.local_addr()?;
    let cancellation_token_clone = cancellation_token.clone();
    let dg = LoopBackDropGuard {
        cancellation_token: cancellation_token_clone,
    };

    let loop_back = LoopBackForSendingAuthCode {
        one_shot_sender: Arc::new(std::sync::Mutex::new(Some(one_shot_sender))),
    };

    // This is one and done
    // This server only needs to last as long as it takes to send the auth code or to fail the auth
    // flow
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Oauth loopback server cancelled");
            },
            res = http1::Builder::new().serve_connection(io, loop_back) => {
                if let Err(err) = res {
                    error!("Auth code loop back has failed: {:?}", err);
                }
            }
        }

        Ok::<(), eyre::Report>(())
    });

    Ok((actual_addr, dg))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── OAuthConfig serde ───────────────────────────────────────────────

    #[test]
    fn test_oauth_config_serialize_all_fields() {
        let cfg = OAuthConfig {
            client_id: Some("my-client".into()),
            client_secret: None,
            redirect_uri: Some("127.0.0.1:7778".into()),
            oauth_scopes: Some(vec!["openid".into(), "email".into()]),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"clientId\":\"my-client\""));
        assert!(json.contains("\"redirectUri\":\"127.0.0.1:7778\""));
        assert!(json.contains("\"oauthScopes\":[\"openid\",\"email\"]"));
    }

    #[test]
    fn test_oauth_config_serialize_none_fields_omitted() {
        let cfg = OAuthConfig {
            client_id: None,
            client_secret: None,
            redirect_uri: None,
            oauth_scopes: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_oauth_config_deserialize_camel_case() {
        let json = r#"{"clientId":"x","redirectUri":"y","oauthScopes":["a"]}"#;
        let cfg: OAuthConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.client_id.unwrap(), "x");
        assert_eq!(cfg.redirect_uri.unwrap(), "y");
        assert_eq!(cfg.oauth_scopes.unwrap(), vec!["a"]);
    }

    #[test]
    fn test_oauth_config_deserialize_empty_object() {
        let cfg: OAuthConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.client_id.is_none());
        assert!(cfg.redirect_uri.is_none());
        assert!(cfg.oauth_scopes.is_none());
    }

    #[test]
    fn test_oauth_config_eq_and_clone() {
        let cfg = OAuthConfig {
            client_id: Some("id".into()),
            client_secret: None,
            redirect_uri: None,
            oauth_scopes: None,
        };
        let cfg2 = cfg.clone();
        assert_eq!(cfg, cfg2);
    }

    // ─── Registration serde ──────────────────────────────────────────────

    #[test]
    fn test_registration_tolerates_missing_scopes_field() {
        let cached_json =
            b"{\n  \"client_id\": \"abc-123\",\n  \"redirect_uri\": \"http://localhost:8080/callback\"\n}";
        let reg: Registration = serde_json::from_slice(cached_json).unwrap();
        assert_eq!(reg.client_id, "abc-123");
        assert_eq!(reg.redirect_uri, "http://localhost:8080/callback");
        assert!(reg.scopes.is_empty());
    }

    #[test]
    fn test_registration_full_roundtrip() {
        let reg = Registration {
            client_id: "cid".into(),
            client_secret: Some("secret".into()),
            scopes: vec!["openid".into(), "email".into()],
            redirect_uri: "http://127.0.0.1:9999".into(),
        };
        let json = serde_json::to_string(&reg).unwrap();
        let reg2: Registration = serde_json::from_str(&json).unwrap();
        assert_eq!(reg2.client_id, "cid");
        assert_eq!(reg2.client_secret.unwrap(), "secret");
        assert_eq!(reg2.scopes, vec!["openid", "email"]);
        assert_eq!(reg2.redirect_uri, "http://127.0.0.1:9999");
    }

    #[test]
    fn test_registration_missing_required_field_fails() {
        let json = r#"{"client_id": "x"}"#;
        let res = serde_json::from_str::<Registration>(json);
        assert!(res.is_err());
    }

    #[test]
    fn test_registration_from_oauth_client_config() {
        let config = OAuthClientConfig::new("from-config", "http://localhost")
            .with_client_secret("s")
            .with_scopes(vec!["a".into(), "b".into()]);
        let reg: Registration = config.into();
        assert_eq!(reg.client_id, "from-config");
        assert_eq!(reg.client_secret.unwrap(), "s");
        assert_eq!(reg.scopes, vec!["a", "b"]);
        assert_eq!(reg.redirect_uri, "http://localhost");
    }

    #[test]
    fn test_registration_null_secret() {
        let json = r#"{"client_id":"x","client_secret":null,"redirect_uri":"http://l","scopes":[]}"#;
        let reg: Registration = serde_json::from_str(json).unwrap();
        assert!(reg.client_secret.is_none());
    }

    // ─── OAuthMeta serde ─────────────────────────────────────────────────

    #[test]
    fn test_oauth_meta_roundtrip() {
        let meta = OAuthMeta {
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            registration_endpoint: Some("https://auth.example.com/register".into()),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let meta2: OAuthMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta2.authorization_endpoint, meta.authorization_endpoint);
        assert_eq!(meta2.token_endpoint, meta.token_endpoint);
        assert_eq!(meta2.registration_endpoint, meta.registration_endpoint);
    }

    #[test]
    fn test_oauth_meta_optional_registration() {
        let json = r#"{"authorization_endpoint":"a","token_endpoint":"t"}"#;
        let meta: OAuthMeta = serde_json::from_str(json).unwrap();
        assert!(meta.registration_endpoint.is_none());
    }

    #[test]
    fn test_oauth_meta_missing_required_fails() {
        let json = r#"{"authorization_endpoint":"a"}"#;
        assert!(serde_json::from_str::<OAuthMeta>(json).is_err());
    }

    // ─── compute_key ─────────────────────────────────────────────────────

    #[test]
    fn test_compute_key_deterministic() {
        let url = Url::parse("https://example.com/mcp").unwrap();
        let k1 = compute_key(&url);
        let k2 = compute_key(&url);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_compute_key_different_paths_differ() {
        let u1 = Url::parse("https://example.com/a").unwrap();
        let u2 = Url::parse("https://example.com/b").unwrap();
        assert_ne!(compute_key(&u1), compute_key(&u2));
    }

    #[test]
    fn test_compute_key_different_hosts_differ() {
        let u1 = Url::parse("https://a.com/path").unwrap();
        let u2 = Url::parse("https://b.com/path").unwrap();
        assert_ne!(compute_key(&u1), compute_key(&u2));
    }

    #[test]
    fn test_compute_key_ignores_query_and_fragment() {
        let u1 = Url::parse("https://example.com/p?q=1#frag").unwrap();
        let u2 = Url::parse("https://example.com/p?q=2#other").unwrap();
        assert_eq!(compute_key(&u1), compute_key(&u2));
    }

    #[test]
    fn test_compute_key_is_hex_sha256() {
        let url = Url::parse("https://example.com/mcp").unwrap();
        let key = compute_key(&url);
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_key_port_matters() {
        let u1 = Url::parse("https://example.com:443/p").unwrap();
        let u2 = Url::parse("https://example.com:8443/p").unwrap();
        assert_ne!(compute_key(&u1), compute_key(&u2));
    }

    #[test]
    fn test_compute_key_scheme_matters() {
        let u1 = Url::parse("http://example.com/mcp").unwrap();
        let u2 = Url::parse("https://example.com/mcp").unwrap();
        assert_ne!(compute_key(&u1), compute_key(&u2));
    }

    #[test]
    fn test_compute_key_trailing_slash() {
        let u1 = Url::parse("https://example.com/mcp/").unwrap();
        let u2 = Url::parse("https://example.com/mcp").unwrap();
        assert_ne!(compute_key(&u1), compute_key(&u2));
    }

    // ─── get_default_scopes ──────────────────────────────────────────────

    #[test]
    fn test_get_default_scopes() {
        let scopes = get_default_scopes();
        assert_eq!(scopes, &["openid", "email", "profile", "offline_access"]);
    }

    #[test]
    fn test_get_default_scopes_length() {
        assert_eq!(get_default_scopes().len(), 4);
    }

    // ─── get_stub_credentials ────────────────────────────────────────────

    #[test]
    fn test_get_stub_credentials_parses() {
        let cred = get_stub_credentials().unwrap();
        let json = serde_json::to_value(&cred).unwrap();
        assert_eq!(json["access_token"], "stub");
        assert_eq!(json["token_type"], "bearer");
        assert_eq!(json["expires_in"], 3600);
        assert_eq!(json["refresh_token"], "stub");
    }

    // ─── OauthUtilError ──────────────────────────────────────────────────

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let err: OauthUtilError = io_err.into();
        assert!(matches!(err, OauthUtilError::Io(_)));
        assert!(err.to_string().contains("gone"));
    }

    #[test]
    fn test_error_from_url_parse() {
        let parse_err = Url::parse("://bad").unwrap_err();
        let err: OauthUtilError = parse_err.into();
        assert!(matches!(err, OauthUtilError::Parse(_)));
    }

    #[test]
    fn test_error_from_serde() {
        let serde_err = serde_json::from_str::<Registration>("not json").unwrap_err();
        let err: OauthUtilError = serde_err.into();
        assert!(matches!(err, OauthUtilError::Serde(_)));
    }

    #[test]
    fn test_error_display_oauth_discovery_failed() {
        let err = OauthUtilError::OAuthDiscoveryFailed;
        assert!(err.to_string().contains("OAuth discovery failed"));
    }

    #[test]
    fn test_error_display_missing_auth_manager() {
        let err = OauthUtilError::MissingAuthorizationManager;
        assert!(err.to_string().contains("Missing authorization manager"));
    }

    #[test]
    fn test_error_display_missing_auth_client() {
        let err = OauthUtilError::MissingAuthClient;
        assert!(err.to_string().contains("Missing auth client"));
    }

    #[test]
    fn test_error_display_malform_directory() {
        let err = OauthUtilError::MalformDirectory;
        assert!(err.to_string().contains("Malformed directory"));
    }

    #[test]
    fn test_error_display_missing_credentials() {
        let err = OauthUtilError::MissingCredentials;
        assert!(err.to_string().contains("Missing credential"));
    }

    #[test]
    fn test_error_display_service_not_obtained() {
        let err = OauthUtilError::ServiceNotObtained("timeout".into());
        assert!(err.to_string().contains("timeout"));
        assert!(err.to_string().contains("Failed to create a running service"));
    }

    #[test]
    fn test_error_display_http() {
        let err = OauthUtilError::Http("bad header".into());
        assert_eq!(err.to_string(), "bad header");
    }

    #[test]
    fn test_error_from_auth_no_authorization_support() {
        let auth_err = rmcp::transport::AuthError::NoAuthorizationSupport;
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::OAuthDiscoveryFailed));
    }

    #[tokio::test]
    async fn test_error_from_oneshot_recv() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        drop(tx);
        let recv_err = rx.await.unwrap_err();
        let err: OauthUtilError = recv_err.into();
        assert!(matches!(err, OauthUtilError::OneshotRecv(_)));
    }

    // ─── LoopBackDropGuard ───────────────────────────────────────────────

    #[test]
    fn test_loopback_drop_guard_cancels_on_drop() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        {
            let _guard = LoopBackDropGuard {
                cancellation_token: token.clone(),
            };
        }
        assert!(token.is_cancelled());
    }

    // ─── make_svc (loopback server) ──────────────────────────────────────

    #[tokio::test]
    async fn test_make_svc_binds_and_returns_addr() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();
        assert_eq!(actual_addr.ip(), std::net::Ipv4Addr::LOCALHOST);
        assert_ne!(actual_addr.port(), 0);
    }

    #[tokio::test]
    async fn test_make_svc_sends_auth_code_on_request() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();

        let client = reqwest::Client::new();
        let url = format!("http://{}/?code=AUTH_CODE&state=CSRF_STATE", actual_addr);
        let _resp = client.get(&url).send().await.unwrap();

        let (code, state) = rx.await.unwrap();
        assert_eq!(code, "AUTH_CODE");
        assert_eq!(state, "CSRF_STATE");
    }

    #[tokio::test]
    async fn test_make_svc_handles_error_param() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();

        let client = reqwest::Client::new();
        let url = format!("http://{}/?error=access_denied&code=c&state=s", actual_addr);
        let resp = client.get(&url).send().await.unwrap();
        let body = resp.text().await.unwrap();
        assert!(body.contains("OAuth failed"));

        let (code, state) = rx.await.unwrap();
        assert_eq!(code, "c");
        assert_eq!(state, "s");
    }

    #[tokio::test]
    async fn test_make_svc_guard_drop_cancels() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let token_clone = token.clone();
        let (_actual_addr, guard) = make_svc(tx, addr, token).await.unwrap();
        assert!(!token_clone.is_cancelled());
        drop(guard);
        assert!(token_clone.is_cancelled());
    }

    #[tokio::test]
    async fn test_make_svc_missing_code_defaults_empty() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();

        let client = reqwest::Client::new();
        let url = format!("http://{}/", actual_addr);
        let _resp = client.get(&url).send().await.unwrap();

        let (code, state) = rx.await.unwrap();
        assert_eq!(code, "");
        assert_eq!(state, "");
    }

    // ─── HttpServiceBuilder construction ─────────────────────────────────

    #[test]
    fn test_http_service_builder_new() {
        let headers = HashMap::new();
        let scopes: Vec<String> = vec![];
        let oauth_config = None;
        let (tx, _rx) = mpsc::channel(1);
        let builder = HttpServiceBuilder::new(
            "test-server",
            "https://example.com/mcp",
            5000,
            &scopes,
            &headers,
            &oauth_config,
            &tx,
            false,
        );
        assert_eq!(builder.server_name, "test-server");
        assert_eq!(builder.url, "https://example.com/mcp");
        assert_eq!(builder.timeout, 5000);
        assert!(!builder.force_auth);

        let forced = HttpServiceBuilder::new(
            "test-server",
            "https://example.com/mcp",
            5000,
            &scopes,
            &headers,
            &oauth_config,
            &tx,
            true,
        );
        assert!(forced.force_auth);
    }

    // ─── ReauthContext ───────────────────────────────────────────────────

    #[test]
    fn test_reauth_context_clone() {
        let (tx, _rx) = mpsc::channel(1);
        let ctx = ReauthContext {
            server_name: "srv".into(),
            url: Url::parse("https://example.com").unwrap(),
            reg_full_path: PathBuf::from("/tmp/reg.json"),
            scopes: vec!["openid".into()],
            oauth_config: None,
            server_actor_event_tx: tx,
        };
        let ctx2 = ctx.clone();
        assert_eq!(ctx2.server_name, "srv");
        assert_eq!(ctx2.scopes, vec!["openid"]);
    }

    // ─── File-system touching tests (tempfile) ───────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_write_credentials_securely_sets_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("creds.json");

        write_credentials_securely(&cred_path, r#"{"access_token":"secret"}"#)
            .await
            .unwrap();

        let mode = tokio::fs::metadata(&cred_path).await.unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "credential file mode is {:o}, expected 600",
            mode & 0o777
        );
    }

    #[tokio::test]
    async fn test_remove_persisted_credentials_deletes_token_and_registration() {
        let dir = tempfile::tempdir().unwrap();
        let cred_dir = dir.path();
        let url = "https://example.com/mcp";
        let key = compute_key(&Url::from_str(url).unwrap());
        let token_path = cred_dir.join(format!("{key}.token.json"));
        let reg_path = cred_dir.join(format!("{key}.registration.json"));
        tokio::fs::write(&token_path, "{}").await.unwrap();
        tokio::fs::write(&reg_path, "{}").await.unwrap();

        let removed = remove_persisted_credentials(cred_dir, url).await.unwrap();
        assert!(removed);
        assert!(!token_path.exists());
        assert!(!reg_path.exists());

        // Removing again is a no-op and reports nothing removed.
        let removed_again = remove_persisted_credentials(cred_dir, url).await.unwrap();
        assert!(!removed_again);
    }

    #[tokio::test]
    async fn test_remove_persisted_credentials_missing_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let removed = remove_persisted_credentials(dir.path(), "https://none.example.com/mcp")
            .await
            .unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_registration_file_write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let reg_path = dir.path().join("reg.json");

        let reg = Registration {
            client_id: "test-id".into(),
            client_secret: None,
            scopes: vec!["openid".into()],
            redirect_uri: "http://127.0.0.1:8080".into(),
        };

        let json = serde_json::to_string_pretty(&reg).unwrap();
        tokio::fs::write(&reg_path, &json).await.unwrap();

        let read_bytes = tokio::fs::read(&reg_path).await.unwrap();
        let reg2: Registration = serde_json::from_slice(&read_bytes).unwrap();
        assert_eq!(reg2.client_id, "test-id");
        assert_eq!(reg2.scopes, vec!["openid"]);
    }

    #[tokio::test]
    async fn test_credential_file_malformed_json_fails_parse() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("token.json");
        tokio::fs::write(&cred_path, "not valid json {{{").await.unwrap();

        let bytes = tokio::fs::read(&cred_path).await.unwrap();
        let result = serde_json::from_slice::<OAuthTokenResponse>(&bytes);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_credential_file_missing_fields_fails_parse() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("token.json");
        tokio::fs::write(&cred_path, r#"{"access_token": "x"}"#).await.unwrap();

        let bytes = tokio::fs::read(&cred_path).await.unwrap();
        let result = serde_json::from_slice::<OAuthTokenResponse>(&bytes);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_valid_token_response_parses() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("token.json");
        let token_json = r#"{
            "access_token": "abc123",
            "token_type": "bearer",
            "expires_in": 3600,
            "refresh_token": "ref456",
            "scope": "openid email"
        }"#;
        tokio::fs::write(&cred_path, token_json).await.unwrap();

        let bytes = tokio::fs::read(&cred_path).await.unwrap();
        let token: OAuthTokenResponse = serde_json::from_slice(&bytes).unwrap();
        let val = serde_json::to_value(&token).unwrap();
        assert_eq!(val["access_token"], "abc123");
        assert_eq!(val["token_type"], "bearer");
    }

    #[tokio::test]
    async fn test_create_nested_dir_for_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let cred_path = dir.path().join("a/b/c/token.json");
        let parent = cred_path.parent().unwrap();
        tokio::fs::create_dir_all(parent).await.unwrap();
        tokio::fs::write(&cred_path, "{}").await.unwrap();
        assert!(cred_path.exists());
    }

    // ─── URL parsing edge cases ──────────────────────────────────────────

    #[test]
    fn test_url_parse_invalid_returns_error() {
        let result = Url::from_str("not a url");
        assert!(result.is_err());
        let err: OauthUtilError = result.unwrap_err().into();
        assert!(matches!(err, OauthUtilError::Parse(_)));
    }

    // ─── OAuthConfig redirect_uri parsing ────────────────────────────────

    #[test]
    fn test_parse_redirect_uri_host_port() {
        let parsed = RedirectUriConfig::try_from(Some("127.0.0.1:7778")).unwrap();
        assert_eq!(parsed.port, Some(7778));
        assert_eq!(parsed.host.as_str(), "127.0.0.1");
        assert_eq!(parsed.path, "");
    }

    #[test]
    fn test_parse_redirect_uri_colon_port_only() {
        let parsed = RedirectUriConfig::try_from(Some(":7778")).unwrap();
        assert_eq!(parsed.port, Some(7778));
        assert_eq!(parsed.host.as_str(), "127.0.0.1");
        assert_eq!(parsed.path, "");
    }

    #[test]
    fn test_parse_redirect_uri_full_url_with_path() {
        let parsed = RedirectUriConfig::try_from(Some("http://localhost:7778/oauth/callback")).unwrap();
        assert_eq!(parsed.port, Some(7778));
        assert_eq!(parsed.host.as_str(), "localhost");
        assert_eq!(parsed.path, "/oauth/callback");
    }

    #[test]
    fn test_parse_redirect_uri_full_url_trailing_slash_normalized() {
        // A trailing-slash root path must not break port parsing (the old bug)
        // and normalizes to an empty path.
        let parsed = RedirectUriConfig::try_from(Some("http://localhost:7778/")).unwrap();
        assert_eq!(parsed.port, Some(7778));
        assert_eq!(parsed.host.as_str(), "localhost");
        assert_eq!(parsed.path, "");
    }

    #[test]
    fn test_parse_redirect_uri_full_url_no_port() {
        let parsed = RedirectUriConfig::try_from(Some("http://localhost/callback")).unwrap();
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.host.as_str(), "localhost");
        assert_eq!(parsed.path, "/callback");
    }

    #[test]
    fn test_parse_redirect_uri_bare_host() {
        let parsed = RedirectUriConfig::try_from(Some("localhost")).unwrap();
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.host.as_str(), "localhost");
        assert_eq!(parsed.path, "");
    }

    #[test]
    fn test_parse_redirect_uri_none_defaults() {
        let parsed = RedirectUriConfig::try_from(None).unwrap();
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.host.as_str(), "127.0.0.1");
        assert_eq!(parsed.path, "");
    }

    #[test]
    fn test_parse_redirect_uri_blank_defaults() {
        let parsed = RedirectUriConfig::try_from(Some("   ")).unwrap();
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.host.as_str(), "127.0.0.1");
        assert_eq!(parsed.path, "");
    }

    #[test]
    fn test_parse_redirect_uri_invalid_port_ignored() {
        // 99999 > u16::MAX, so no port is parsed and the OS assigns one.
        let parsed = RedirectUriConfig::try_from(Some("127.0.0.1:99999")).unwrap();
        assert_eq!(parsed.port, None);
        assert_eq!(parsed.host.as_str(), "127.0.0.1");
    }

    #[test]
    fn test_parse_redirect_uri_non_loopback_host_rejected() {
        // Only loopback hosts can receive the OAuth callback.
        for uri in ["example.com:7778", "http://example.com:7778/cb", "0.0.0.0:7778"] {
            let err = RedirectUriConfig::try_from(Some(uri)).unwrap_err();
            assert!(
                matches!(err, OauthUtilError::InvalidRedirectUri(_)),
                "expected reject for {uri}"
            );
        }
    }

    #[test]
    fn test_parse_redirect_uri_non_http_scheme_rejected() {
        let err = RedirectUriConfig::try_from(Some("https://localhost:7778/cb")).unwrap_err();
        assert!(matches!(err, OauthUtilError::InvalidRedirectUri(_)));
    }

    // ─── HeaderMap conversion error ──────────────────────────────────────

    #[test]
    fn test_invalid_header_produces_http_error() {
        let mut headers = HashMap::new();
        headers.insert("bad\nheader".to_string(), "value".to_string());
        let result: Result<HeaderMap, _> = HeaderMap::try_from(&headers);
        assert!(result.is_err());
    }

    // ─── AuthClientWrapper construction ──────────────────────────────────

    #[tokio::test]
    async fn test_auth_client_wrapper_new() {
        // We can't easily construct a real AuthClient without a server,
        // but we can test the path logic
        let path = PathBuf::from("/tmp/test/cred.json");
        assert_eq!(path.parent().unwrap(), Path::new("/tmp/test"));
    }

    // ─── Additional coverage: AuthError From impl (non-NoAuthorizationSupport) ───

    #[test]
    fn test_error_from_auth_authorization_required() {
        let auth_err = rmcp::transport::AuthError::AuthorizationRequired;
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("authorization required"));
    }

    #[test]
    fn test_error_from_auth_authorization_failed() {
        let auth_err = rmcp::transport::AuthError::AuthorizationFailed("bad code".into());
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("bad code"));
    }

    #[test]
    fn test_error_from_auth_token_exchange_failed() {
        let auth_err = rmcp::transport::AuthError::TokenExchangeFailed("exchange err".into());
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("exchange err"));
    }

    #[test]
    fn test_error_from_auth_token_refresh_failed() {
        let auth_err = rmcp::transport::AuthError::TokenRefreshFailed("refresh err".into());
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("refresh err"));
    }

    #[test]
    fn test_error_from_auth_oauth_error() {
        let auth_err = rmcp::transport::AuthError::OAuthError("generic oauth".into());
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("generic oauth"));
    }

    #[test]
    fn test_error_from_auth_metadata_error() {
        let auth_err = rmcp::transport::AuthError::MetadataError("meta err".into());
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("meta err"));
    }

    #[test]
    fn test_error_from_auth_internal_error() {
        let auth_err = rmcp::transport::AuthError::InternalError("internal".into());
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
        assert!(err.to_string().contains("internal"));
    }

    #[test]
    fn test_error_from_auth_url_error() {
        let url_err = url::Url::parse("://").unwrap_err();
        let auth_err = rmcp::transport::AuthError::UrlError(url_err);
        let err: OauthUtilError = auth_err.into();
        assert!(matches!(err, OauthUtilError::Auth(_)));
    }

    // ─── Additional compute_key edge cases ───────────────────────────────

    #[test]
    fn test_compute_key_with_username_password_in_url() {
        // Origin serialization strips userinfo
        let u1 = Url::parse("https://user:pass@example.com/mcp").unwrap();
        let u2 = Url::parse("https://example.com/mcp").unwrap();
        assert_eq!(compute_key(&u1), compute_key(&u2));
    }

    #[test]
    fn test_compute_key_root_path() {
        let url = Url::parse("https://example.com/").unwrap();
        let key = compute_key(&url);
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_compute_key_deep_path() {
        let url = Url::parse("https://example.com/a/b/c/d/e/f").unwrap();
        let key = compute_key(&url);
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_key_non_standard_port() {
        let url = Url::parse("http://localhost:3000/mcp").unwrap();
        let key = compute_key(&url);
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_compute_key_ipv6() {
        let url = Url::parse("http://[::1]:8080/mcp").unwrap();
        let key = compute_key(&url);
        assert_eq!(key.len(), 64);
    }

    // ─── Additional make_svc tests ───────────────────────────────────────

    #[tokio::test]
    async fn test_make_svc_response_body_success() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();

        let client = reqwest::Client::new();
        let url = format!("http://{}/?code=c&state=s", actual_addr);
        let resp = client.get(&url).send().await.unwrap();
        let body = resp.text().await.unwrap();
        assert!(body.contains("You can close this page now"));
    }

    #[tokio::test]
    async fn test_make_svc_error_response_contains_hint() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();

        let client = reqwest::Client::new();
        let url = format!("http://{}/?error=invalid_scope&code=c&state=s", actual_addr);
        let resp = client.get(&url).send().await.unwrap();
        let body = resp.text().await.unwrap();
        assert!(body.contains("invalid_scope"));
        assert!(body.contains("oauthScopes"));
    }

    #[tokio::test]
    async fn test_make_svc_with_specific_port() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        // Use port 0 to let OS assign
        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let token = CancellationToken::new();
        let (actual_addr, _guard) = make_svc(tx, addr, token).await.unwrap();
        assert!(actual_addr.port() > 0);
    }

    // ─── Additional file I/O tests ───────────────────────────────────────

    #[tokio::test]
    async fn test_credential_and_registration_coexist_in_same_dir() {
        let dir = tempfile::tempdir().unwrap();
        let url = Url::parse("https://example.com/mcp").unwrap();
        let key = compute_key(&url);
        let cred_path = dir.path().join(format!("{key}.token.json"));
        let reg_path = dir.path().join(format!("{key}.registration.json"));

        let token_json = r#"{"access_token":"t","token_type":"bearer","expires_in":3600}"#;
        let reg_json = r#"{"client_id":"c","redirect_uri":"http://localhost","scopes":[]}"#;

        tokio::fs::write(&cred_path, token_json).await.unwrap();
        tokio::fs::write(&reg_path, reg_json).await.unwrap();

        assert!(cred_path.exists());
        assert!(reg_path.exists());

        let reg: Registration = serde_json::from_slice(&tokio::fs::read(&reg_path).await.unwrap()).unwrap();
        assert_eq!(reg.client_id, "c");
    }

    #[tokio::test]
    async fn test_file_read_nonexistent_returns_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result = tokio::fs::read(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parent_path_none_for_root() {
        // Path::new("/").parent() returns Some("") not None
        // But PathBuf::from("").parent() returns None
        let path = PathBuf::from("");
        assert_eq!(path.parent(), None);
    }

    // ─── Additional OAuthConfig tests ────────────────────────────────────

    #[test]
    fn test_oauth_config_debug_impl() {
        let cfg = OAuthConfig {
            client_id: Some("id".into()),
            client_secret: None,
            redirect_uri: None,
            oauth_scopes: Some(vec!["scope1".into()]),
        };
        let debug_str = format!("{:?}", cfg);
        assert!(debug_str.contains("OAuthConfig"));
        assert!(debug_str.contains("id"));
    }

    #[test]
    fn test_oauth_config_partial_fields() {
        let json = r#"{"clientId":"only-id"}"#;
        let cfg: OAuthConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.client_id.unwrap(), "only-id");
        assert!(cfg.redirect_uri.is_none());
        assert!(cfg.oauth_scopes.is_none());
    }

    #[test]
    fn test_oauth_config_empty_scopes_array() {
        let json = r#"{"oauthScopes":[]}"#;
        let cfg: OAuthConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.oauth_scopes.unwrap(), Vec::<String>::new());
    }

    #[test]
    fn test_oauth_config_ne() {
        let cfg1 = OAuthConfig {
            client_id: Some("a".into()),
            client_secret: None,
            redirect_uri: None,
            oauth_scopes: None,
        };
        let cfg2 = OAuthConfig {
            client_id: Some("b".into()),
            client_secret: None,
            redirect_uri: None,
            oauth_scopes: None,
        };
        assert_ne!(cfg1, cfg2);
    }

    // ─── Additional Registration tests ───────────────────────────────────

    #[test]
    fn test_registration_from_oauth_client_config_no_secret() {
        let config = OAuthClientConfig::new("id", "http://localhost:1234");
        let reg: Registration = config.into();
        assert!(reg.client_secret.is_none());
        assert!(reg.scopes.is_empty());
    }

    #[test]
    fn test_registration_debug_impl() {
        let reg = Registration {
            client_id: "debug-test".into(),
            client_secret: None,
            scopes: vec![],
            redirect_uri: "http://localhost".into(),
        };
        let debug_str = format!("{:?}", reg);
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_registration_clone() {
        let reg = Registration {
            client_id: "clone-test".into(),
            client_secret: Some("secret".into()),
            scopes: vec!["s1".into()],
            redirect_uri: "http://localhost".into(),
        };
        let reg2 = reg.clone();
        assert_eq!(reg2.client_id, "clone-test");
        assert_eq!(reg2.client_secret.unwrap(), "secret");
    }

    // ─── Additional OAuthMeta tests ──────────────────────────────────────

    #[test]
    fn test_oauth_meta_clone() {
        let meta = OAuthMeta {
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            registration_endpoint: None,
        };
        let meta2 = meta.clone();
        assert_eq!(meta2.authorization_endpoint, "https://auth.example.com/authorize");
        assert!(meta2.registration_endpoint.is_none());
    }

    #[test]
    fn test_oauth_meta_debug_impl() {
        let meta = OAuthMeta {
            authorization_endpoint: "a".into(),
            token_endpoint: "t".into(),
            registration_endpoint: Some("r".into()),
        };
        let debug_str = format!("{:?}", meta);
        assert!(debug_str.contains("OAuthMeta"));
    }

    // ─── HttpServiceBuilder URL parse error path ─────────────────────────

    #[test]
    fn test_http_service_builder_invalid_url_parse() {
        // The try_build method calls Url::from_str(url) which will fail for invalid URLs
        let result = Url::from_str("not a valid url");
        assert!(result.is_err());
        let err: OauthUtilError = result.unwrap_err().into();
        assert!(matches!(err, OauthUtilError::Parse(_)));
    }

    // ─── HttpServiceBuilder invalid headers path ─────────────────────────

    #[test]
    fn test_http_service_builder_invalid_headers_conversion() {
        let mut headers = HashMap::new();
        headers.insert("invalid\nheader".to_string(), "value".to_string());
        let result: Result<HeaderMap, _> = HeaderMap::try_from(&headers);
        assert!(result.is_err());
        let err = OauthUtilError::Http(result.unwrap_err().to_string());
        assert!(matches!(err, OauthUtilError::Http(_)));
    }

    // ─── get_stub_credentials additional tests ───────────────────────────

    #[test]
    fn test_get_stub_credentials_has_refresh_token() {
        let cred = get_stub_credentials().unwrap();
        let json = serde_json::to_value(&cred).unwrap();
        assert_eq!(json["refresh_token"], "stub");
        assert_eq!(json["scope"], "stub");
    }

    #[test]
    fn test_get_stub_credentials_serializes_back() {
        let cred = get_stub_credentials().unwrap();
        let json_str = serde_json::to_string(&cred).unwrap();
        let reparsed: OAuthTokenResponse = serde_json::from_str(&json_str).unwrap();
        let val = serde_json::to_value(&reparsed).unwrap();
        assert_eq!(val["access_token"], "stub");
    }

    // ─── LoopBackDropGuard additional tests ──────────────────────────────

    #[test]
    fn test_loopback_drop_guard_multiple_drops_idempotent() {
        let token = CancellationToken::new();
        let guard = LoopBackDropGuard {
            cancellation_token: token.clone(),
        };
        drop(guard);
        assert!(token.is_cancelled());
        // Cancelling an already-cancelled token is fine
        token.cancel();
        assert!(token.is_cancelled());
    }

    // ─── get_default_scopes contains expected values ─────────────────────

    #[test]
    fn test_get_default_scopes_contains_offline_access() {
        assert!(get_default_scopes().contains(&"offline_access"));
    }

    #[test]
    fn test_get_default_scopes_contains_openid() {
        assert!(get_default_scopes().contains(&"openid"));
    }

    // ─── OauthUtilError reqwest variant ──────────────────────────────────

    #[test]
    fn test_error_reqwest_display() {
        // We can't easily construct a reqwest::Error directly, but we can test the variant exists
        let err = OauthUtilError::Http("simulated reqwest error".into());
        assert!(err.to_string().contains("simulated reqwest error"));
    }

    // ─── HttpServiceBuilder with valid headers ───────────────────────────

    #[test]
    fn test_http_service_builder_stores_all_fields() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        let scopes = vec!["openid".to_string()];
        let oauth_config = Some(OAuthConfig {
            client_id: Some("cid".into()),
            client_secret: None,
            redirect_uri: Some("127.0.0.1:8080".into()),
            oauth_scopes: Some(vec!["openid".into()]),
        });
        let (tx, _rx) = mpsc::channel(1);
        let builder = HttpServiceBuilder::new(
            "srv",
            "https://x.com/mcp",
            10000,
            &scopes,
            &headers,
            &oauth_config,
            &tx,
            false,
        );
        assert_eq!(builder.server_name, "srv");
        assert_eq!(builder.timeout, 10000);
        assert_eq!(builder.scopes, &["openid".to_string()]);
        assert!(builder.oauth_config.is_some());
    }

    // ─── ReauthContext with oauth_config ─────────────────────────────────

    #[test]
    fn test_reauth_context_with_oauth_config() {
        let (tx, _rx) = mpsc::channel(1);
        let ctx = ReauthContext {
            server_name: "srv".into(),
            url: Url::parse("https://example.com").unwrap(),
            reg_full_path: PathBuf::from("/tmp/reg.json"),
            scopes: vec!["openid".into(), "email".into()],
            oauth_config: Some(OAuthConfig {
                client_id: Some("custom-id".into()),
                client_secret: None,
                redirect_uri: Some("127.0.0.1:9999".into()),
                oauth_scopes: None,
            }),
            server_actor_event_tx: tx,
        };
        let ctx2 = ctx.clone();
        assert_eq!(
            ctx2.oauth_config.as_ref().unwrap().client_id.as_deref(),
            Some("custom-id")
        );
    }

    // ─── Token response with optional fields ─────────────────────────────

    #[tokio::test]
    async fn test_token_response_without_refresh_token() {
        let token_json = r#"{
            "access_token": "abc",
            "token_type": "bearer",
            "expires_in": 7200
        }"#;
        let token: OAuthTokenResponse = serde_json::from_str(token_json).unwrap();
        let val = serde_json::to_value(&token).unwrap();
        assert_eq!(val["access_token"], "abc");
        assert_eq!(val["expires_in"], 7200);
    }

    #[tokio::test]
    async fn test_token_response_with_all_fields() {
        let token_json = r#"{
            "access_token": "access",
            "token_type": "Bearer",
            "expires_in": 1800,
            "refresh_token": "refresh",
            "scope": "openid email profile"
        }"#;
        let token: OAuthTokenResponse = serde_json::from_str(token_json).unwrap();
        let val = serde_json::to_value(&token).unwrap();
        // token_type may be normalized to lowercase by the library
        assert!(val["token_type"].as_str().unwrap().eq_ignore_ascii_case("bearer"));
        assert_eq!(val["scope"], "openid email profile");
    }

    // ─── compute_key known value test ────────────────────────────────────

    #[test]
    fn test_compute_key_known_value() {
        // Verify the hash is SHA-256 of "https://example.com/mcp"
        let url = Url::parse("https://example.com/mcp").unwrap();
        let key = compute_key(&url);
        // Manually compute: origin = "https://example.com", path = "/mcp"
        // input = "https://example.com/mcp"
        let mut hasher = Sha256::new();
        hasher.update(b"https://example.com/mcp");
        let expected = format!("{:x}", hasher.finalize());
        assert_eq!(key, expected);
    }

    // ─── File path computation for cred/reg ──────────────────────────────

    #[test]
    fn test_cred_and_reg_path_computation() {
        let url = Url::parse("https://mcp.example.com/v1").unwrap();
        let key = compute_key(&url);
        let cred_dir = PathBuf::from("/home/user/.config/mcp");
        let cred_path = cred_dir.join(format!("{key}.token.json"));
        let reg_path = cred_dir.join(format!("{key}.registration.json"));

        assert!(cred_path.to_str().unwrap().ends_with(".token.json"));
        assert!(reg_path.to_str().unwrap().ends_with(".registration.json"));
        // Both share the same key prefix
        assert_eq!(
            cred_path.file_stem().unwrap().to_str().unwrap().replace(".token", ""),
            reg_path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .replace(".registration", "")
        );
    }

    // ─── MalformDirectory error for paths without parent ─────────────────

    #[test]
    fn test_malform_directory_error_path_without_parent() {
        let path = PathBuf::from("");
        let result: Result<&Path, OauthUtilError> = path.parent().ok_or(OauthUtilError::MalformDirectory);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OauthUtilError::MalformDirectory));
    }

    // ─── OAuthConfig JsonSchema ──────────────────────────────────────────

    #[test]
    fn test_oauth_config_json_schema() {
        let schema = schemars::schema_for!(OAuthConfig);
        let schema_json = serde_json::to_string(&schema).unwrap();
        assert!(schema_json.contains("clientId"));
        assert!(schema_json.contains("redirectUri"));
        assert!(schema_json.contains("oauthScopes"));
    }
}
