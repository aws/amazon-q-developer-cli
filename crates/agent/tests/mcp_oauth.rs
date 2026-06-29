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
    ResolvedGlobalPrompt,
};
use agent::mcp::{
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
fn registry_resolved_remote(server_name: &str, url: String) -> McpServerConfig {
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
        oauth_scopes: vec![],
        oauth: None,
        disabled: false,
        disabled_tools: vec![],
        force_auth: false,
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
fn spawn_oauth_browser(
    mut handle: McpManagerHandle,
    oauth_request_count: Arc<AtomicUsize>,
    max_drives: Option<usize>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut driven = 0usize;
        loop {
            match handle.recv().await {
                Ok(McpServerEvent::OauthRequest { oauth_url, .. }) => {
                    oauth_request_count.fetch_add(1, Ordering::SeqCst);
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
async fn launch_and_wait(
    handle: &mut McpManagerHandle,
    server_name: &str,
    config: McpServerConfig,
    timeout: Duration,
) -> Result<(), String> {
    let rx = handle
        .launch_server(server_name.to_string(), config)
        .await
        .map_err(|e| format!("launch dispatch failed: {e}"))?;
    tokio::time::timeout(timeout, rx)
        .await
        .map_err(|_elapsed| "timed out waiting for MCP server to initialize".to_string())?
        .map_err(|_recv| "launch result channel dropped".to_string())?
        .map_err(|e| format!("server failed to initialize: {e}"))
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
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None);

    let config = registry_resolved_remote("oauth-mcp", server.url());

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
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None);

    let config = registry_resolved_remote("refresh-mcp", server.url());
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
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None);

    let config = registry_resolved_remote("reauth-mcp", server.url());
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
    let _browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), Some(1));

    let config = registry_resolved_remote("badrefresh-mcp", server.url());
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
    let browser = spawn_oauth_browser(handle.clone(), oauth_requests.clone(), None);

    let config = registry_resolved_remote("cached-mcp", server.url());

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
