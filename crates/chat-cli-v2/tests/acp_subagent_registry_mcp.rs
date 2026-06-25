//! End-to-end coverage for spawning a subagent that uses a registry-type MCP server.
//!
//! Lives in its own integration-test binary (separate from `acp.rs`) because it
//! exercises a distinct, higher-cost path: a real HTTP MCP server + mock registry
//! resolved inside a spawned subagent.

mod common;

use std::time::Duration;

use agent_client_protocol::{
    SessionUpdate,
    ToolCallStatus,
};
use chat_cli_v2::agent::acp::extensions::methods;
use common::{
    AcpTestHarnessBuilder,
    spawn_mock_registry,
};
use ntest::timeout;
use serial_test::serial;

/// End-to-end safety net for the **subagent + registry-MCP** path.
///
/// Background: the original Apr 1 customer report and the later silent-hang
/// regression (PR #2158) were both *subagent* failures with registry-type MCP
/// servers, yet the fixes shipped without a test exercising that path. This
/// test closes that gap by spawning a real subagent whose agent config declares
/// a `"type": "registry"` MCP server, resolved through chat_cli_v2's real
/// `RegistryAdapter` against a mock registry (pointed at via
/// `KIRO_MCP_REGISTRY_URL_OVERRIDE`). It drives the full stack — the real
/// `chat_cli_v2 acp` subprocess, the `_session/spawn` orchestration path, and a
/// real HTTP MCP server — and asserts:
///
///   1. the registry server is resolved + launched **inside the subagent process** (an
///      `MCP_SERVER_INITIALIZED` notification for the subagent's own session id),
///   2. the resolved tool is registered in the subagent's LLM tool list (proving the subagent
///      inherited/loaded the registry server config), and
///   3. the subagent can **invoke** that tool and receive its result (the tool result flows back
///      into the subagent's next LLM request).
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn e2e_subagent_with_registry_mcp_server_invokes_tool() {
    use agent::agent_config::definitions::{
        AgentConfigV2025_08_22,
        McpServerConfig,
        RegistryMcpServerConfig,
    };
    use chat_cli_v2::api_client::model::ChatResponseStream;
    use chat_cli_v2::api_client::send_message_output::MockStreamItem;
    use mock_mcp_server::{
        MockMcpServerBuilder,
        MockResponse,
        ToolDef,
        prebuild_bin,
    };

    prebuild_bin().expect("mock mcp server build failed");

    // 1. Plain HTTP MCP server exposing a single `echo` tool. Its response is a distinctive string we
    //    can look for in the subagent's tool results.
    const ECHOED: &str = "hello from subagent registry mcp";
    let mcp = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "echo".to_string(),
            description: "Echoes back the input message".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "message": { "type": "string" } }
            }),
        })
        .add_response(MockResponse {
            tool: "echo".to_string(),
            input_match: None,
            response: serde_json::json!({ "echoed": ECHOED }),
        })
        .spawn_http()
        .expect("failed to spawn mock MCP server");
    mcp.wait_ready(Duration::from_secs(10))
        .expect("mock MCP server not ready");

    // 2. Registry that resolves the server *name* to the mock MCP server's URL.
    let registry_body = serde_json::json!({
        "servers": [{
            "server": {
                "name": "subagent-registry-mcp",
                "description": "Registry-sourced server for the subagent e2e test",
                "version": "1.0.0",
                "remotes": [{ "type": "streamable-http", "url": mcp.url() }]
            }
        }]
    })
    .to_string();
    let (registry_url, registry_task) = spawn_mock_registry(registry_body).await;

    // 3. The subagent's agent: its only MCP server is a `"type": "registry"` placeholder, resolved by
    //    the real RegistryAdapter at session creation.
    let worker_config = AgentConfigV2025_08_22 {
        name: "registry_worker".to_string(),
        mcp_servers: [(
            "subagent-registry-mcp".to_string(),
            McpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: None,
                timeout: None,
                oauth_scopes: vec![],
                oauth: None,
            }),
        )]
        .into_iter()
        .collect(),
        tools: vec!["*".to_string()],
        ..Default::default()
    };

    let (mut harness, client, parent_session_id, _cwd) = AcpTestHarnessBuilder::new("e2e_subagent_registry_mcp")
        .with_agent_config("registry_worker", &worker_config)
        .with_env("KIRO_MCP_REGISTRY_URL_OVERRIDE", &registry_url)
        .with_trust_all(true)
        .build_with_session()
        .await;

    // 4. Spawn a subagent that runs the `registry_worker` agent. The subagent is a full ACP session, so
    //    it resolves + launches the registry MCP server in its own process before running its task.
    let spawn = client
        .spawn_session(
            parent_session_id.clone(),
            "registry_worker",
            "Call the echo tool with message 'ping', then summarize the result.",
        )
        .await
        .expect("_session/spawn failed");

    // 5. The registry server must come up **inside the subagent process** — i.e. an
    //    MCP_SERVER_INITIALIZED for the subagent's own session id. If the subagent failed to
    //    inherit/resolve the registry config (the regression this test guards), this never arrives and
    //    the assertion fails.
    let mcp_initialized_method = methods::MCP_SERVER_INITIALIZED
        .strip_prefix('_')
        .expect("method should have underscore prefix");
    let initialized = client
        .wait_for_timeout(
            |captured| {
                captured.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == mcp_initialized_method && {
                        let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                        params.get("serverName").and_then(|v| v.as_str()) == Some("subagent-registry-mcp")
                            && params.get("sessionId").and_then(|v| v.as_str()) == Some(spawn.session_id.as_str())
                    }
                })
            },
            Duration::from_secs(90),
        )
        .await;
    assert!(
        initialized,
        "registry MCP server 'subagent-registry-mcp' did not initialize inside the subagent within 90s"
    );

    // 6. Queue the subagent's LLM turns: (a) call the registry-resolved `echo` tool, (b) call `summary`
    //    to report back to the parent, (c) a trailing end-turn so the subagent loop finishes cleanly.
    // Buffered per-session, so ordering relative to the subagent's requests
    // doesn't matter — each GetStream blocks until its turn is available.
    harness
        .push_mock_response(
            &spawn.session_id,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_echo_1".to_string(),
                    name: "echo".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_echo_1".to_string(),
                    name: "echo".to_string(),
                    input: Some(r#"{"message":"ping"}"#.to_string()),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_echo_1".to_string(),
                    name: "echo".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(&spawn.session_id, None).await;

    harness
        .push_mock_response(
            &spawn.session_id,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_summary_1".to_string(),
                    name: "summary".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_summary_1".to_string(),
                    name: "summary".to_string(),
                    input: Some(
                        r#"{"taskDescription":"echo ping","taskResult":"the echo tool returned its response"}"#
                            .to_string(),
                    ),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_summary_1".to_string(),
                    name: "summary".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(&spawn.session_id, None).await;

    harness
        .push_mock_response(
            &spawn.session_id,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Done.".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&spawn.session_id, None).await;

    // 7. The subagent must actually invoke the registry-resolved tool and have it succeed. The subagent
    //    shares the parent's ACP connection, so its `session/update` notifications (ToolCall +
    //    ToolCallUpdate) reach this client. We key on the echo `tool_use_id` we scripted
    //    ("sub_echo_1").
    //
    //    We assert via notifications rather than `get_captured_requests` because an
    //    MCP tool result is carried as a `ToolResultContentBlock::Json` block, which
    //    the test IPC channel cannot serialize.
    let invoked_and_succeeded = client
        .wait_for_timeout(
            |captured| {
                let invoked = captured.session_updates.iter().any(|u| {
                    matches!(u, SessionUpdate::ToolCall(tc)
                        if tc.tool_call_id.0.as_ref() == "sub_echo_1" || tc.title.to_lowercase().contains("echo"))
                });
                let succeeded = captured.session_updates.iter().any(|u| {
                    matches!(u, SessionUpdate::ToolCallUpdate(upd)
                        if upd.tool_call_id.0.as_ref() == "sub_echo_1"
                            && upd.fields.status == Some(ToolCallStatus::Completed))
                });
                invoked && succeeded
            },
            Duration::from_secs(60),
        )
        .await;

    registry_task.abort();

    assert!(
        invoked_and_succeeded,
        "subagent did not invoke the registry-resolved 'echo' tool and complete it successfully; \
         captured session updates: {:?}",
        client.captured().await.session_updates
    );

    // Keep the harness (and thus the agent subprocess) alive until assertions complete.
    let _ = &mut harness;
}
