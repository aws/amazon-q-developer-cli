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
    AuthorizationMetadata,
    CredentialStore,
    InMemoryCredentialStore,
    OAuthClientConfig,
    OAuthState,
    OAuthTokenResponse,
    StoredCredentials,
};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{
    AuthorizationManager,
    AuthorizationRequest,
    AuthorizationSession,
    StreamableHttpClientTransport,
};
use rmcp::{
    RoleClient,
    Service,
    serde_json,
};
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use tokio::sync::oneshot::Sender;
use tokio_util::sync::CancellationToken;
use tracing::{
    debug,
    error,
    info,
    warn,
};
use url::Url;

use super::messenger::Messenger;
use crate::os::Os;
use crate::util::paths::DirectoryError;

/// Builds a reqwest Client with a User-Agent header set.
/// Some MCP servers sit behind CloudFront WAFs that reject requests without a
/// User-Agent, returning 403 instead of the expected 401 with WWW-Authenticate.
fn oauth_discovery_client() -> Result<Client, OauthUtilError> {
    Ok(reqwest::ClientBuilder::new()
        .user_agent(concat!("kiro-cli/", env!("CARGO_PKG_VERSION")))
        .build()?)
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
    Directory(#[from] DirectoryError),
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
#[allow(dead_code)]
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
    pub url: Url,
    pub reg_full_path: PathBuf,
    pub scopes: Vec<String>,
    pub oauth_config: Option<crate::cli::chat::tools::custom_tool::OAuthConfig>,
    pub messenger: Arc<dyn Messenger>,
    pub os: Os,
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

/// The granted `scope` of a cached token response. Read straight from the JSON
/// because the SDK's own accessor sits behind an `oauth2` trait this crate does
/// not depend on.
#[derive(Deserialize)]
struct GrantedScopes {
    #[serde(default)]
    scope: Option<String>,
}

impl GrantedScopes {
    /// RFC 6749 §3.3: `scope` is a space-delimited list. An absent or malformed
    /// field yields no scopes, matching a token response that omits it.
    fn parse(token_bytes: &[u8]) -> Vec<String> {
        serde_json::from_slice::<Self>(token_bytes)
            .ok()
            .and_then(|granted| granted.scope)
            .map(|scope| scope.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    }
}

/// Filesystem locations of the OAuth state cached for one remote MCP server.
struct OAuthCachePaths {
    token: PathBuf,
    registration: PathBuf,
    /// Authorization server metadata, so a later connect can skip discovery.
    metadata: PathBuf,
}

impl OAuthCachePaths {
    fn new(cred_dir: &Path, key: &str) -> Self {
        Self {
            token: cred_dir.join(format!("{key}.token.json")),
            registration: cred_dir.join(format!("{key}.registration.json")),
            metadata: cred_dir.join(format!("{key}.metadata.json")),
        }
    }
}

/// Reads cached authorization server metadata, or `None` when it is absent or
/// unusable. Metadata is a cache, so a bad file must never fail a connection:
/// the caller runs discovery again and overwrites it.
async fn read_cached_metadata(path: &Path) -> Option<AuthorizationMetadata> {
    let bytes = tokio::fs::read(path).await.ok()?;
    match serde_json::from_slice::<AuthorizationMetadata>(&bytes) {
        Ok(metadata) => Some(metadata),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                "## mcp: cached authorization server metadata failed to parse, re-running discovery: {e}"
            );
            None
        },
    }
}

/// Persists discovered authorization server metadata. Best effort: a failed
/// write only costs another discovery pass on the next connect.
async fn write_cached_metadata(path: &Path, metadata: &AuthorizationMetadata) {
    let write = async {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, serde_json::to_vec_pretty(metadata)?).await?;
        Ok::<(), OauthUtilError>(())
    };

    if let Err(e) = write.await {
        tracing::warn!(
            path = %path.display(),
            "## mcp: failed to cache authorization server metadata: {e}"
        );
    }
}

/// Discards every cached OAuth artifact for one server. The three files describe
/// one authorization: keeping any of them after rejecting the others would let a
/// stale authorization server survive into the next connect.
async fn remove_cached_oauth_state(paths: &OAuthCachePaths) {
    for path in [&paths.token, &paths.registration, &paths.metadata] {
        let _ = tokio::fs::remove_file(path).await;
    }
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

            let oauth_state = OAuthState::new(ctx.url.clone(), Some(oauth_discovery_client()?))
                .await
                .map_err(|e| {
                    error!("## mcp: reauthorize failed to create OAuthState for {}: {e}", ctx.url);
                    e
                })?;
            let (new_am, redirect_uri) = get_auth_manager_impl(
                oauth_state,
                &ctx.scopes,
                &ctx.oauth_config,
                ctx.messenger.as_ref(),
                &ctx.os,
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
                scopes: ctx.scopes.clone(),
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
    pub os: &'a Os,
    pub url: &'a str,
    pub timeout: u64,
    pub scopes: &'a [String],
    pub headers: &'a HashMap<String, String>,
    pub oauth_config: &'a Option<crate::cli::chat::tools::custom_tool::OAuthConfig>,
    pub messenger: &'a dyn Messenger,
}

impl<'a> HttpServiceBuilder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        server_name: &'a str,
        os: &'a Os,
        url: &'a str,
        timeout: u64,
        scopes: &'a [String],
        headers: &'a HashMap<String, String>,
        oauth_config: &'a Option<crate::cli::chat::tools::custom_tool::OAuthConfig>,
        messenger: &'a dyn Messenger,
    ) -> Self {
        Self {
            server_name,
            os,
            url,
            timeout,
            scopes,
            headers,
            oauth_config,
            messenger,
        }
    }

    pub async fn try_build<S: Service<RoleClient> + Clone>(
        self,
        service: &S,
    ) -> Result<HttpRunningService, OauthUtilError> {
        let HttpServiceBuilder {
            server_name,
            os,
            url,
            timeout,
            scopes,
            headers,
            oauth_config,
            messenger,
        } = self;

        let mut state = HttpServiceBuilderState::TryUnauthenticated;
        let cred_dir = os.path_resolver().global().mcp_auth_dir()?;
        let url = Url::from_str(url)?;
        let key = compute_key(&url);
        let paths = OAuthCachePaths::new(&cred_dir, &key);
        let cred_full_path = paths.token.clone();
        let reg_full_path = paths.registration.clone();
        let mut auth_client = None::<AuthClient<Client>>;

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
                            let am = get_auth_manager(url.clone(), &paths, scopes, oauth_config, messenger, os).await?;

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
                                url: url.clone(),
                                reg_full_path,
                                scopes: scopes.to_vec(),
                                oauth_config: oauth_config.clone(),
                                messenger: Arc::from(messenger.duplicate()),
                                os: os.clone(),
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
                        // Cached endpoints just failed to serve a refresh, so drop them and let
                        // the retry rediscover rather than reusing a server that may have moved.
                        let _ = tokio::fs::remove_file(&paths.metadata).await;
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
    url: Url,
    paths: &OAuthCachePaths,
    scopes: &[String],
    oauth_config: &Option<crate::cli::chat::tools::custom_tool::OAuthConfig>,
    messenger: &dyn Messenger,
    os: &Os,
) -> Result<AuthorizationManager, OauthUtilError> {
    let cred_as_bytes = tokio::fs::read(&paths.token).await;
    let reg_as_bytes = tokio::fs::read(&paths.registration).await;

    // If cached credentials exist and parse, use them. Otherwise fall through
    // to a fresh OAuth flow (and remove the bad files so we don't loop on them).
    let cached_path = if let (Ok(cred_bytes), Ok(reg_bytes)) = (cred_as_bytes, reg_as_bytes) {
        match (
            serde_json::from_slice::<OAuthTokenResponse>(&cred_bytes),
            serde_json::from_slice::<Registration>(&reg_bytes),
        ) {
            (Ok(token), Ok(reg)) => Some((token, reg, GrantedScopes::parse(&cred_bytes))),
            (cred_res, reg_res) => {
                tracing::warn!(
                    cred_err = ?cred_res.err(),
                    reg_err = ?reg_res.err(),
                    "## mcp: cached OAuth credentials failed to parse, removing and re-running OAuth flow"
                );
                // Remove malformed caches so we don't loop on them.
                remove_cached_oauth_state(paths).await;
                None
            },
        }
    } else {
        None
    };

    match cached_path {
        Some((token, reg, granted_scopes)) => {
            let OAuthState::Unauthorized(mut am) =
                OAuthState::new(url.clone(), Some(oauth_discovery_client()?)).await?
            else {
                return Err(OauthUtilError::MissingAuthorizationManager);
            };

            // Resolve the authorization server first: the SDK only adopts stored
            // credentials without rediscovery once its metadata is set.
            let (metadata, minted_under) = match read_cached_metadata(&paths.metadata).await {
                Some(metadata) => {
                    debug!("## mcp: reusing cached authorization server metadata");
                    let issuer = metadata.issuer.clone();
                    (metadata, issuer)
                },
                None => {
                    let resolution = am.resolve_metadata().await?;
                    // A legacy fallback is `/authorize`, `/token` and `/register` guessed from
                    // the base URL because discovery answered nothing usable. Persisting a
                    // guess would turn a momentary discovery outage into a permanent one, so
                    // it is used for this connect only.
                    if resolution.source.is_discovered() {
                        write_cached_metadata(&paths.metadata, &resolution.metadata).await;
                    } else {
                        tracing::warn!(
                            "## mcp: server published no authorization server metadata; using fallback endpoints without caching them"
                        );
                    }
                    (resolution.metadata, None)
                },
            };

            // Hand the cached token to the SDK through its credential store instead of
            // `OAuthState::set_credentials`, which always rediscovers the authorization
            // server. Everything the store needs is already on disk.
            let received_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let store = InMemoryCredentialStore::new();
            store
                .save(
                    StoredCredentials::new(reg.client_id.clone(), Some(token), granted_scopes, Some(received_at))
                        .with_issuer(minted_under),
                )
                .await?;
            am.set_credential_store(store);
            am.set_metadata(metadata);

            // Metadata is already set, so this adopts the stored token and client id
            // without discovery. `false` means the SDK refused the credentials rather
            // than adopting them, which leaves an unauthorized manager: treat it as a
            // cache miss and re-authorize instead of connecting without a token.
            if !am.initialize_from_store().await? {
                tracing::warn!("## mcp: cached OAuth credentials were rejected, re-running OAuth flow");
                remove_cached_oauth_state(paths).await;
                return authorize_from_scratch(url, paths, scopes, oauth_config, messenger, os).await;
            }

            debug!("## mcp: credentials set with cache");

            // The stored credentials describe a public client (no secret). For confidential
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
            authorize_from_scratch(url, paths, scopes, oauth_config, messenger, os).await
        },
    }
}

/// Runs the full interactive OAuth flow and persists the resulting client
/// registration and token.
async fn authorize_from_scratch(
    url: Url,
    paths: &OAuthCachePaths,
    scopes: &[String],
    oauth_config: &Option<crate::cli::chat::tools::custom_tool::OAuthConfig>,
    messenger: &dyn Messenger,
    os: &Os,
) -> Result<AuthorizationManager, OauthUtilError> {
    debug!("## mcp: constructing auth manager from scratch");
    let oauth_state = OAuthState::new(url, Some(oauth_discovery_client()?)).await?;
    let (am, redirect_uri) = get_auth_manager_impl(oauth_state, scopes, oauth_config, messenger, os).await?;

    // Client registration is done in [start_authorization]
    // If we have gotten past that point that means we have the info to persist the
    // registration on disk.
    let (client_id, credentials) = am.get_credentials().await?;
    let reg = Registration {
        client_id,
        client_secret: None,
        scopes: scopes.to_vec(),
        redirect_uri,
    };
    let reg_as_str = serde_json::to_string_pretty(&reg)?;
    let reg_parent_path = paths.registration.parent().ok_or(OauthUtilError::MalformDirectory)?;
    tokio::fs::create_dir_all(reg_parent_path).await?;
    tokio::fs::write(&paths.registration, &reg_as_str).await?;

    let credentials = credentials.ok_or(OauthUtilError::MissingCredentials)?;

    let cred_parent_path = paths.token.parent().ok_or(OauthUtilError::MalformDirectory)?;
    tokio::fs::create_dir_all(cred_parent_path).await?;
    let reg_as_str = serde_json::to_string_pretty(&credentials)?;
    write_credentials_securely(&paths.token, &reg_as_str).await?;

    Ok(am)
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
    mut oauth_state: OAuthState,
    scopes: &[String],
    oauth_config: &Option<crate::cli::chat::tools::custom_tool::OAuthConfig>,
    messenger: &dyn Messenger,
    _os: &Os,
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
    let user_client_metadata_url = oauth_config.as_ref().and_then(|cfg| cfg.client_metadata_url.as_deref());
    start_authorization(
        &mut oauth_state,
        scopes_as_slice,
        &redirect_uri,
        user_client_id,
        user_client_secret,
        user_client_metadata_url,
    )
    .await?;

    let auth_url = oauth_state.get_authorization_url().await?;
    _ = messenger.send_oauth_link(auth_url).await;

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
    user_client_metadata_url: Option<&str>,
) -> Result<(), OauthUtilError> {
    // DO NOT CHANGE THIS
    // This string has significance as it is used for remote servers to identify us
    const DEFAULT_CLIENT_ID: &str = "Q DEV CLI";
    // The client_name sent during Dynamic Client Registration (DCR).
    // Some servers (e.g. Figma) use this to identify the application.
    const DEFAULT_CLIENT_NAME: &str = "kiro";

    // Client identity priority per the MCP authorization spec: a pre-registered client_id,
    // then a Client ID Metadata Document (SEP-991), then Dynamic Client Registration.
    // A metadata document names a public client and the SDK rejects a secret alongside it, so a
    // configured secret suppresses the document rather than being silently dropped.
    if user_client_id.is_none() && user_client_secret.is_some() && user_client_metadata_url.is_some() {
        warn!(
            "ignoring clientMetadataUrl because clientSecret is set: a client id metadata document identifies a public client"
        );
    }
    let client_metadata_url =
        user_client_metadata_url.filter(|_| user_client_id.is_none() && user_client_secret.is_none());
    let client_id = user_client_id.unwrap_or(DEFAULT_CLIENT_ID);

    let stub_cred = get_stub_credentials()?;
    oauth_state.set_credentials(client_id, stub_cred).await?;

    if let Some(client_metadata_url) = client_metadata_url
        && try_client_id_metadata_authorization(
            oauth_state,
            scopes,
            redirect_uri,
            client_metadata_url,
            DEFAULT_CLIENT_NAME,
        )
        .await?
    {
        return Ok(());
    }

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

/// Attempt authorization through [AuthorizationSession::new], the only SDK entry point that
/// resolves a Client ID Metadata Document. The SDK downgrades to Dynamic Client Registration
/// itself when the server does not advertise `client_id_metadata_document_supported`.
///
/// Returns `false` (leaving `oauth_state` authorized and untouched) when the attempt fails, so
/// the caller can retry on its own registration path rather than failing the whole flow.
async fn try_client_id_metadata_authorization(
    oauth_state: &mut OAuthState,
    scopes: &[&str],
    redirect_uri: &str,
    client_metadata_url: &str,
    client_name: &str,
) -> Result<bool, OauthUtilError> {
    let OAuthState::Authorized(auth_manager) = oauth_state else {
        return Ok(false);
    };

    // The session constructor consumes the manager, and discovered metadata lives inside it.
    let mut discovered = AuthorizationManager::new("http://localhost").await?;
    std::mem::swap(auth_manager, &mut discovered);

    let request = AuthorizationRequest::new(redirect_uri)
        .with_scopes(scopes.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
        .with_client_name(client_name)
        .with_client_metadata_url(client_metadata_url);

    match AuthorizationSession::new(discovered, request).await {
        Ok(session) => {
            *oauth_state = OAuthState::Session(session);
            Ok(true)
        },
        Err((returned, e)) => {
            warn!(?e, "client id metadata authorization failed, falling back");
            *oauth_state = OAuthState::Authorized(returned);
            Ok(false)
        },
    }
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
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use super::*;

    // ─── Client ID Metadata Documents (SEP-991) ──────────────────────────

    const CLIENT_METADATA_URL: &str = "https://kiro.dev/client-metadata.json";

    /// A minimal authorization server that publishes RFC 8414 metadata and counts Dynamic Client
    /// Registration requests, so a test can assert registration was skipped entirely.
    async fn spawn_mock_authorization_server(advertise_cimd: bool) -> (String, Arc<AtomicUsize>, CancellationToken) {
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
        let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let registrations = Arc::new(AtomicUsize::new(0));
        let cancellation_token = CancellationToken::new();

        let metadata = serde_json::json!({
            "issuer": base_url,
            "authorization_endpoint": format!("{base_url}/authorize"),
            "token_endpoint": format!("{base_url}/token"),
            "registration_endpoint": format!("{base_url}/register"),
            "response_types_supported": ["code"],
            "code_challenge_methods_supported": ["S256"],
            "client_id_metadata_document_supported": advertise_cimd,
        })
        .to_string();

        let counter = Arc::clone(&registrations);
        let shutdown = cancellation_token.clone();
        tokio::spawn(async move {
            loop {
                let stream = tokio::select! {
                    _ = shutdown.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => stream,
                        Err(_) => break,
                    },
                };
                let metadata = metadata.clone();
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                        let metadata = metadata.clone();
                        let counter = Arc::clone(&counter);
                        async move {
                            let path = req.uri().path().to_string();
                            if path.starts_with("/.well-known/oauth-authorization-server") {
                                Response::builder()
                                    .header("content-type", "application/json")
                                    .body(Full::new(Bytes::from(metadata)))
                            } else if path == "/register" {
                                counter.fetch_add(1, Ordering::SeqCst);
                                Response::builder()
                                    .header("content-type", "application/json")
                                    .body(Full::new(Bytes::from(
                                        r#"{"client_id":"dcr-issued","redirect_uris":[]}"#,
                                    )))
                            } else {
                                Response::builder().status(404).body(Full::new(Bytes::new()))
                            }
                        }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });

        (base_url, registrations, cancellation_token)
    }

    /// Run `start_authorization` against a mock server and report the client identifier it put on
    /// the authorize URL alongside the number of registration requests the server saw.
    async fn authorize_with(
        advertise_cimd: bool,
        user_client_id: Option<&str>,
        user_client_secret: Option<&str>,
        user_client_metadata_url: Option<&str>,
    ) -> (String, usize) {
        let (base_url, registrations, cancellation_token) = spawn_mock_authorization_server(advertise_cimd).await;
        let mut oauth_state = OAuthState::new(base_url, Some(oauth_discovery_client().unwrap()))
            .await
            .unwrap();

        start_authorization(
            &mut oauth_state,
            &["openid"],
            "http://127.0.0.1:7778/oauth/callback",
            user_client_id,
            user_client_secret,
            user_client_metadata_url,
        )
        .await
        .unwrap();

        let auth_url = Url::parse(&oauth_state.get_authorization_url().await.unwrap()).unwrap();
        cancellation_token.cancel();

        let client_id = auth_url
            .query_pairs()
            .find(|(key, _)| key == "client_id")
            .map(|(_, value)| value.into_owned())
            .expect("authorize url carries a client_id");
        (client_id, registrations.load(Ordering::SeqCst))
    }

    #[tokio::test]
    async fn test_client_metadata_url_is_used_as_client_id_without_registration() {
        let (client_id, registrations) = authorize_with(true, None, None, Some(CLIENT_METADATA_URL)).await;
        assert_eq!(client_id, CLIENT_METADATA_URL);
        assert_eq!(registrations, 0, "a metadata document must not trigger registration");
    }

    #[tokio::test]
    async fn test_without_client_metadata_url_registration_still_runs() {
        let (client_id, registrations) = authorize_with(true, None, None, None).await;
        assert_eq!(client_id, "dcr-issued");
        assert_eq!(registrations, 1);
    }

    #[tokio::test]
    async fn test_client_id_outranks_client_metadata_url() {
        let (client_id, registrations) =
            authorize_with(true, Some("preregistered"), None, Some(CLIENT_METADATA_URL)).await;
        assert_eq!(client_id, "preregistered");
        assert_eq!(registrations, 0);
    }

    #[tokio::test]
    async fn test_client_metadata_url_falls_back_to_registration_when_unsupported() {
        let (client_id, registrations) = authorize_with(false, None, None, Some(CLIENT_METADATA_URL)).await;
        assert_eq!(client_id, "dcr-issued");
        assert_eq!(registrations, 1);
    }

    #[tokio::test]
    async fn test_non_https_client_metadata_url_falls_back_to_registration() {
        let (client_id, registrations) =
            authorize_with(true, None, None, Some("http://kiro.dev/client-metadata.json")).await;
        assert_eq!(client_id, "dcr-issued");
        assert_eq!(registrations, 1, "a rejected metadata document must still register");
    }

    #[tokio::test]
    async fn test_client_secret_suppresses_client_metadata_url() {
        let (client_id, registrations) = authorize_with(true, None, Some("shhh"), Some(CLIENT_METADATA_URL)).await;
        assert_eq!(
            client_id, "dcr-issued",
            "a secret must not ride on a public client identity"
        );
        assert_eq!(registrations, 1);
    }

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

    #[tokio::test]
    async fn test_remove_cached_oauth_state_deletes_all_three_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let paths = OAuthCachePaths::new(dir.path(), "abc123");
        for path in [&paths.token, &paths.registration, &paths.metadata] {
            tokio::fs::write(path, "{}").await.unwrap();
        }

        remove_cached_oauth_state(&paths).await;

        assert!(!paths.token.exists());
        assert!(!paths.registration.exists());
        assert!(
            !paths.metadata.exists(),
            "metadata must not survive the credentials it describes"
        );

        // A second pass over the now-missing files must not panic or error.
        remove_cached_oauth_state(&paths).await;
    }
}
