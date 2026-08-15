//! End-to-end OAuth tests for **registry-resolved** MCP servers.
//!
//! These exercise the real OAuth machinery in `agent::mcp` (the
//! `HttpServiceBuilder` connection strategy, `AuthClientWrapper` token refresh,
//! and the `decorate_with_auth_retry!` call path) against a live
//! `mock-mcp-server` subprocess running in its `--oauth` mode. The mock
//! implements the full handshake — discovery, dynamic client registration,
//! authorize, and a token endpoint that supports both `authorization_code` and
//! `refresh_token` grants — and validates bearer tokens on `/mcp`.
//!
//! Every server here is **registry-resolved**: it starts life as a
//! `McpServerConfig::Registry` placeholder in a `LoadedAgentConfig` and is
//! turned into a concrete `Remote` config by an `McpRegistry::apply` pass
//! (the same hook Kiro's real registry uses), mirroring how a server sourced
//! from a registry reaches the OAuth code path.
//!
//! Coverage:
//!   - Test A: initial OAuth flow against a registry-resolved server.
//!   - Test B: token expiry mid-session triggers a silent **refresh** (refresh token present).
//!   - Test C: token expiry with **no refresh token** falls back to re-authorization.
//!   - Test D: a **failed refresh** surfaces a clear user-facing error instead of hanging.
//!   - Test E: a persisted token is reused on relaunch instead of prompting again.
//!   - Test F: the authorization request's `resource` indicator is the protected-resource-metadata
//!     resource, not the server base URL.
//!   - Test G: the authorization request omits `scope` entirely when no scopes are configured.
//!
//! The test process plays the role of the user's browser: when the agent emits
//! an `OauthRequest`, a background task fetches the authorization URL, which
//! 302-redirects to the CLI's loopback listener with the auth code, completing
//! the flow without any human interaction.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::time::Duration;

use agent::agent_config::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
    McpServerConfig,
    RegistryMcpServerConfig,
    RemoteMcpServerConfig,
};
use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
    McpServerConfigSource,
    ResolvedGlobalPrompt,
};
use agent::mcp::{
    LaunchOutcome,
    McpManager,
    McpManagerHandle,
    McpRegistry,
    McpServerEvent,
};
use mock_mcp_server::{
    MockMcpServerBuilder,
    MockResponse,
    ToolDef,
    prebuild_bin,
};
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

const TOOL_NAME: &str = "echo";

/// An `McpRegistry` that resolves a single named `Registry` placeholder into a
/// concrete `Remote` config. Stands in for Kiro's real registry: the agent
/// crate only cares that `apply` turns the placeholder into something
/// launchable.
#[derive(Debug, Clone)]
struct OAuthResolvingRegistry {
    server_name: String,
    resolved: RemoteMcpServerConfig,
}

impl McpRegistry for OAuthResolvingRegistry {
    fn apply(&self, agent_config: &mut LoadedAgentConfig) {
        let updates: Vec<(String, McpServerConfig)> = agent_config
            .config()
            .mcp_servers()
            .iter()
            .filter(|(name, cfg)| name.as_str() == self.server_name && matches!(cfg, McpServerConfig::Registry(_)))
            .map(|(name, _)| (name.clone(), McpServerConfig::Remote(self.resolved.clone())))
            .collect();
        agent_config.config_mut().insert_mcp_servers(updates);
    }
}

/// Build a `LoadedAgentConfig` whose only MCP server is a `Registry`
/// placeholder, then run the registry resolution pass and return the resulting
/// concrete config. Panics unless the placeholder was resolved into a `Remote`
/// variant — that assertion is itself part of the "registry-resolved" coverage.
///
/// `oauth_scopes` and `force_auth` are threaded onto the resolved `Remote`
/// config so tests can request a scope set or skip the unauthenticated probe.
fn registry_resolved_remote(
    server_name: &str,
    url: String,
    oauth_scopes: Vec<String>,
    force_auth: bool,
) -> McpServerConfig {
    let mut inner = AgentConfigV2025_08_22 {
        name: "oauth-e2e".to_string(),
        ..Default::default()
    };
    inner.mcp_servers.insert(
        server_name.to_string(),
        McpServerConfig::Registry(RegistryMcpServerConfig {
            server_type: "registry".to_string(),
            env: None,
            headers: None,
            timeout: None,
            oauth_scopes: vec![],
            oauth: None,
        }),
    );

    let mut config = LoadedAgentConfig::new(
        AgentConfig::V2025_08_22(inner),
        ConfigSource::Ephemeral,
        ResolvedGlobalPrompt::None,
    );

    let resolved = RemoteMcpServerConfig {
        url,
        headers: HashMap::new(),
        timeout_ms: 30_000,
        oauth_scopes,
        oauth: None,
        disabled: false,
        disabled_tools: vec![],
        force_auth,
    };
    OAuthResolvingRegistry {
        server_name: server_name.to_string(),
        resolved,
    }
    .apply(&mut config);

    let resolved = config
        .config()
        .mcp_servers()
        .get(server_name)
        .cloned()
        .expect("registry should have resolved the placeholder");
    assert!(
        matches!(resolved, McpServerConfig::Remote(_)),
        "registry must resolve the placeholder into a Remote config; got {resolved:?}"
    );
    resolved
}

/// Spawn a background task that plays the user's browser: for every
/// `OauthRequest` event, it fetches the authorization URL (which 302-redirects
/// to the CLI loopback listener, delivering the auth code). Increments
/// `oauth_request_count` for every request seen so tests can distinguish a
/// silent refresh (no new request) from a re-authorization (a new request).
///
/// `max_drives` caps how many authorization URLs are actually fetched. `None`
/// drives every request; `Some(n)` drives only the first `n` (used to prevent
/// a background re-auth from racing a test that asserts a failure).
///
/// When `recorder` is `Some`, each authorization URL is captured (before it is
/// driven, so a completed handshake guarantees the capture is present) for
/// tests that inspect the `/authorize` query parameters.
fn spawn_oauth_browser(
    mut handle: McpManagerHandle,
    oauth_request_count: Arc<AtomicUsize>,
    max_drives: Option<usize>,
    recorder: Option<Arc<std::sync::Mutex<Vec<String>>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut driven = 0usize;
        loop {
            match handle.recv().await {
                Ok(McpServerEvent::OauthRequest { oauth_url, .. }) => {
                    oauth_request_count.fetch_add(1, Ordering::SeqCst);
                    if let Some(recorder) = &recorder {
                        recorder.lock().unwrap().push(oauth_url.clone());
                    }
                    let should_drive = max_drives.is_none_or(|max| driven < max);
                    if should_drive {
                        driven += 1;
                        // Follows the 302 to the loopback redirect URI, delivering ?code&state.
                        let _ = client.get(&oauth_url).send().await;
                    }
                },
                Ok(_) => {},
                Err(RecvError::Lagged(_)) => {},
                Err(RecvError::Closed) => break,
            }
        }
    })
}

/// Launch a server and wait for it to finish initializing (which includes the
/// full OAuth handshake). Returns an error string on failure or timeout.
///
/// A launch that stops at an authorization prompt reports back before the grant
/// lands, so keep waiting on the server's own events for the real outcome.
async fn launch_and_wait(
    handle: &mut McpManagerHandle,
    server_name: &str,
    config: McpServerConfig,
    timeout: Duration,
) -> Result<(), String> {
    let mut events = handle.clone();
    let rx = handle
        .launch_server(server_name.to_string(), config, McpServerConfigSource::Registry)
        .await
        .map_err(|e| format!("launch dispatch failed: {e}"))?;
    let outcome = tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_elapsed| "timed out waiting for MCP server to initialize".to_string())?
        .map_err(|_recv| "launch result channel dropped".to_string())?
        .map_err(|e| format!("server failed to initialize: {e}"))?;
    if outcome == LaunchOutcome::Initialized {
        return Ok(());
    }
    tokio::time::timeout(timeout, async {
        loop {
            match events.recv().await {
                Ok(McpServerEvent::Initialized { server_name: n, .. }) if n == server_name => return Ok(()),
                Ok(McpServerEvent::InitializeError {
                    server_name: n, error, ..
                }) if n == server_name => {
                    return Err(format!("server failed to initialize: {error}"));
                },
                Ok(_) | Err(RecvError::Lagged(_)) => {},
                Err(RecvError::Closed) => return Err("event channel closed".to_string()),
            }
        }
    })
    .await
    .map_err(|_elapsed| "timed out waiting for MCP server to initialize".to_string())?
}

/// Invoke the mock's `echo` tool once. `Ok(())` on success; `Err(message)` with
/// the surfaced error string on failure.
async fn call_tool_once(handle: &McpManagerHandle, server_name: &str) -> Result<(), String> {
    let rx = handle
        .execute_tool(server_name.to_string(), TOOL_NAME.to_string(), None)
        .await
        .map_err(|e| format!("execute_tool dispatch failed: {e}"))?;
    match rx.await.map_err(|e| format!("tool result channel dropped: {e}"))? {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Poll `call_tool_once` until it succeeds or the timeout elapses.
async fn call_tool_until_success(
    handle: &McpManagerHandle,
    server_name: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last_err = String::new();
    while tokio::time::Instant::now() < deadline {
        match call_tool_once(handle, server_name).await {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err(format!(
        "tool call never succeeded within {timeout:?}; last error: {last_err}"
    ))
}

fn echo_tool() -> ToolDef {
    ToolDef {
        name: TOOL_NAME.to_string(),
        description: "Echoes back a canned response".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    }
}

fn echo_response() -> MockResponse {
    MockResponse {
        tool: TOOL_NAME.to_string(),
        input_match: None,
        response: serde_json::json!({"echoed": true}),
    }
}

/// Invalidate every access token the mock has issued, simulating server-side
/// token expiry mid-session. The client's current bearer token will be rejected
/// (401) on its next request, forcing a refresh or re-authorization. Driving
/// expiry explicitly (rather than via a short TTL + sleep) keeps the tests
/// deterministic and avoids racing the initial handshake.
async fn expire_server_tokens(port: u16) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/control/expire-tokens");
    let resp = client.post(&url).send().await.expect("expire-tokens request failed");
    assert!(resp.status().is_success(), "expire-tokens returned {}", resp.status());
}

/// Test A — Initial OAuth flow against a registry-resolved server.
///
/// A registry placeholder is resolved into a `Remote` config pointing at an
/// OAuth-protected mock. Launching it must complete the full handshake
/// (unauthenticated probe → 401 → discovery → DCR → authorize → token), expose
/// the server's tools, and let a tool call succeed. Exactly one OAuth request
/// is emitted during initialization.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_oauth_against_registry_resolved_server() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None, None);

    let config = registry_resolved_remote("oauth-mcp", server.url(), vec![], false);

    launch_and_wait(&mut handle, "oauth-mcp", config, Duration::from_secs(30))
        .await
        .expect("registry-resolved server should complete OAuth and initialize");

    // The server's tools are available after the authenticated handshake.
    let specs = handle
        .get_tool_specs("oauth-mcp".to_string())
        .await
        .expect("should fetch tool specs after OAuth");
    assert!(
        specs.iter().any(|s| s.name.contains(TOOL_NAME)),
        "expected the echo tool to be available; got {:?}",
        specs.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    // And a tool call succeeds over the authenticated transport.
    call_tool_once(&handle, "oauth-mcp")
        .await
        .expect("tool call should succeed after initial OAuth");

    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        1,
        "exactly one OAuth authorization request should occur during initial sign-in"
    );

    handle.shutdown().await;
}

/// Test B — Token expiry mid-session triggers a silent refresh.
///
/// The mock issues access tokens *with* refresh tokens. When the token is
/// invalidated mid-session (simulating expiry), the next tool call must
/// transparently refresh and succeed — without emitting a second OAuth request
/// (a refresh is silent; only a full re-authorization would prompt the user
/// again).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_expiry_triggers_silent_refresh_with_refresh_token() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None, None);

    let config = registry_resolved_remote("refresh-mcp", server.url(), vec![], false);
    launch_and_wait(&mut handle, "refresh-mcp", config, Duration::from_secs(30))
        .await
        .expect("server should initialize");

    // First call succeeds with the freshly-issued token.
    call_tool_once(&handle, "refresh-mcp")
        .await
        .expect("first tool call should succeed");

    // Invalidate the access token mid-session.
    expire_server_tokens(server.port()).await;

    // The next call must succeed by refreshing the now-rejected token.
    call_tool_until_success(&handle, "refresh-mcp", Duration::from_secs(15))
        .await
        .expect("tool call after expiry should succeed via token refresh");

    // A refresh is silent: no new authorization request should have been emitted.
    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        1,
        "refresh must not trigger a new OAuth authorization request"
    );

    handle.shutdown().await;
}

/// Test C — Token expiry with no refresh token falls back to re-authorization.
///
/// The mock issues access tokens but *no* refresh token. When the token is
/// invalidated mid-session, the client cannot refresh, so it must fall back to
/// a full re-authorization (a new OAuth request), then recover. We assert both
/// that a second OAuth request is emitted and that a tool call eventually
/// succeeds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_expiry_without_refresh_token_reauthorizes() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth_no_refresh_token() // force the re-authorization path
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None, None);

    let config = registry_resolved_remote("reauth-mcp", server.url(), vec![], false);
    launch_and_wait(&mut handle, "reauth-mcp", config, Duration::from_secs(30))
        .await
        .expect("server should initialize");

    call_tool_once(&handle, "reauth-mcp")
        .await
        .expect("first tool call should succeed");
    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        1,
        "only the initial sign-in should have happened so far"
    );

    // Invalidate the access token mid-session (no refresh token is available).
    expire_server_tokens(server.port()).await;

    // Recovery is only possible via re-authorization, which the browser driver
    // completes. A successful call therefore implies a re-auth occurred.
    call_tool_until_success(&handle, "reauth-mcp", Duration::from_secs(20))
        .await
        .expect("tool call should eventually succeed after re-authorization");

    assert!(
        oauth_requests.load(Ordering::SeqCst) >= 2,
        "expiry without a refresh token must trigger a new OAuth authorization request; saw {}",
        oauth_requests.load(Ordering::SeqCst)
    );

    handle.shutdown().await;
}

/// Test D — A failed refresh surfaces a clear user-facing error, not a hang.
///
/// The mock rejects every refresh attempt (HTTP 400). The browser driver only
/// completes the *initial* handshake (`max_drives = 1`), so re-authorization
/// cannot silently rescue the session. When the token is invalidated and a tool
/// call is made, it must return promptly with the `MCP_AUTH_REFRESH_FAILED`
/// sentinel rather than hanging indefinitely.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_refresh_surfaces_clear_error() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth_refresh_fails() // refresh is rejected with HTTP 400
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    // Only drive the initial handshake so a background re-auth can't rescue the failing call.
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), Some(1), None);

    let config = registry_resolved_remote("badrefresh-mcp", server.url(), vec![], false);
    launch_and_wait(&mut handle, "badrefresh-mcp", config, Duration::from_secs(30))
        .await
        .expect("server should initialize");

    call_tool_once(&handle, "badrefresh-mcp")
        .await
        .expect("first tool call should succeed");

    // Invalidate the token; refresh will fail and re-auth is not driven.
    expire_server_tokens(server.port()).await;

    // The call must return an error promptly (no silent hang). Bound it with a
    // timeout: exceeding it means the call hung, which is itself a failure.
    let result = tokio::time::timeout(Duration::from_secs(15), call_tool_once(&handle, "badrefresh-mcp"))
        .await
        .expect("tool call hung after a failed refresh instead of returning an error");

    let err = result.expect_err("tool call should fail when the token cannot be refreshed");
    assert!(
        err.contains("MCP_AUTH"),
        "failed refresh should surface a clear auth error (MCP_AUTH_REFRESH_FAILED / \
         MCP_AUTH_REAUTH_FAILED); got: {err}"
    );

    handle.shutdown().await;
}

/// Test E — A persisted token is reused on relaunch instead of prompting again.
///
/// Once the initial OAuth sign-in writes a token to the shared credential cache,
/// tearing the server down and relaunching it against the *same* cache must load
/// the persisted token. Crucially, **no OAuth flow runs on the relaunch**: there
/// is no browser and no redirect loopback. When the authenticated connection path
/// (`get_auth_manager`) finds cached credentials on disk it builds the
/// `AuthorizationManager` directly and never reaches the interactive
/// `get_auth_manager_impl` loopback/browser flow.
///
/// To prove that, this test **tears the browser driver down before the relaunch**:
/// if the relaunch were to (incorrectly) start a fresh OAuth handshake, it would
/// block on the redirect loopback with nothing to complete it and time out. A
/// successful relaunch therefore demonstrates the cached token was reused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cached_token_is_reused_on_relaunch() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    // A single credential cache shared across both launches.
    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None, None);

    let config = registry_resolved_remote("cached-mcp", server.url(), vec![], false);

    // First launch performs the full OAuth handshake and persists the token.
    launch_and_wait(&mut handle, "cached-mcp", config.clone(), Duration::from_secs(30))
        .await
        .expect("initial launch should complete OAuth and initialize");
    call_tool_once(&handle, "cached-mcp")
        .await
        .expect("tool call should succeed after initial sign-in");
    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        1,
        "exactly one authorization request during the initial sign-in"
    );

    // Tear the browser down so nothing can complete an OAuth flow from here on.
    // The relaunch below must therefore succeed purely by reusing the cached
    // token — if it tried to re-authorize it would hang on the loopback and the
    // `launch_and_wait` timeout would fail the test. Awaiting the aborted handle
    // guarantees the driver is fully stopped before we relaunch.
    browser.abort();
    let _ = browser.await;

    // Tear the server down (the token stays on disk), then relaunch against the
    // same credential cache.
    handle
        .shutdown_server("cached-mcp".to_string())
        .await
        .expect("shutdown_server should succeed");

    launch_and_wait(&mut handle, "cached-mcp", config, Duration::from_secs(30))
        .await
        .expect("relaunch should initialize by reusing the cached token (no OAuth/browser involved)");
    call_tool_once(&handle, "cached-mcp")
        .await
        .expect("tool call should succeed after relaunch");

    // Belt-and-suspenders: with the browser gone, no authorization could have been
    // completed during the relaunch — the count is still the single initial sign-in.
    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        1,
        "relaunch must reuse the cached token rather than re-authorize; saw {} request(s)",
        oauth_requests.load(Ordering::SeqCst)
    );

    handle.shutdown().await;
}

/// Test F — Regression guard: the OAuth authorization request must carry the RFC
/// 9728 protected-resource-metadata `resource`, not the MCP server base URL (rmcp
/// 2.0 derived it from the base URL, which Entra ID v2 rejects; rmcp 3.0 honors
/// the PRM value). The mock declares its PRM `resource` as the server origin — a
/// valid RFC 8707 identifier for the `/mcp` base URL but a distinct string — so a
/// client honoring the PRM emits a `resource` different from the base URL.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authorize_resource_indicator_uses_protected_resource_metadata_not_base_url() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .oauth_resource_origin_only()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    // The mock declares its PRM `resource` as the origin — the base URL with the
    // `/mcp` path stripped — so deriving the expectation from `base_url` keeps the
    // two in sync and makes explicit that they differ only by that path.
    let base_url = server.url(); // http://127.0.0.1:{port}/mcp
    let expected_resource = base_url
        .strip_suffix("/mcp")
        .expect("mock base URL ends with /mcp")
        .to_string();

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let captured_urls = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let _browser = spawn_oauth_browser(
        handle.clone(),
        oauth_requests.clone(),
        None,
        Some(captured_urls.clone()),
    );

    // `force_auth` skips the unauthenticated probe and goes straight to OAuth so a
    // single authorization request is emitted; the non-empty scope mirrors the
    // customer's `oauthScopes` (Entra v2 requires `scope` on the authorize request).
    let config = registry_resolved_remote("entra-mcp", base_url.clone(), vec!["openid".to_string()], true);

    launch_and_wait(&mut handle, "entra-mcp", config, Duration::from_secs(30))
        .await
        .expect("server should complete OAuth and initialize");

    let urls = captured_urls.lock().unwrap().clone();
    assert_eq!(
        urls.len(),
        1,
        "exactly one authorization request expected; got {urls:?}"
    );
    let parsed = reqwest::Url::parse(&urls[0]).expect("authorization URL should parse");
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

    let resource = params
        .get("resource")
        .expect("authorization request must include a resource indicator");

    // Regression signal: `resource` is the protected-resource-metadata identifier
    // (the origin), which differs from the base URL. rmcp 2.0 derived it from the
    // base URL and Microsoft Entra ID v2 rejected that.
    assert_eq!(
        resource, &expected_resource,
        "resource must be the protected-resource-metadata identifier `{expected_resource}`; got `{resource}`"
    );

    // Entra v2 requires `scope`; the requested scope must be carried through.
    assert!(
        params.get("scope").is_some_and(|s| s.contains("openid")),
        "authorization request must carry the requested scope; params: {params:?}"
    );

    handle.shutdown().await;
}

/// Test G — Regression guard: when no OAuth scopes are configured, the
/// authorization request must not carry a `scope` parameter at all. Atlassian's
/// `authv2` endpoint fails its consent flow with a 404 when `scope` is present,
/// so an injected default scope set breaks authentication against it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authorize_omits_scope_param_when_no_scopes_configured() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let base_url = server.url();

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let captured_urls = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let _browser = spawn_oauth_browser(
        handle.clone(),
        oauth_requests.clone(),
        None,
        Some(captured_urls.clone()),
    );

    // Empty scopes mirror a registry server with no oauthScopes declared anywhere.
    let config = registry_resolved_remote("atlassian-mcp", base_url.clone(), Vec::new(), true);

    launch_and_wait(&mut handle, "atlassian-mcp", config, Duration::from_secs(30))
        .await
        .expect("server should complete OAuth and initialize");

    let urls = captured_urls.lock().unwrap().clone();
    assert_eq!(
        urls.len(),
        1,
        "exactly one authorization request expected; got {urls:?}"
    );
    let parsed = reqwest::Url::parse(&urls[0]).expect("authorization URL should parse");
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();

    assert!(
        !params.contains_key("scope"),
        "authorization request must omit `scope` when no scopes are configured; params: {params:?}"
    );

    handle.shutdown().await;
}

/// How many OAuth metadata discovery requests the mock has served so far.
async fn discovery_count(port: u16) -> u64 {
    let url = format!("http://127.0.0.1:{port}/control/discovery-count");
    let body: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("discovery-count request failed")
        .json()
        .await
        .expect("discovery-count response was not JSON");
    body["discoveries"]
        .as_u64()
        .expect("discovery count should be a number")
}

/// Take the mock's metadata discovery endpoints offline: every well-known
/// document answers 404 from now on, the way a gateway or WAF hides them from a
/// client that is otherwise perfectly able to connect. A 5xx would not do — the
/// SDK treats server errors as retryable and fails discovery outright instead of
/// synthesizing the legacy endpoints this test is about.
async fn break_discovery(port: u16) {
    let url = format!("http://127.0.0.1:{port}/control/break-discovery");
    let resp = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .expect("break-discovery request failed");
    assert!(resp.status().is_success(), "break-discovery returned {}", resp.status());
}

/// The cached authorization server metadata file in `cred_dir`, if one was written.
fn cached_metadata_file(cred_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(cred_dir)
        .expect("credential cache dir should be readable")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.to_string_lossy().ends_with(".metadata.json"))
        .collect();
    assert!(
        found.len() <= 1,
        "expected at most one cached metadata file; got {found:?}"
    );
    found.pop()
}

/// Complete the initial OAuth sign-in for `server_name`, then stop the browser
/// driver. Every later launch in the test must therefore succeed from the on-disk
/// cache alone — a fresh flow would block on the loopback redirect with nothing to
/// complete it.
async fn sign_in_then_stop_browser(handle: &mut McpManagerHandle, server_name: &str, config: McpServerConfig) {
    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None, None);

    launch_and_wait(handle, server_name, config, Duration::from_secs(30))
        .await
        .expect("initial launch should complete OAuth and initialize");
    call_tool_once(handle, server_name)
        .await
        .expect("tool call should succeed after initial sign-in");
    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        1,
        "exactly one authorization request during the initial sign-in"
    );

    browser.abort();
    let _ = browser.await;
}

/// Relaunch a server against the same credential cache and require a working tool
/// call, proving the connection was rebuilt from cached state.
async fn relaunch_from_cache(handle: &mut McpManagerHandle, server_name: &str, config: McpServerConfig) {
    handle
        .shutdown_server(server_name.to_string())
        .await
        .expect("shutdown_server should succeed");
    launch_and_wait(handle, server_name, config, Duration::from_secs(30))
        .await
        .expect("relaunch should initialize from the on-disk OAuth cache");
    call_tool_once(handle, server_name)
        .await
        .expect("tool call should succeed after relaunch");
}

/// Test G — Authorization server metadata is cached on a cached-token connect.
///
/// The interactive sign-in persists only the token and the client registration.
/// The first connect that *reuses* that token is the one which discovers the
/// authorization server outside the interactive flow, so that is where the
/// metadata cache is written — beside the registration, under the same key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authorization_server_metadata_is_cached_on_cached_token_connect() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();
    let config = registry_resolved_remote("meta-mcp", server.url(), vec![], false);

    sign_in_then_stop_browser(&mut handle, "meta-mcp", config.clone()).await;
    assert!(
        cached_metadata_file(cred_dir.path()).is_none(),
        "the interactive sign-in should not write a metadata cache"
    );

    relaunch_from_cache(&mut handle, "meta-mcp", config).await;

    let path = cached_metadata_file(cred_dir.path()).expect("cached-token connect should cache metadata");
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let key = name
        .strip_suffix(".metadata.json")
        .expect("cache file should use the .metadata.json suffix");
    assert!(
        cred_dir.path().join(format!("{key}.registration.json")).is_file(),
        "metadata should be cached beside the registration under the same key"
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).expect("cached metadata should be valid JSON");
    let port = server.port();
    assert_eq!(
        metadata["token_endpoint"].as_str(),
        Some(format!("http://127.0.0.1:{port}/oauth/token").as_str()),
        "cached metadata should carry the discovered token endpoint; got {metadata}"
    );
    assert_eq!(
        metadata["authorization_endpoint"].as_str(),
        Some(format!("http://127.0.0.1:{port}/oauth/authorize").as_str()),
        "cached metadata should carry the discovered authorization endpoint; got {metadata}"
    );
    assert_eq!(
        metadata["issuer"].as_str(),
        Some(format!("http://127.0.0.1:{port}").as_str()),
        "cached metadata should preserve the issuer; got {metadata}"
    );

    handle.shutdown().await;
}

/// Test H — A cached token plus cached metadata reaches no discovery endpoint.
///
/// This is the point of the cache: once metadata is on disk, reconnecting must not
/// re-run RFC 9728 / RFC 8414 discovery. The mock counts every metadata request it
/// serves, so the count must not move across the last launch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cached_metadata_skips_discovery_on_later_connect() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();
    let config = registry_resolved_remote("skip-mcp", server.url(), vec![], false);

    sign_in_then_stop_browser(&mut handle, "skip-mcp", config.clone()).await;

    // The first cached-token connect still discovers, and caches what it found.
    relaunch_from_cache(&mut handle, "skip-mcp", config.clone()).await;
    assert!(
        cached_metadata_file(cred_dir.path()).is_some(),
        "metadata should be cached before asserting discovery is skipped"
    );
    let discoveries_before = discovery_count(server.port()).await;
    assert!(
        discoveries_before > 0,
        "discovery must have happened at least once before the cache existed"
    );

    // With metadata on disk, this connect must not touch a discovery endpoint.
    relaunch_from_cache(&mut handle, "skip-mcp", config).await;
    assert_eq!(
        discovery_count(server.port()).await,
        discoveries_before,
        "a connect with cached token and metadata must not re-run discovery"
    );

    handle.shutdown().await;
}

/// Test I — Corrupt cached metadata falls back to discovery and self-heals.
///
/// The cache is an optimization, never a dependency: an unreadable file must not
/// fail the connection, and the fresh discovery it triggers must overwrite the bad
/// file so it cannot poison later connects.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_cached_metadata_falls_back_to_discovery() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();
    let config = registry_resolved_remote("corrupt-mcp", server.url(), vec![], false);

    sign_in_then_stop_browser(&mut handle, "corrupt-mcp", config.clone()).await;
    relaunch_from_cache(&mut handle, "corrupt-mcp", config.clone()).await;

    let path = cached_metadata_file(cred_dir.path()).expect("metadata should be cached by now");
    std::fs::write(&path, b"{ this is not metadata").unwrap();
    let discoveries_before = discovery_count(server.port()).await;

    // The relaunch must succeed despite the unreadable cache.
    relaunch_from_cache(&mut handle, "corrupt-mcp", config).await;

    assert!(
        discovery_count(server.port()).await > discoveries_before,
        "an unusable metadata cache must fall back to discovery"
    );
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap())
        .expect("fresh discovery should have overwritten the corrupt cache");
    assert!(
        metadata["token_endpoint"]
            .as_str()
            .is_some_and(|e| e.contains("/oauth/token")),
        "rewritten cache should carry rediscovered endpoints; got {metadata}"
    );

    handle.shutdown().await;
}

/// Test J — A discovery outage is used for the connect but never cached.
///
/// When no well-known document answers, the SDK synthesizes `/authorize`, `/token`
/// and `/register` from the base URL. Those endpoints are a guess about a server
/// that gave no evidence of supporting OAuth, so caching them would make a
/// momentary outage permanent: every later connect would read a valid-looking file
/// and skip discovery forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovery_outage_is_used_for_the_connect_but_not_cached() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();
    let config = registry_resolved_remote("outage-mcp", server.url(), vec![], false);

    sign_in_then_stop_browser(&mut handle, "outage-mcp", config.clone()).await;

    break_discovery(server.port()).await;
    let discoveries_before = discovery_count(server.port()).await;

    // Only discovery is down; the cached token still authorizes the connection.
    relaunch_from_cache(&mut handle, "outage-mcp", config).await;

    assert!(
        discovery_count(server.port()).await > discoveries_before,
        "the connect should have attempted discovery before falling back"
    );
    assert_eq!(
        cached_metadata_file(cred_dir.path()),
        None,
        "client-synthesized fallback endpoints must never be persisted"
    );

    handle.shutdown().await;
}

/// Test K — Wiping malformed credentials also drops the cached metadata.
///
/// The token, the registration and the metadata describe one authorization. The
/// fresh sign-in that follows a wipe rewrites the first two but not the third, so
/// leaving the metadata behind would carry the old authorization server's
/// endpoints into credentials minted after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wiping_malformed_credentials_also_drops_cached_metadata() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();
    let config = registry_resolved_remote("wipe-mcp", server.url(), vec![], false);

    // The browser stays up for the whole test: the wipe is followed by a fresh
    // sign-in, which needs the authorization redirect completed.
    let oauth_requests = Arc::new(AtomicUsize::new(0));
    let browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None, None);

    launch_and_wait(&mut handle, "wipe-mcp", config.clone(), Duration::from_secs(30))
        .await
        .expect("initial launch should complete OAuth and initialize");

    // The cached-token connect is what discovers and caches the metadata.
    relaunch_from_cache(&mut handle, "wipe-mcp", config.clone()).await;
    let metadata_path = cached_metadata_file(cred_dir.path()).expect("metadata should be cached by now");
    let key = metadata_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .strip_suffix(".metadata.json")
        .expect("cache file should use the .metadata.json suffix")
        .to_string();
    std::fs::write(cred_dir.path().join(format!("{key}.token.json")), b"{ not a token").unwrap();

    handle
        .shutdown_server("wipe-mcp".to_string())
        .await
        .expect("shutdown_server should succeed");
    launch_and_wait(&mut handle, "wipe-mcp", config, Duration::from_secs(30))
        .await
        .expect("a malformed token should re-authorize rather than fail the launch");
    call_tool_once(&handle, "wipe-mcp")
        .await
        .expect("tool call should succeed after re-authorization");
    assert_eq!(
        oauth_requests.load(Ordering::SeqCst),
        2,
        "the wipe should have forced a second authorization request"
    );

    assert_eq!(
        cached_metadata_file(cred_dir.path()),
        None,
        "wiping malformed credentials must drop the metadata cache with them"
    );

    browser.abort();
    let _ = browser.await;
    handle.shutdown().await;
}
