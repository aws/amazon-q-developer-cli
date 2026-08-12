//! Every launch request must reach a terminal outcome, and reach it without
//! waiting on something that will never arrive.
//!
//! Session startup waits on launch outcomes under a shared deadline, so a launch
//! that reports nothing at all costs every session the full deadline and leaves
//! the server stuck in a limbo state that never resolves. The cases here are the
//! ones that used to have no outcome to report:
//!
//!   - a remote server that answers the probe with an authorization challenge, where the next step
//!     belongs to the user and may never happen;
//!   - a local server that goes quiet at any point in its launch, before answering `initialize` or
//!     after;
//!   - and the ordinary fast server, which must keep initializing as before — including when its
//!     per-request timeout is small or unlimited, since the launch bound is floored well above it.
//!
//! Startup proceeds without an authorization-pending server by default. An agent
//! that has nobody to answer a later prompt can ask to wait for it instead, and
//! that wait must end as soon as the grant lands.

mod common;

use std::collections::HashMap;
use std::time::Duration;

use agent::agent_config::McpServerConfigSource;
use agent::agent_config::definitions::{
    LocalMcpServerConfig,
    McpServerConfig,
    RemoteMcpServerConfig,
};
use agent::mcp::{
    LaunchOutcome,
    McpManager,
    McpServerEvent,
};
use agent::protocol::{
    AgentEvent,
    InitializeUpdateEvent,
};
use agent::types::AgentSettings;
use common::*;
use mock_mcp_server::{
    ConfigEntry,
    MockMcpServerBuilder,
    MockResponse,
    ToolDef,
    prebuild_bin,
};
use tokio::sync::broadcast::error::RecvError;

const TOOL_NAME: &str = "echo";

/// Generous enough to absorb CI scheduling noise while still failing any launch
/// that reports no outcome at all — those never resolve, at any bound.
const OUTCOME_BOUND: Duration = Duration::from_secs(20);

/// For the tests that let the clock run ahead instead of waiting: must outlast
/// the launch bound itself, which is minutes long by design.
const VIRTUAL_OUTCOME_BOUND: Duration = Duration::from_secs(600);

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

fn remote_config(url: String) -> McpServerConfig {
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

/// A launch that stops at an authorization prompt reports back immediately, so
/// startup can move on while the user decides.
#[tokio::test]
async fn awaiting_authorization_is_reported_without_waiting_for_the_grant() {
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
    let mut events = handle.clone();

    // Nothing here plays the user's browser, so the authorization is never granted.
    let rx = handle
        .launch_server(
            "pending-auth".to_string(),
            remote_config(server.url()),
            McpServerConfigSource::Registry,
        )
        .await
        .expect("launch dispatch should succeed");

    let outcome = tokio::time::timeout(OUTCOME_BOUND, rx)
        .await
        .expect("launch must report an outcome without waiting for the user")
        .expect("launch result channel dropped")
        .expect("an authorization prompt is not a launch failure");
    assert_eq!(outcome, LaunchOutcome::AwaitingAuthorization);

    // The flow itself is untouched: the prompt still reaches the user.
    let saw_prompt = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(McpServerEvent::OauthRequest { server_name, oauth_url }) => {
                    assert_eq!(server_name, "pending-auth");
                    assert!(!oauth_url.is_empty());
                    return true;
                },
                Ok(_) | Err(RecvError::Lagged(_)) => {},
                Err(RecvError::Closed) => return false,
            }
        }
    })
    .await
    .expect("authorization prompt should still be emitted");
    assert!(saw_prompt);

    handle.shutdown().await;
}

/// A server that starts but never speaks MCP is given up on instead of leaving
/// the launch with no outcome forever.
#[tokio::test]
async fn a_server_that_never_handshakes_reaches_a_terminal_failure() {
    // Both arms are type-checked wherever this file compiles, so neither can rot
    // unnoticed on the platform that does not run it.
    let (command, args) = if cfg!(windows) {
        // The local equivalent of `cat`: it reads stdin, and with a pattern that
        // never matches it writes nothing back.
        ("findstr", vec!["/c:handshake-that-never-arrives".to_string()])
    } else {
        // `cat` holds stdin open and answers nothing.
        ("cat", vec![])
    };
    assert_never_handshakes(command, args).await;
}

async fn assert_never_handshakes(command: &str, args: Vec<String>) {
    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    // A per-request timeout below the launch floor, so the launch bound is the floor.
    let config = McpServerConfig::Local(LocalMcpServerConfig {
        command: command.to_string(),
        args,
        env: None,
        timeout_ms: 30_000,
        disabled: false,
        disabled_tools: vec![],
    });

    let rx = handle
        .launch_server("mute".to_string(), config, McpServerConfigSource::AgentConfig)
        .await
        .expect("launch dispatch should succeed");

    // Run the clock ahead of the launch bound rather than wait it out. A launch
    // with no bound at all still reports nothing, at any point on the clock.
    tokio::time::pause();
    let result = tokio::time::timeout(VIRTUAL_OUTCOME_BOUND, rx)
        .await
        .expect("launch must report an outcome")
        .expect("launch result channel dropped");
    let err = result.expect_err("a server that never handshakes must fail");
    assert!(
        err.to_string().contains("not responding"),
        "expected the launch to be given up on, got: {err}"
    );

    handle.shutdown().await;
}

/// Answering `initialize` and then going quiet must fail too: the tool and prompt
/// listings the launch makes next are part of the launch, so a stall there is the
/// same no-outcome bug one step later.
#[cfg(unix)]
#[tokio::test]
async fn a_server_that_stalls_after_the_handshake_reaches_a_terminal_failure() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let listing_seen = dir.path().join("listing-requested");
    let script = dir.path().join("stall-after-handshake.sh");
    // Answers `initialize` with tool support, records that the tool listing was
    // asked for, then never answers anything again.
    std::fs::write(
        &script,
        r#"#!/bin/sh
IFS= read -r req
id=$(printf '%s' "$req" | sed -n 's/^.*"id"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*$/\1/p')
[ -n "$id" ] || id=0
printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"stall","version":"0.0.0"}}}\n' "$id"
while IFS= read -r line; do
  case "$line" in
    *tools/list*) : > "$1" ;;
  esac
done
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let config = McpServerConfig::Local(LocalMcpServerConfig {
        command: script.to_string_lossy().to_string(),
        args: vec![listing_seen.to_string_lossy().to_string()],
        env: None,
        timeout_ms: 30_000,
        disabled: false,
        disabled_tools: vec![],
    });

    let rx = handle
        .launch_server("stall".to_string(), config, McpServerConfigSource::AgentConfig)
        .await
        .expect("launch dispatch should succeed");

    // Wait on real time until the stall is provably past the handshake: the
    // listing request only goes out once `initialize` has been answered.
    let deadline = tokio::time::Instant::now() + OUTCOME_BOUND;
    while !listing_seen.exists() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the server never got as far as the tool listing"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    tokio::time::pause();
    let result = tokio::time::timeout(VIRTUAL_OUTCOME_BOUND, rx)
        .await
        .expect("a stall after the handshake must still report an outcome")
        .expect("launch result channel dropped");
    let err = result.expect_err("a server that stops answering must fail");
    assert!(
        err.to_string().contains("not responding"),
        "expected the launch to be given up on, got: {err}"
    );

    handle.shutdown().await;
}

/// The ordinary case still initializes and still exposes its tools.
#[tokio::test]
async fn a_fast_server_still_initializes() {
    assert_local_mock_initializes("fast", 30_000).await;
}

/// `timeout_ms` bounds individual requests, and zero is the config's way of
/// spelling "no limit". Neither reading may keep a healthy server from launching.
#[tokio::test]
async fn an_unlimited_request_timeout_still_lets_a_server_launch() {
    assert_local_mock_initializes("unlimited", 0).await;
}

async fn assert_local_mock_initializes(server_name: &str, timeout_ms: u64) {
    let bin = prebuild_bin().expect("failed to build mock server binary");
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.jsonl");
    let entries = [ConfigEntry::Tool(echo_tool()), ConfigEntry::Response(echo_response())]
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, entries).unwrap();

    let cred_dir = tempfile::tempdir().unwrap();
    let mut handle = McpManager::new(cred_dir.path().to_path_buf()).spawn();

    let config = McpServerConfig::Local(LocalMcpServerConfig {
        command: bin.to_string_lossy().to_string(),
        args: vec!["--config".to_string(), config_path.to_string_lossy().to_string()],
        env: None,
        timeout_ms,
        disabled: false,
        disabled_tools: vec![],
    });

    let rx = handle
        .launch_server(server_name.to_string(), config, McpServerConfigSource::AgentConfig)
        .await
        .expect("launch dispatch should succeed");

    let outcome = tokio::time::timeout(OUTCOME_BOUND, rx)
        .await
        .expect("launch must report an outcome")
        .expect("launch result channel dropped")
        .expect("a well-behaved server should initialize");
    assert_eq!(outcome, LaunchOutcome::Initialized);

    let specs = handle
        .get_tool_specs(server_name.to_string())
        .await
        .expect("tool specs should be available once initialized");
    assert!(
        specs.iter().any(|s| s.name.contains(TOOL_NAME)),
        "expected the echo tool; got {:?}",
        specs.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    handle.shutdown().await;
}

fn oauth_url_of(evt: &AgentEvent) -> Option<String> {
    match evt {
        AgentEvent::Mcp(McpServerEvent::OauthRequest { oauth_url, .. })
        | AgentEvent::InitializeUpdate(InitializeUpdateEvent::Mcp(McpServerEvent::OauthRequest {
            oauth_url, ..
        })) => Some(oauth_url.clone()),
        _ => None,
    }
}

/// An agent that asks to wait for authorization holds startup while the prompt is
/// outstanding — a delegated turn would otherwise begin silently tool-less — and
/// resumes the moment the grant lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_waits_for_a_pending_authorization_when_asked_to() {
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
    let mut test = TestCase::builder()
        .test_name("startup_waits_for_a_pending_authorization_when_asked_to")
        .with_default_agent_config()
        .with_mcp_server("pending-auth", remote_config(server.url()))
        .with_mcp_cred_dir(cred_dir.path().to_path_buf())
        .with_settings(AgentSettings {
            // Long enough that only the grant can end the wait.
            mcp_init_timeout: Duration::from_secs(300),
            mcp_wait_for_authorization: true,
            ..Default::default()
        })
        .build()
        .await
        .expect("failed to build test agent");

    // Nothing plays the user's browser yet, so the authorization stays outstanding.
    let prompt = test
        .wait_until_agent_event(OUTCOME_BOUND, |evt| oauth_url_of(evt).is_some())
        .await
        .expect("the authorization prompt should reach the agent's client");
    let oauth_url = oauth_url_of(&prompt).expect("prompt event should carry the url");

    let released_early = test
        .wait_until_agent_event(Duration::from_secs(3), |evt| matches!(evt, AgentEvent::Initialized))
        .await;
    assert!(
        released_early.is_err(),
        "startup finished while the authorization was still outstanding"
    );

    // Play the user's browser: the authorize URL redirects to the loopback
    // listener with the code, which completes the flow.
    reqwest::Client::new()
        .get(&oauth_url)
        .send()
        .await
        .expect("failed to drive the authorization");

    test.wait_until_agent_event(Duration::from_secs(60), |evt| matches!(evt, AgentEvent::Initialized))
        .await
        .expect("startup should resume once the authorization is granted");
}
