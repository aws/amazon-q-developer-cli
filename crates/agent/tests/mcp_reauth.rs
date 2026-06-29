//! Agent-level tests for forced MCP re-authentication ("shadow" servers).
//!
//! Forcing auth on an already-loaded remote server does **not** unload it.
//! Instead the agent launches a hidden "shadow" server that runs the OAuth flow
//! alongside the still-running original, promoting it only once it initializes.
//! On abort (or when the shadow can't finish) the original is left untouched.
//!
//! These run the real `agent::mcp` machinery against a live `mock-mcp-server`
//! subprocess in `--oauth` mode. The harness attaches a background "browser"
//! that auto-completes the OAuth requests we want driven (see
//! `with_oauth_autodrive`). To park a shadow mid-auth we use a server that issues
//! no refresh token and then invalidate the access token, so a forced re-auth is
//! pushed onto a fresh authorization request that we deliberately leave undriven.

mod common;

use std::collections::HashMap;
use std::time::Duration;

use agent::agent_config::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
    LocalMcpServerConfig,
    McpServerConfig,
    RegistryMcpServerConfig,
    RemoteMcpServerConfig,
};
use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
    ResolvedGlobalPrompt,
};
use agent::mcp::McpRegistry;
use agent::protocol::SwapAgentArgs;
use agent::tui_commands::McpServerStatus;
use common::*;
use mock_mcp_server::{
    MockMcpServerBuilder,
    MockResponse,
    ToolDef,
    prebuild_bin,
};

const TOOL_NAME: &str = "echo";

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

fn remote_oauth_config(url: String) -> McpServerConfig {
    McpServerConfig::Remote(RemoteMcpServerConfig {
        url,
        headers: HashMap::new(),
        timeout_ms: 30_000,
        oauth_scopes: vec![],
        oauth: None,
        disabled: false,
        disabled_tools: vec![],
        force_auth: false,
    })
}

/// Invalidate every access token the mock has issued, simulating server-side
/// expiry. The client's cached token will be rejected (401) on its next request.
async fn expire_server_tokens(port: u16) {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/control/expire-tokens");
    let resp = client.post(&url).send().await.expect("expire-tokens request failed");
    assert!(resp.status().is_success(), "expire-tokens returned {}", resp.status());
}

/// An `McpRegistry` that resolves one named `Registry` placeholder into a
/// concrete config — stands in for Kiro's real registry.
#[derive(Debug, Clone)]
struct ResolveTo {
    name: String,
    cfg: McpServerConfig,
}

impl McpRegistry for ResolveTo {
    fn apply(&self, agent_config: &mut LoadedAgentConfig) {
        let updates: Vec<(String, McpServerConfig)> = agent_config
            .config()
            .mcp_servers()
            .iter()
            .filter(|(name, cfg)| name.as_str() == self.name && matches!(cfg, McpServerConfig::Registry(_)))
            .map(|(name, _)| (name.clone(), self.cfg.clone()))
            .collect();
        agent_config.config_mut().insert_mcp_servers(updates);
    }
}

fn registry_placeholder() -> McpServerConfig {
    McpServerConfig::Registry(RegistryMcpServerConfig {
        server_type: "registry".to_string(),
        env: None,
        headers: None,
        timeout: None,
        oauth_scopes: vec![],
        oauth: None,
    })
}

/// Build a `TestCase` whose only MCP server is an OAuth-protected mock, with the
/// initial sign-in auto-driven, then drive it to a state where a forced re-auth
/// is **pending** on the server `srv`:
///   1. The server signs in and loads (initial OAuth driven).
///   2. Its access token is invalidated server-side.
///   3. `reauth` launches a shadow; with no refresh token available it falls onto a fresh, undriven
///      authorization request and parks there.
///
/// On return, `srv` reports `authenticating = true` while still `Running`.
async fn pending_shadow(
    test_name: &str,
    server: &mock_mcp_server::MockMcpServerHandle,
) -> (TestCase, tempfile::TempDir) {
    let cred_dir = tempfile::tempdir().unwrap();

    let test = TestCase::builder()
        .test_name(test_name)
        .with_default_agent_config()
        .with_mcp_server("srv", remote_oauth_config(server.url()))
        .with_mcp_cred_dir(cred_dir.path().to_path_buf())
        .with_oauth_autodrive(Some(1)) // drive only the initial sign-in
        .build()
        .await
        .unwrap();

    // Original signs in and loads.
    test.wait_for_mcp_server("srv", Duration::from_secs(30), |s| {
        matches!(s.status, McpServerStatus::Running)
    })
    .await;

    // Invalidate the token so the forced re-auth must hit a fresh (undriven)
    // authorization request and stay pending.
    expire_server_tokens(server.port()).await;

    test.reauth_mcp_server("srv").await.expect("reauth should dispatch");

    // The master shows `authenticating` while the shadow runs, and keeps its tools.
    let info = test
        .wait_for_mcp_server("srv", Duration::from_secs(30), |s| s.authenticating)
        .await;
    assert!(
        matches!(info.status, McpServerStatus::Running),
        "original must stay running during forced auth; got {:?}",
        info.status
    );
    assert!(info.tool_count > 0, "original keeps its tools during forced auth");

    (test, cred_dir)
}

/// Scenario: aborting a forced auth (^X) drops the shadow only — the original
/// server keeps running, so the user never loses access to its tools.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_drops_shadow_and_keeps_original() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");
    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth_no_refresh_token()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let (test, _cred_dir) = pending_shadow("abort_drops_shadow_and_keeps_original", &server).await;

    // Abort the pending auth.
    test.abort_mcp_server_auth("srv").await.expect("abort should succeed");

    // The shadow is gone (no more `authenticating`), and the original is intact.
    let info = test
        .wait_for_mcp_server("srv", Duration::from_secs(30), |s| !s.authenticating)
        .await;
    assert!(
        matches!(info.status, McpServerStatus::Running),
        "original must still be running after abort; got {:?}",
        info.status
    );
    assert!(info.tool_count > 0, "tools remain available after aborting forced auth");
}

/// Scenario: switching agents (a full swap) offloads any in-flight shadow — the
/// new agent must not report a phantom `authenticating` server (the bug where
/// stale shadow tracking survived the manager teardown).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_swap_offloads_shadow() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");
    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth_no_refresh_token()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let (test, _cred_dir) = pending_shadow("agent_swap_offloads_shadow", &server).await;

    // Switch to a different agent that declares a same-named server, this time a
    // trivial local one (no OAuth). The forced-auth tracking from the old agent
    // must be cleared during the swap.
    let mut swapped = AgentConfigV2025_08_22 {
        name: "swapped-agent".to_string(),
        tools: vec!["*".to_string()],
        ..Default::default()
    };
    swapped.mcp_servers.insert(
        "srv".to_string(),
        McpServerConfig::Local(LocalMcpServerConfig {
            command: "/bin/echo".to_string(),
            args: vec![],
            env: None,
            timeout_ms: 5_000,
            disabled: false,
            disabled_tools: vec![],
        }),
    );

    test.swap_agent(SwapAgentArgs {
        agent_config: LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(swapped),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        ),
        force: true,
        knowledge_provider: None,
    })
    .await
    .expect("swap should succeed");

    // After the switch the server must not be stuck "authenticating".
    let info = test
        .wait_for_mcp_server("srv", Duration::from_secs(30), |s| !s.authenticating)
        .await;
    assert!(
        !info.authenticating,
        "agent switch must offload the shadow — no phantom authenticating state"
    );
}

/// Scenario: forced re-auth works end-to-end for a **registry-resolved** server.
/// The server starts as a `Registry` placeholder, is resolved to a `Remote` by
/// the agent's registry, signs in, and then a forced re-auth runs a shadow that
/// completes and is promoted — leaving the server available with its tools.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reauth_promotes_shadow_for_registry_resolved_server() {
    prebuild_bin().expect("failed to prebuild mock-mcp-server");
    let server = MockMcpServerBuilder::new()
        .add_tool(echo_tool())
        .add_response(echo_response())
        .oauth_no_refresh_token()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    server
        .wait_ready(Duration::from_secs(10))
        .expect("mock server not ready");

    let cred_dir = tempfile::tempdir().unwrap();

    // agent.json declares "regsrv" as a registry placeholder; the registry
    // resolves it to the OAuth-protected mock.
    let mut config = AgentConfigV2025_08_22 {
        name: "registry-reauth".to_string(),
        tools: vec!["*".to_string()],
        ..Default::default()
    };
    config.mcp_servers.insert("regsrv".to_string(), registry_placeholder());

    let registry = ResolveTo {
        name: "regsrv".to_string(),
        cfg: remote_oauth_config(server.url()),
    };

    let test = TestCase::builder()
        .test_name("reauth_promotes_shadow_for_registry_resolved_server")
        .with_agent_config(AgentConfig::V2025_08_22(config))
        .with_mcp_registry(Box::new(registry))
        .with_mcp_cred_dir(cred_dir.path().to_path_buf())
        .with_oauth_autodrive(None) // drive both the initial sign-in and the shadow's re-auth
        .build()
        .await
        .unwrap();

    // The registry-resolved server signs in and loads.
    test.wait_for_mcp_server("regsrv", Duration::from_secs(30), |s| {
        matches!(s.status, McpServerStatus::Running)
    })
    .await;

    // Invalidate the token, then force re-auth. With the browser driver active,
    // the shadow re-authenticates and is promoted in place.
    expire_server_tokens(server.port()).await;
    test.reauth_mcp_server("regsrv").await.expect("reauth should dispatch");

    // After promotion the server is available again with its tools, and is no
    // longer reported as authenticating.
    let info = test
        .wait_for_mcp_server("regsrv", Duration::from_secs(30), |s| {
            !s.authenticating && matches!(s.status, McpServerStatus::Running)
        })
        .await;
    assert!(
        info.tool_count > 0,
        "registry-resolved server should keep its tools after a forced re-auth + promotion"
    );
}
