mod common;

use std::time::Duration;

use agent_client_protocol::{
    SessionUpdate,
    ToolCallStatus,
};
use amzn_codewhisperer_streaming_client::types::builders::AssistantResponseEventBuilder;
use chat_cli_v2::agent::acp::extensions::methods;
use common::{
    AcpTestClient,
    AcpTestHarness,
    AcpTestHarnessBuilder,
};
use kiro_telemetry::metric;
use kiro_telemetry::testing::{
    expect_metric,
    expect_metric_attrs,
};
use kiro_telemetry_legacy::event_to_otel_metric_record;
use ntest::timeout;
use serial_test::serial;
use tokio::time::sleep;

use crate::common::PermissionResponse;

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn initialize() {
    let mut harness = AcpTestHarness::new("initialize").await;
    let (stdin, stdout) = harness.take_stdio();

    let client = common::AcpTestClient::spawn(stdin, stdout, true);

    let resp = client.initialize().await.expect("initialize failed");
    assert_eq!(resp.protocol_version, agent_client_protocol::ProtocolVersion::V1);

    // Verify auth_methods contains login guidance
    assert_eq!(resp.auth_methods.len(), 1);
    let auth_method = &resp.auth_methods[0];
    assert_eq!(auth_method.id().0.as_ref(), "kiro-login");
    assert_eq!(auth_method.name(), "Kiro Login");
    assert!(
        auth_method
            .description()
            .unwrap()
            .contains("https://kiro.dev/docs/cli/authentication/")
    );
}

/// Verifies that new_session waits for MCP server initialization before returning.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn new_session_waits_for_mcp_server_initialization() {
    // MCP stdio handshake is unreliable on CI runners under load
    if std::env::var("CI").is_ok() {
        return;
    }

    use std::path::PathBuf;
    use std::time::Instant;

    use mock_mcp_server::prebuild_bin;
    use sacp::schema::McpServerStdio;

    let binary_path = prebuild_bin().expect("failed to build mock-mcp-server");

    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/stdio_server.jsonl");

    let (harness, client) = AcpTestHarnessBuilder::new("new_session_mcp_init")
        .with_trust_all(true)
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();
    let mcp_server = sacp::schema::McpServer::Stdio(McpServerStdio::new("test-mcp", binary_path).args(vec![
        "--config".to_string(),
        config_path.to_str().unwrap().to_string(),
        "--startup-delay-ms".to_string(),
        "2000".to_string(),
    ]));

    let start = Instant::now();
    let _resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");
    let elapsed = start.elapsed();

    // new_session should have waited for the 2s server startup delay
    assert!(
        elapsed.as_millis() >= 2000,
        "new_session returned in {}ms, expected >= 2000ms (should wait for MCP server init)",
        elapsed.as_millis()
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn new_session_creates_files() {
    let (harness, _, session_id, _) = AcpTestHarnessBuilder::new("new_session_creates_files")
        .build_with_session()
        .await;

    // Verify session files exist
    let metadata_path = harness.paths.sessions_dir.join(format!("{}.json", session_id));
    let log_path = harness.paths.sessions_dir.join(format!("{}.jsonl", session_id));

    assert!(
        metadata_path.exists(),
        "session metadata should exist at {:?}",
        metadata_path
    );
    assert!(log_path.exists(), "session log should exist at {:?}", log_path);
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn load_session_emits_history() {
    let (mut harness, client, session_id, cwd) = AcpTestHarnessBuilder::new("load_session_emits_history")
        .with_trust_all(true)
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/write_hello_world_in_bash.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "write hello world in bash to hello.sh")
        .await
        .expect("prompt failed");

    // Clear notifications from first prompt
    client.clear_captured().await;

    // Load the session - should emit historical notifications
    client
        .load_session(session_id.clone(), cwd)
        .await
        .expect("load_session failed");

    // Wait for notifications to arrive (they're sent async)
    sleep(Duration::from_millis(100)).await;

    let captured = client.captured().await;
    // Filter out AvailableCommandsUpdate - not part of conversation history
    let _updates: Vec<_> = captured
        .session_updates
        .iter()
        .filter(|u| !matches!(u, SessionUpdate::AvailableCommandsUpdate(_)))
        .collect();

    let captured = client.captured().await;
    // Filter out AvailableCommandsUpdate - not part of conversation history
    let updates: Vec<_> = captured
        .session_updates
        .iter()
        .filter(|u| !matches!(u, SessionUpdate::AvailableCommandsUpdate(_)))
        .collect();

    // Verify sequence order: UserMessageChunk -> AgentMessageChunk -> ToolCall -> ToolCallUpdate
    // (completed) -> AgentMessageChunk
    let mut iter = updates.iter();
    assert!(iter.any(|u| matches!(u, SessionUpdate::UserMessageChunk(_))));
    assert!(iter.any(|u| matches!(u, SessionUpdate::AgentMessageChunk(_))));
    assert!(iter.any(|u| matches!(u, SessionUpdate::ToolCall(_))));
    assert!(iter.any(
        |u| matches!(u, SessionUpdate::ToolCallUpdate(upd) if upd.fields.status == Some(ToolCallStatus::Completed))
    ));
    assert!(iter.any(|u| matches!(u, SessionUpdate::AgentMessageChunk(_))));
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn load_session_emits_failed_for_orphaned_tool_calls() {
    let (mut harness, client, session_id, cwd) =
        AcpTestHarnessBuilder::new("load_session_emits_failed_for_orphaned_tool_calls")
            .with_trust_all(true)
            .build_with_session()
            .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/write_hello_world_in_bash.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "write hello world in bash to hello.sh")
        .await
        .expect("prompt failed");

    // Remove the ToolResults line from the session log to simulate a crash mid-tool-execution.
    let log_path = harness.paths.sessions_dir.join(format!("{}.jsonl", session_id.0));
    let content = std::fs::read_to_string(&log_path).expect("failed to read session log");
    let filtered: String = content
        .lines()
        .filter(|line| !line.contains("\"ToolResults\""))
        .map(|line| format!("{}\n", line))
        .collect();
    std::fs::write(&log_path, filtered).expect("failed to write session log");

    client.clear_captured().await;

    client
        .load_session(session_id.clone(), cwd)
        .await
        .expect("load_session failed");

    sleep(Duration::from_millis(100)).await;

    let captured = client.captured().await;
    let updates: Vec<_> = captured
        .session_updates
        .iter()
        .filter(|u| !matches!(u, SessionUpdate::AvailableCommandsUpdate(_)))
        .collect();

    // Should have a ToolCall followed by a ToolCallUpdate with Failed status (orphaned)
    let mut iter = updates.iter();
    assert!(iter.any(|u| matches!(u, SessionUpdate::ToolCall(_))));
    assert!(
        iter.any(
            |u| matches!(u, SessionUpdate::ToolCallUpdate(upd) if upd.fields.status == Some(ToolCallStatus::Failed))
        )
    );
}

/// Regression: reloading an already-active session must not destroy its on-disk
/// files. The manager shuts the previous live instance down during reload; its
/// `Drop` must not delete the `{id}.json` / `{id}.jsonl` / `{id}.lock` files that
/// the freshly-loaded instance now owns. We reload twice (a second `session/load`
/// would fail with NotFound if the metadata had been deleted) and confirm the
/// session is still usable afterward.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn reload_active_session_preserves_files_and_stays_usable() {
    let (mut harness, client, session_id, cwd) = AcpTestHarnessBuilder::new("reload_active_session_preserves_files")
        .with_trust_all(true)
        .build_with_session()
        .await;

    let meta = harness.paths.sessions_dir.join(format!("{}.json", session_id.0));
    let log = harness.paths.sessions_dir.join(format!("{}.jsonl", session_id.0));
    let lock = harness.paths.sessions_dir.join(format!("{}.lock", session_id.0));

    assert!(meta.exists(), "metadata should exist after new_session");
    assert!(log.exists(), "log should exist after new_session");

    // Reload the still-active session twice. Each reload shuts the previous live
    // instance down before building the new one; the old Drop must leave the
    // shared files intact for the new owner.
    for i in 1..=2 {
        client
            .load_session(session_id.clone(), cwd.clone())
            .await
            .unwrap_or_else(|e| panic!("reload #{i} failed: {e:?}"));

        assert!(meta.exists(), "metadata must survive reload #{i} of an active session");
        assert!(log.exists(), "log must survive reload #{i} of an active session");
        assert!(lock.exists(), "the new owner should hold the lock after reload #{i}");
    }

    // The reloaded session must still accept prompts.
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), "hello after reload")
        .await
        .expect("prompt after reload should succeed");

    assert!(
        meta.exists(),
        "metadata must still exist after prompting the reloaded session"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn prompt_with_send_error() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("prompt_with_send_error")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/send_error.jsonl")
        .await;

    // Prompt should return an error because send_message fails
    let result = client.prompt_text(session_id, "hello").await;
    assert!(result.is_err(), "expected prompt to fail with send error");

    // Verify telemetry events capture the failure.
    // Use polling because telemetry events are emitted asynchronously and may still
    // be in the pipeline when prompt_text returns an error.
    let events = harness
        .wait_for_telemetry_events(Duration::from_secs(5), |events| {
            let has_add_msg = events
                .iter()
                .any(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }));
            let has_turn = events.iter().any(|e| {
                matches!(
                    &e.ty,
                    chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { .. }
                )
            });
            has_add_msg && has_turn
        })
        .await;

    // addChatMessage with Failed
    let add_msg = events
        .iter()
        .find(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }))
        .expect("expected addChatMessage event");
    if let chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { result, data, .. } = &add_msg.ty {
        assert_eq!(*result, chat_cli_v2::telemetry::TelemetryResult::Failed);
        assert_eq!(data.reason.as_deref(), Some("QuotaBreachError"));
    }

    // messageResponseError
    let err_event = events
        .iter()
        .find(|e| {
            matches!(
                &e.ty,
                chat_cli_v2::telemetry::core::EventType::MessageResponseError { .. }
            )
        })
        .expect("expected messageResponseError event");
    if let chat_cli_v2::telemetry::core::EventType::MessageResponseError {
        reason, status_code, ..
    } = &err_event.ty
    {
        assert_eq!(reason.as_deref(), Some("QuotaBreachError"));
        assert_eq!(*status_code, Some(429));
    }

    // recordUserTurnCompletion with Failed
    let turn = events
        .iter()
        .find(|e| {
            matches!(
                &e.ty,
                chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { .. }
            )
        })
        .expect("expected recordUserTurnCompletion event");
    if let chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { result, args, .. } = &turn.ty {
        assert_eq!(*result, chat_cli_v2::telemetry::TelemetryResult::Failed);
        assert_eq!(args.reason.as_deref(), Some("QuotaBreachError"));
    }
}

#[tokio::test]
#[serial]
async fn captured_request_contains_conversation_state() {
    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("captured_request_contains_conversation_state")
            .build_with_session()
            .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    // Send a prompt (blocks until turn completes)
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    // Verify the captured request contains the full ConversationState
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 1, "should have captured one request");

    // Verify we have access to the conversation structure
    let conv = &captured[0];
    assert!(conv.conversation_id.is_some(), "should have conversation_id");
    assert!(
        conv.user_input_message.content.contains("hello"),
        "current message should contain the prompt, got: {}",
        conv.user_input_message.content
    );
    assert!(conv.history.is_some(), "should have history");

    // Verify tools are present in user_input_message_context
    let tools = conv
        .user_input_message
        .user_input_message_context
        .as_ref()
        .and_then(|ctx| ctx.tools.as_ref());
    assert!(tools.is_some(), "should have tools in context");
    assert!(!tools.unwrap().is_empty(), "tools should not be empty");
}

/// Regression test: a prompt starting with "/" that doesn't match any known
/// slash command or prompt should reach the LLM verbatim (previously only
/// the leading token made it through).
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn prompt_starting_with_slash_is_not_truncated() {
    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("prompt_starting_with_slash_is_not_truncated")
            .build_with_session()
            .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    let prompt = "// hello this is a comment";
    client
        .prompt_text(session_id.clone(), prompt)
        .await
        .expect("prompt failed");

    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 1, "should have captured one request");

    let content = &captured[0].user_input_message.content;
    assert!(
        content.contains(prompt),
        "full prompt should reach the LLM, got: {:?}",
        content
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn default_agent_setting_used_as_initial_mode() {
    use agent::agent_config::definitions::AgentConfigV2025_08_22;

    let config = AgentConfigV2025_08_22 {
        name: "my_custom_agent".to_string(),
        description: Some("Custom test agent".to_string()),
        ..Default::default()
    };

    let (harness, client) = AcpTestHarnessBuilder::new("default_agent_setting_used_as_initial_mode")
        .with_agent_config("my_custom_agent", &config)
        .with_setting("chat.defaultAgent", "my_custom_agent")
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();
    let resp = client.new_session(cwd).await.expect("new_session failed");

    // Verify the default agent is used as the current mode
    let modes = resp.modes.expect("modes should be present in response");
    assert_eq!(
        modes.current_mode_id.0.as_ref(),
        "my_custom_agent",
        "current_mode_id should match chat.defaultAgent setting"
    );
}

/// Regression test: a startup `--agent` flag must bind every `session/new`, not
/// just the first. See the `resolve_agent_name` unit tests in `session_manager.rs`
/// for the detailed rationale.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn cli_agent_flag_applies_to_every_new_session() {
    use agent::agent_config::definitions::AgentConfigV2025_08_22;

    let pinned = AgentConfigV2025_08_22 {
        name: "docs-v2".to_string(),
        description: Some("Pinned via --agent".to_string()),
        ..Default::default()
    };
    let default_agent = AgentConfigV2025_08_22 {
        name: "kiro-dev".to_string(),
        description: Some("Configured default".to_string()),
        ..Default::default()
    };

    let (harness, client) = AcpTestHarnessBuilder::new("cli_agent_flag_applies_to_every_new_session")
        .with_agent_config("docs-v2", &pinned)
        .with_agent_config("kiro-dev", &default_agent)
        // Default differs from the pinned agent so a dropped flag would be visible
        // (the session would fall back to kiro-dev).
        .with_setting("chat.defaultAgent", "kiro-dev")
        // Startup --agent flag; must apply to every session/new for the subprocess.
        .with_acp_args(["--agent", "docs-v2"])
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();

    // Fire several sequential session/new calls; each must bind to docs-v2,
    // never falling back to chat.defaultAgent (kiro-dev).
    for i in 1..=3 {
        let resp = client.new_session(cwd.clone()).await.expect("new_session failed");
        let modes = resp.modes.expect("modes should be present in response");
        assert_eq!(
            modes.current_mode_id.0.as_ref(),
            "docs-v2",
            "session/new #{i} should bind to the --agent flag, not fall back to chat.defaultAgent",
        );
    }
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn set_mode_switches_agent() {
    use agent::agent_config::definitions::AgentConfigV2025_08_22;

    let swapped_config = AgentConfigV2025_08_22 {
        name: "swapped_agent".to_string(),
        global_prompt: Some("You are the swapped agent".to_string()),
        tools: vec!["read".to_string(), "write".to_string()],
        ..Default::default()
    };

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("set_mode_switches_agent")
        .with_agent_config("swapped_agent", &swapped_config)
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/two_simple_responses.jsonl")
        .await;

    // First prompt with default agent
    client
        .prompt_text(session_id.clone(), "first prompt")
        .await
        .expect("first prompt failed");

    // Switch to swapped agent
    client
        .set_session_mode(session_id.clone(), "swapped_agent".to_string())
        .await
        .expect("set_session_mode failed");

    // Second prompt with swapped agent
    client
        .prompt_text(session_id.clone(), "second prompt")
        .await
        .expect("second prompt failed");

    // Verify swapped agent config applied
    let captured = harness.get_captured_requests(&session_id.0).await;
    let req = &captured[1];
    let history = req.history.as_ref().expect("should have history");
    let history_text: String = history
        .iter()
        .map(|m| match m {
            chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(u) => u.content.clone(),
            chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(a) => a.content.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        history_text.contains("You are the swapped agent"),
        "should include swapped system prompt"
    );

    let tools = req
        .user_input_message
        .user_input_message_context
        .as_ref()
        .and_then(|ctx| ctx.tools.as_ref())
        .expect("should have tools");
    let tool_names: Vec<_> = tools
        .iter()
        .map(|t| match t {
            chat_cli_v2::api_client::model::Tool::ToolSpecification(spec) => spec.name.as_str(),
        })
        .collect();
    assert!(tool_names.contains(&"read"), "should have read tool");
    assert!(tool_names.contains(&"write"), "should have write tool");
    assert!(!tool_names.contains(&"shell"), "should NOT have shell tool after swap");

    // Verify conversation history preserved
    assert!(
        history_text.contains("first prompt"),
        "history should contain first prompt"
    );
    assert!(
        history_text.contains("Response before swap"),
        "history should contain first response"
    );

    // Verify switching to non-existent agent fails
    let result = client
        .set_session_mode(session_id.clone(), "nonexistent_agent".to_string())
        .await;
    assert!(result.is_err(), "set_session_mode should fail for non-existent agent");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn session_cancel_notification() {
    let (_, client, session_id, _) = AcpTestHarnessBuilder::new("session_cancel_notification")
        .build_with_session()
        .await;

    // Send cancel notification
    client.cancel(session_id).await.expect("cancel notification failed");

    // The test passes if no error occurs - the agent should handle the cancellation gracefully
}

#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn cancel_mid_stream_partial_response() {
    use chat_cli_v2::api_client::send_message_output::MockStreamItem;

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("cancel_mid_stream")
        .build_with_session()
        .await;

    harness
        .push_mock_response(
            &session_id.to_string(),
            Some(vec![
                MockStreamItem::Event(
                    amzn_codewhisperer_streaming_client::types::ChatResponseStream::AssistantResponseEvent({
                        AssistantResponseEventBuilder::default()
                            .content("This is a partial response...")
                            .build()
                            .expect("Failed to build mock response")
                    })
                    .into(),
                ),
                MockStreamItem::Event(
                    amzn_codewhisperer_streaming_client::types::ChatResponseStream::AssistantResponseEvent({
                        AssistantResponseEventBuilder::default()
                            .content(" more content")
                            .build()
                            .expect("Failed to build mock response")
                    })
                    .into(),
                ),
            ]),
        )
        .await;
    // Don't push None yet — we want cancel to fire while stream is still open

    let prompt_res_recv = client.prompt_text_async(session_id.clone(), "Hi").await;

    // Wait until streaming content arrives — guarantees agent is mid-stream
    client
        .wait_for(|n| {
            n.session_updates
                .iter()
                .any(|u| matches!(u, SessionUpdate::AgentMessageChunk(_)))
        })
        .await;

    // Fire cancel (non-blocking), then push end-of-stream to unblock the drain
    client.cancel_async(session_id.clone()).await;
    sleep(Duration::from_millis(50)).await;
    harness.push_mock_response(&session_id.to_string(), None).await;

    let prompt_res = prompt_res_recv
        .await
        .expect("Failed to receive prompt response")
        .expect("Failed to receive prompt response");
    assert_eq!(prompt_res.stop_reason, agent_client_protocol::StopReason::Cancelled);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "TODO: Test times out waiting for tool approval flow - likely issue with how cancellation interacts with pending tool approval state"]
#[timeout(30000)]
#[serial]
async fn cancel_during_tool_approval_allows_new_prompt() {
    use chat_cli_v2::api_client::model::ChatResponseStream;
    use chat_cli_v2::api_client::send_message_output::MockStreamItem;

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("cancel_during_approval")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Push a mock response with a tool use that requires approval
    harness
        .push_mock_response(
            &session_id.to_string(),
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                    content: "I'll read that file for you.".to_string(),
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tool-123".to_string(),
                    name: "write".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tool-123".to_string(),
                    name: "write".to_string(),
                    input: Some(r#"{"command":"create","path":"/tmp/test.txt","content":"hello"}"#.to_string()),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tool-123".to_string(),
                    name: "write".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    // End the stream so tool use is processed
    harness.push_mock_response(&session_id.to_string(), None).await;

    // Start a prompt - it will wait for tool approval
    let first_prompt = client.prompt_text_async(session_id.clone(), "Read a file").await;

    // Cancel while waiting for approval
    let cancel_res = client.cancel(session_id.clone()).await;
    assert!(cancel_res.is_ok());

    let first_prompt_res = first_prompt
        .await
        .expect("Failed to receive prompt response")
        .expect("Prompt should succeed");
    assert_eq!(
        first_prompt_res.stop_reason,
        agent_client_protocol::StopReason::Cancelled
    );

    // Now push a simple response for the second prompt
    harness
        .push_mock_response(
            &session_id.to_string(),
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Hello after cancel!".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&session_id.to_string(), None).await;

    // Verify we can send a new prompt after cancellation
    let second_prompt_res = client
        .prompt_text(session_id.clone(), "Hello again")
        .await
        .expect("Second prompt should succeed");
    assert_eq!(
        second_prompt_res.stop_reason,
        agent_client_protocol::StopReason::EndTurn
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn prompt_with_resource_link() {
    use agent_client_protocol as acp;

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("prompt_with_resource_link")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    // Send prompt with mixed Text and ResourceLink
    let content = vec![
        common::text_content("Please analyze this file:"),
        acp::ContentBlock::ResourceLink(
            acp::ResourceLink::new("auth.rs", "file:///test/project/auth.rs")
                .mime_type(Some("text/x-rust".to_string())),
        ),
        common::text_content("What security issues do you see?"),
    ];

    let result = client.prompt(session_id.clone(), content).await;
    assert!(result.is_ok(), "prompt with resource_link should succeed");

    // Verify the captured request contains the resource link info
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 1, "should have captured one request");

    let conv = &captured[0];
    let user_content = &conv.user_input_message.content;

    // The resource link should appear in the content (as JSON text)
    assert!(
        user_content.contains("file:///test/project/auth.rs"),
        "content should contain the resource link uri, got: {}",
        user_content
    );
}

#[tokio::test]
#[ignore = "TODO: times out in CI but passes locally, needs investigation"]
#[timeout(30000)]
#[serial]
async fn set_model_changes_model_id() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("set_model_changes_model_id")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/two_simple_responses.jsonl")
        .await;

    // First prompt with default model
    client
        .prompt_text(session_id.clone(), "first prompt")
        .await
        .expect("first prompt failed");

    // Switch to a different model
    client
        .set_session_model(session_id.clone(), "claude-opus-4.5".to_string())
        .await
        .expect("set_session_model failed");

    // Second prompt should use the new model
    client
        .prompt_text(session_id.clone(), "second prompt")
        .await
        .expect("second prompt failed");

    // Verify the second request used the new model_id
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 2, "should have two captured requests");

    let second_req = &captured[1];
    assert_eq!(
        second_req.user_input_message.model_id.as_deref(),
        Some("claude-opus-4.5"),
        "second request should use the new model_id"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn set_current_as_default_persists_model_to_settings() {
    // 1. Create a session and switch to a non-default model
    let (harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("set_current_as_default_persists")
        .build_with_session()
        .await;

    // Switch model in the current session
    client
        .set_session_model(session_id.clone(), "claude-opus-4.5".to_string())
        .await
        .expect("set_session_model failed");

    // 2. Send /model set-current-as-default as a slash command
    client
        .prompt_text(session_id.clone(), "/model set-current-as-default")
        .await
        .expect("set-current-as-default prompt failed");

    // 3. Verify the command response contains a success message
    let captured = client.captured().await;
    let has_success_message = captured.session_updates.iter().any(|update| {
        if let SessionUpdate::AgentMessageChunk(chunk) = update {
            let text = format!("{:?}", chunk);
            text.contains("default model")
        } else {
            false
        }
    });
    assert!(
        has_success_message,
        "Should have received a success message about setting default model"
    );

    // 4. Verify the setting was persisted to disk
    let settings_content = std::fs::read_to_string(&harness.paths.settings_path).expect("failed to read settings file");
    let settings: serde_json::Value = serde_json::from_str(&settings_content).expect("failed to parse settings");
    assert_eq!(
        settings.get("chat.defaultModel").and_then(|v| v.as_str()),
        Some("claude-opus-4.5"),
        "chat.defaultModel should be persisted to settings file"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn set_current_as_default_picked_up_by_new_session() {
    // Verify the full in-process flow: set default in session 1, create session 2,
    // confirm session 2 uses the persisted default without a process restart.
    let (mut harness, client, session_id, cwd) = AcpTestHarnessBuilder::new("set_default_new_session")
        .build_with_session()
        .await;

    // Switch model and persist as default
    client
        .set_session_model(session_id.clone(), "claude-opus-4.5".to_string())
        .await
        .expect("set_session_model failed");
    client
        .prompt_text(session_id.clone(), "/model set-current-as-default")
        .await
        .expect("set-current-as-default prompt failed");

    // Create a new session — it should pick up the persisted default model
    let second_resp = client.new_session(cwd).await.expect("second new_session failed");
    let second_session_id = second_resp.session_id;

    harness
        .push_mock_responses_from_file(&second_session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    // Send a prompt in the new session to trigger a request
    client
        .prompt_text(second_session_id.clone(), "hello")
        .await
        .expect("prompt in second session failed");

    // Verify the new session used the persisted default model
    let captured_requests = harness.get_captured_requests(&second_session_id.0).await;
    assert_eq!(
        captured_requests.len(),
        1,
        "should have one captured request in new session"
    );
    assert_eq!(
        captured_requests[0].user_input_message.model_id.as_deref(),
        Some("claude-opus-4.5"),
        "new session should use the persisted default model"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn prompt_with_image() {
    use agent_client_protocol as acp;
    use base64::Engine;

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("prompt_with_image")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    // Create a minimal valid PNG (1x1 red pixel)
    let png_bytes: [u8; 70] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // PNG signature
        0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, // IHDR chunk header
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1 dimensions
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, // bit depth, color type, CRC
        0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, // IDAT chunk header
        0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00, // compressed data
        0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xfe, // more data + CRC
        0xd4, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, // IEND chunk
        0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let base64_data = base64::engine::general_purpose::STANDARD.encode(png_bytes);

    let content = vec![
        common::text_content("What's in this image?"),
        acp::ContentBlock::Image(acp::ImageContent::new(base64_data, "image/png".to_string())),
    ];

    let result = client.prompt(session_id.clone(), content).await;
    assert!(result.is_ok(), "prompt with image should succeed");

    // Verify the captured request contains image data
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 1, "should have captured one request");

    let conv = &captured[0];
    let images = &conv.user_input_message.images;
    assert!(
        images.is_some() && !images.as_ref().unwrap().is_empty(),
        "request should contain images"
    );
}

#[tokio::test]
#[timeout(120000)]
#[serial]
async fn http_mcp_server_tool_call_triggers_permission_request() {
    use mock_mcp_server::{
        MockMcpServerBuilder,
        MockResponse,
        ToolDef,
    };
    use sacp::schema::McpServerHttp;

    mock_mcp_server::prebuild_bin().expect("mock mcp server build failed");

    // Start mock MCP server
    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "echo".to_string(),
            description: "Echoes back the input message".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            }),
        })
        .add_response(MockResponse {
            tool: "echo".to_string(),
            input_match: None,
            response: serde_json::json!({"echoed": "hello from mcp"}),
        })
        .spawn_http()
        .expect("failed to spawn mock MCP server");

    // Wait for server to be ready
    handle
        .wait_ready(Duration::from_secs(10))
        .expect("mock MCP server not ready");

    let (harness, client) = AcpTestHarnessBuilder::new("mcp_tool_call")
        .with_trust_all(true)
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();

    // Create session with MCP server
    let mcp_server = sacp::schema::McpServer::Http(McpServerHttp::new("test-mcp", handle.url()));

    let resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");
    let session_id = resp.session_id;

    // Wait for MCP server to initialize before prompting
    let mcp_initialized_method = methods::MCP_SERVER_INITIALIZED
        .strip_prefix("_")
        .expect("method should have underscore prefix");

    let initialized = client
        .wait_for_timeout(
            |captured| {
                captured.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == mcp_initialized_method && {
                        let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                        params.get("serverName").and_then(|v| v.as_str()) == Some("test-mcp")
                    }
                })
            },
            std::time::Duration::from_secs(90),
        )
        .await;
    assert!(initialized, "MCP server 'test-mcp' did not initialize within 90s");

    let mut harness = harness;
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/mcp_tool_call.jsonl")
        .await;

    // Send prompt that will trigger MCP tool call
    client
        .prompt_text(session_id.clone(), "echo hello")
        .await
        .expect("prompt failed");

    // Check that we received a permission request for the MCP tool
    let captured = client.captured().await;
    let has_tool_call = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCall(tc) if tc.title.contains("echo")));

    assert!(has_tool_call, "should have received a tool call for 'echo'");

    // Verify permission was requested
    assert!(
        !captured.permission_requests.is_empty(),
        "should have received permission requests"
    );
}

#[tokio::test]
#[timeout(120000)]
#[serial]
async fn mcp_stdio_server_tool_call() {
    // MCP stdio handshake is unreliable on CI runners under load
    if std::env::var("CI").is_ok() {
        return;
    }

    use std::path::PathBuf;

    use mock_mcp_server::prebuild_bin;
    use sacp::schema::McpServerStdio;

    // Ensure mock-mcp-server binary is built
    let binary_path = prebuild_bin().expect("failed to build mock-mcp-server");

    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/stdio_server.jsonl");

    let (harness, client) = AcpTestHarnessBuilder::new("mcp_stdio")
        .with_trust_all(true)
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();

    // Create session with stdio MCP server
    let mcp_server = sacp::schema::McpServer::Stdio(
        McpServerStdio::new("test-stdio-mcp", binary_path)
            .args(vec!["--config".to_string(), config_path.to_str().unwrap().to_string()]),
    );

    let resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");
    let session_id = resp.session_id;

    // Wait for MCP server to initialize before prompting
    let mcp_initialized_method = methods::MCP_SERVER_INITIALIZED
        .strip_prefix("_")
        .expect("method should have underscore prefix");

    let initialized = client
        .wait_for_timeout(
            |captured| {
                captured.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == mcp_initialized_method && {
                        let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                        params.get("serverName").and_then(|v| v.as_str()) == Some("test-stdio-mcp")
                    }
                })
            },
            std::time::Duration::from_secs(90),
        )
        .await;
    assert!(initialized, "MCP server 'test-stdio-mcp' did not initialize within 90s");

    let mut harness = harness;
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/mcp_stdio_tool_call.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "greet someone")
        .await
        .expect("prompt failed");

    let captured = client.captured().await;
    let has_tool_call = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCall(tc) if tc.title.contains("greet")));

    assert!(has_tool_call, "should have received a tool call for 'greet'");
}

#[tokio::test]
#[timeout(120000)]
#[serial]
async fn http_mcp_server_oauth_request_triggers_ext_notification() {
    use mock_mcp_server::{
        MockMcpServerBuilder,
        ToolDef,
    };
    use sacp::schema::McpServerHttp;

    mock_mcp_server::prebuild_bin().expect("mock mcp server build failed");

    // Start mock MCP server that returns 401 on probe request
    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "echo".to_string(),
            description: "Echoes back the input message".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                }
            }),
        })
        .probe_status(401) // Return 401 to trigger OAuth flow
        .spawn_http()
        .expect("failed to spawn mock MCP server");

    // Wait for server to be ready
    handle
        .wait_ready(Duration::from_secs(10))
        .expect("mock MCP server not ready");

    let (harness, client) = AcpTestHarnessBuilder::new("mcp_oauth_request").build().await;

    let cwd = harness.paths.cwd.clone();

    // Create session with MCP server that requires OAuth
    let mcp_server = sacp::schema::McpServer::Http(McpServerHttp::new("test-oauth-mcp", handle.url()));

    let _resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");

    let oauth_method_name = methods::MCP_OAUTH_REQUEST
        .strip_prefix("_")
        .expect("failed to strip prefix from method");

    // Wait for OAuth notification to arrive - the OAuth flow involves:
    // 1. Probe request (returns 401)
    // 2. OAuth metadata discovery
    // 3. Authorization URL generation
    // 4. Event emission and forwarding
    client
        .wait_for(|captured| {
            captured
                .ext_notifications
                .iter()
                .any(|n| n.method.as_ref() == oauth_method_name)
        })
        .await;

    let captured = client.captured().await;
    let oauth_notifications: Vec<_> = captured
        .ext_notifications
        .iter()
        .filter(|n| n.method.as_ref() == oauth_method_name)
        .collect();

    assert!(
        !oauth_notifications.is_empty(),
        "should have received OAuth request extension notification, got ext_notifications: {:?}",
        captured.ext_notifications
    );

    // Verify the notification contains expected fields
    let oauth_notif = oauth_notifications.first().unwrap();
    let params: serde_json::Value = serde_json::from_str(oauth_notif.params.get()).unwrap();
    assert!(
        params.get("serverName").is_some(),
        "OAuth notification should contain serverName"
    );
    assert!(
        params.get("oauthUrl").is_some(),
        "OAuth notification should contain oauthUrl"
    );
}

/// End-to-end: initial OAuth flow against a **registry-sourced** MCP server.
///
/// This drives the full stack — the real `chat_cli acp` subprocess, the ACP
/// protocol, and `chat_cli_v2`'s real `RegistryAdapter` — rather than the agent
/// crate in isolation. A registry is served over HTTP and pointed at via
/// `KIRO_MCP_REGISTRY_URL_OVERRIDE`; it resolves a `"type": "registry"` entry in
/// the session's agent config into the OAuth-protected mock server. The test
/// plays the user's browser to complete the handshake (fetching the
/// authorization URL, which 302-redirects to the CLI's loopback listener with
/// the auth code), then asserts via `/mcp` that the resolved server comes up
/// running with its tool — proving the handshake completed end-to-end.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn e2e_oauth_registry_resolved_server_completes_handshake() {
    use agent::agent_config::definitions::{
        AgentConfigV2025_08_22,
        McpServerConfig,
        RegistryMcpServerConfig,
    };
    use mock_mcp_server::{
        MockMcpServerBuilder,
        MockResponse,
        ToolDef,
        prebuild_bin,
    };

    prebuild_bin().expect("mock mcp server build failed");

    // 1. OAuth-protected MCP server exposing a single `echo` tool.
    let mcp = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "echo".to_string(),
            description: "Echoes back the input".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        })
        .add_response(MockResponse {
            tool: "echo".to_string(),
            input_match: None,
            response: serde_json::json!({"echoed": true}),
        })
        .oauth()
        .spawn_http()
        .expect("failed to spawn oauth mock server");
    mcp.wait_ready(Duration::from_secs(10)).expect("mock server not ready");

    // 2. Registry that resolves the server name to the OAuth mock's URL.
    let registry_body = serde_json::json!({
        "servers": [{
            "server": {
                "name": "oauth-registry-mcp",
                "description": "OAuth-protected registry server",
                "version": "1.0.0",
                "remotes": [{ "type": "streamable-http", "url": mcp.url() }]
            }
        }]
    })
    .to_string();
    let (registry_url, registry_task) = common::spawn_mock_registry(registry_body).await;

    // 3. Agent whose only MCP server is a registry placeholder; the real RegistryAdapter resolves it
    //    against the registry above.
    let agent_config = AgentConfigV2025_08_22 {
        name: "oauth_registry_agent".to_string(),
        mcp_servers: [(
            "oauth-registry-mcp".to_string(),
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

    let (mut harness, client) = AcpTestHarnessBuilder::new("e2e_oauth_registry")
        .with_agent_config("oauth_registry_agent", &agent_config)
        .with_setting("chat.defaultAgent", "oauth_registry_agent")
        .with_env("KIRO_MCP_REGISTRY_URL_OVERRIDE", &registry_url)
        .with_trust_all(true)
        .build()
        .await;

    let oauth_method = methods::MCP_OAUTH_REQUEST
        .strip_prefix('_')
        .expect("oauth method should have a leading underscore")
        .to_string();

    // 4. Browser driver: complete every OAuth request by fetching its authorization URL. Runs
    //    concurrently so it can satisfy a handshake that `new_session` may block on while the server
    //    initializes.
    let driver_client = client.clone();
    let driver_method = oauth_method.clone();
    let browser = tokio::spawn(async move {
        let http = reqwest::Client::new();
        let mut driven = std::collections::HashSet::new();
        loop {
            let captured = driver_client.captured().await;
            for n in &captured.ext_notifications {
                if n.method.as_ref() != driver_method {
                    continue;
                }
                let Ok(params) = serde_json::from_str::<serde_json::Value>(n.params.get()) else {
                    continue;
                };
                if let Some(oauth_url) = params.get("oauthUrl").and_then(|v| v.as_str())
                    && driven.insert(oauth_url.to_string())
                {
                    let _ = http.get(oauth_url).send().await;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    // 5. Create the session (launches the registry-resolved OAuth server).
    let session_id = client
        .new_session(harness.paths.cwd.clone())
        .await
        .expect("new_session failed")
        .session_id;

    // 6. Confirm an OAuth request was actually issued for the registry server.
    let saw_oauth_request = client
        .wait_for_timeout(
            |c| {
                c.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == oauth_method
                        && serde_json::from_str::<serde_json::Value>(n.params.get())
                            .ok()
                            .and_then(|p| p.get("serverName").and_then(|v| v.as_str().map(String::from)))
                            == Some("oauth-registry-mcp".to_string())
                })
            },
            Duration::from_secs(30),
        )
        .await;
    assert!(
        saw_oauth_request,
        "expected an OAuth request for the registry-resolved server"
    );

    // 7. After the browser completes the handshake, the server must come up running and expose its
    //    tool. Poll `/mcp` until it does.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let mut last_data = serde_json::Value::Null;
    let running = loop {
        let result = client
            .execute_command(session_id.clone(), serde_json::json!({"command": "mcp", "args": {}}))
            .await
            .expect("/mcp command failed");

        if let Some(data) = result.data.clone() {
            let is_running = data.get("servers").and_then(|s| s.as_array()).is_some_and(|servers| {
                servers.iter().any(|srv| {
                    srv.get("name").and_then(|v| v.as_str()) == Some("oauth-registry-mcp")
                        && srv.get("status").and_then(|v| v.as_str()) == Some("running")
                        && srv.get("toolCount").and_then(|v| v.as_u64()).unwrap_or(0) >= 1
                })
            });
            last_data = data;
            if is_running {
                break true;
            }
        }

        if tokio::time::Instant::now() >= deadline {
            break false;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    browser.abort();
    registry_task.abort();

    assert!(
        running,
        "registry-resolved OAuth server never reached 'running' with its tool; last /mcp data: {last_data}"
    );

    let _ = &mut harness;
}

/// Test that agent swap properly unloads old MCP servers and loads new ones.
///
/// This test uses unique tool/server names (swap_test_*) to avoid conflicts with
/// other tests that may run in parallel. The process check at the end greps for
/// the full command including these unique config file names.
#[tokio::test]
#[ignore = "TODO: MCP tool execution fails - ToolCallUpdate with Completed status not received, possibly MCP server initialization or tool result handling issue"]
#[timeout(30000)]
#[serial]
async fn agent_swap_reloads_mcp_servers() {
    // MCP stdio handshake is unreliable on CI runners under load
    if std::env::var("CI").is_ok() {
        return;
    }

    use std::path::PathBuf;

    use agent::agent_config::definitions::{
        AgentConfigV2025_08_22,
        LocalMcpServerConfig,
        McpServerConfig,
    };
    use agent_client_protocol::SessionUpdate;
    use mock_mcp_server::prebuild_bin;

    // Ensure mock-mcp-server binary is built
    let binary_path = prebuild_bin().expect("failed to build mock-mcp-server");

    let config_a_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/swap_test_agent_a.jsonl");
    let config_b_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/swap_test_agent_b.jsonl");

    // Create agent A with MCP server using swap_test_tool_a
    let agent_a_config = AgentConfigV2025_08_22 {
        name: "swap_test_agent_a".to_string(),
        mcp_servers: [(
            "swap_test_mcp_a".to_string(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: binary_path.to_str().unwrap().to_string(),
                args: vec!["--config".to_string(), config_a_path.to_str().unwrap().to_string()],
                env: None,
                timeout_ms: 30000,
                disabled: false,
                disabled_tools: Vec::new(),
            }),
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };

    // Create agent B with MCP server using swap_test_tool_b
    let agent_b_config = AgentConfigV2025_08_22 {
        name: "swap_test_agent_b".to_string(),
        mcp_servers: [(
            "swap_test_mcp_b".to_string(),
            McpServerConfig::Local(LocalMcpServerConfig {
                command: binary_path.to_str().unwrap().to_string(),
                args: vec!["--config".to_string(), config_b_path.to_str().unwrap().to_string()],
                env: None,
                timeout_ms: 30000,
                disabled: false,
                disabled_tools: Vec::new(),
            }),
        )]
        .into_iter()
        .collect(),
        ..Default::default()
    };

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("agent_swap_mcp")
        .with_agent_config("swap_test_agent_a", &agent_a_config)
        .with_agent_config("swap_test_agent_b", &agent_b_config)
        .with_setting("chat.defaultAgent", "swap_test_agent_a")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Wait for MCP server A to initialize by polling for the initialized notification
    let mcp_initialized_method = methods::MCP_SERVER_INITIALIZED
        .strip_prefix("_")
        .expect("method should have underscore prefix");

    client
        .wait_for(|captured| {
            captured.ext_notifications.iter().any(|n| {
                n.method.as_ref() == mcp_initialized_method && {
                    let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                    params.get("serverName").and_then(|v| v.as_str()) == Some("swap_test_mcp_a")
                }
            })
        })
        .await;

    // Push mock response that triggers tool A call
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/swap_test_tool_a_call.jsonl")
        .await;

    // Send prompt to trigger tool A
    client
        .prompt_text(session_id.clone(), "call tool a")
        .await
        .expect("first prompt failed");

    // Verify tool A was called
    let captured = client.captured().await;
    let has_tool_a_call = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCall(tc) if tc.title.contains("swap_test_tool_a")));
    assert!(has_tool_a_call, "should have called swap_test_tool_a");

    // Verify tool A completed successfully
    let tool_a_completed = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCallUpdate(update) if update.fields.status == Some(ToolCallStatus::Completed)));
    assert!(tool_a_completed, "tool A should have completed successfully");

    client.clear_captured().await;

    // Swap to agent B
    client
        .set_session_mode(session_id.clone(), "swap_test_agent_b".to_string())
        .await
        .expect("set_session_mode failed");

    // Wait for MCP server B to initialize by polling for the initialized notification
    client
        .wait_for(|captured| {
            captured.ext_notifications.iter().any(|n| {
                n.method.as_ref() == mcp_initialized_method && {
                    let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                    params.get("serverName").and_then(|v| v.as_str()) == Some("swap_test_mcp_b")
                }
            })
        })
        .await;

    // Push mock response that triggers tool B call
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/swap_test_tool_b_call.jsonl")
        .await;

    // Send prompt to trigger tool B
    client
        .prompt_text(session_id.clone(), "call tool b")
        .await
        .expect("second prompt failed");

    // Verify tool B was called
    let captured = client.captured().await;
    let has_tool_b_call = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCall(tc) if tc.title.contains("swap_test_tool_b")));
    assert!(has_tool_b_call, "should have called swap_test_tool_b");

    // Verify tool B completed successfully
    let tool_b_completed = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCallUpdate(update) if update.fields.status == Some(ToolCallStatus::Completed)));
    assert!(tool_b_completed, "tool B should have completed successfully");

    // Verify no orphaned MCP server processes for agent A
    // We grep for the unique config file name to avoid matching other tests
    #[cfg(unix)]
    {
        let ps_output = std::process::Command::new("pgrep")
            .args(["-f", "swap_test_agent_a.jsonl"])
            .output()
            .expect("failed to run pgrep");

        assert!(
            ps_output.stdout.is_empty(),
            "should have no orphaned MCP server processes for agent A after swap"
        );
    }
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn auto_compaction_on_context_overflow() {
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("auto_compaction_on_context_overflow")
        .with_trust_all(true)
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/context_window_overflow.jsonl")
        .await;

    // Prompt triggers tool call, then overflow, then compaction + retry
    let result = client.prompt_text(session_id.clone(), "list files").await;
    assert!(result.is_ok(), "prompt should succeed after compaction");

    // Verify at least 2 requests with tool uses (original + retry after compaction)
    let captured = harness.get_captured_requests(&session_id.0).await;
    let requests_with_tool_use = captured
        .iter()
        .filter(|r| {
            r.history
                .as_ref()
                .is_some_and(|h: &Vec<chat_cli_v2::api_client::model::ChatMessage>| {
                    h.iter().any(|m| match m {
                        chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(a) => {
                            a.tool_uses.as_ref().is_some_and(|tu| !tu.is_empty())
                        },
                        chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(_) => false,
                    })
                })
        })
        .count();
    assert!(
        requests_with_tool_use >= 2,
        "expected at least 2 requests with tool uses (original + retry), found {}",
        requests_with_tool_use
    );

    // Verify compaction summary appears in history after compaction
    let has_summary = captured.iter().any(|r| {
        r.history
            .as_ref()
            .is_some_and(|h: &Vec<chat_cli_v2::api_client::model::ChatMessage>| {
                h.iter().any(|m| match m {
                    chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(u) => {
                        u.content.contains("SUMMARY CONTENT:")
                    },
                    chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(_) => false,
                })
            })
    });
    assert!(has_summary, "expected compaction summary in history after overflow");

    // Wait for async notifications to arrive
    sleep(Duration::from_millis(100)).await;

    // Verify compaction status notifications were sent
    // Note: ACP SDK strips leading underscore from method names
    let captured = client.captured().await;
    let compaction_notifications: Vec<_> = captured
        .ext_notifications
        .iter()
        .filter(|n| n.method.as_ref() == "kiro.dev/compaction/status")
        .collect();

    assert!(
        compaction_notifications
            .iter()
            .any(|n| n.params.get().contains("\"type\":\"started\"")),
        "expected compaction started notification"
    );
    assert!(
        compaction_notifications
            .iter()
            .any(|n| n.params.get().contains("\"type\":\"completed\"")),
        "expected compaction completed notification"
    );

    // Verify telemetry events (poll to allow async pipeline to flush)
    let events = harness
        .wait_for_telemetry_events(Duration::from_secs(5), |events| {
            let add_msgs = events
                .iter()
                .filter(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }))
                .count();
            let turn_completions = events
                .iter()
                .filter(|e| {
                    matches!(
                        &e.ty,
                        chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { .. }
                    )
                })
                .count();
            add_msgs >= 2 && turn_completions >= 1
        })
        .await;

    // At least 2 addChatMessage events (original tool call response + retry after compaction)
    let add_msgs: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }))
        .collect();
    assert!(
        add_msgs.len() >= 2,
        "expected at least 2 addChatMessage events, got {}",
        add_msgs.len()
    );

    // One messageResponseError for the context overflow
    let err_event = events
        .iter()
        .find(|e| {
            matches!(
                &e.ty,
                chat_cli_v2::telemetry::core::EventType::MessageResponseError { .. }
            )
        })
        .expect("expected messageResponseError for context overflow");
    if let chat_cli_v2::telemetry::core::EventType::MessageResponseError { reason, .. } = &err_event.ty {
        assert_eq!(reason.as_deref(), Some("ContextWindowOverflow"));
    }

    // recordUserTurnCompletion with Succeeded (compaction recovered)
    let turn = events
        .iter()
        .find(|e| {
            matches!(
                &e.ty,
                chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { .. }
            )
        })
        .expect("expected recordUserTurnCompletion event");
    if let chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { result, .. } = &turn.ty {
        assert_eq!(*result, chat_cli_v2::telemetry::TelemetryResult::Succeeded);
    }

    // toolUseSuggested for the ls tool call
    let tool_event = events
        .iter()
        .find(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ToolUseSuggested { .. }))
        .expect("expected toolUseSuggested event");
    if let chat_cli_v2::telemetry::core::EventType::ToolUseSuggested { tool_name, .. } = &tool_event.ty {
        assert_eq!(tool_name.as_deref(), Some("read"));
    }
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn str_replace_tool_call_includes_location_with_line_number() {
    use agent_client_protocol::SessionUpdate;

    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("str_replace_location")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Create a test file with content where "old_value" is on line 3 (1-indexed)
    // Use /tmp/ since the agent's cwd validation happens before tool execution
    let test_file = std::path::Path::new("/tmp/str_replace_test.txt");
    tokio::fs::write(&test_file, "line one\nline two\nold_value here\nline four\n")
        .await
        .expect("failed to create test file");

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/str_replace_single.jsonl")
        .await;

    client
        .prompt_text(
            session_id.clone(),
            "replace old_value with new_value in /tmp/str_replace_test.txt",
        )
        .await
        .expect("prompt failed");

    let captured = client.captured().await;

    // ToolCall event should have locations with line number (system hook completes before ToolCall is
    // emitted)
    let tool_call = captured
        .session_updates
        .iter()
        .find_map(|u| match u {
            SessionUpdate::ToolCall(tc) => Some(tc),
            _ => None,
        })
        .expect("should have a ToolCall");

    assert!(!tool_call.locations.is_empty(), "ToolCall should have locations");
    assert_eq!(
        tool_call.locations[0].line,
        Some(3),
        "ToolCall location line should be 3 (1-indexed)"
    );

    // ToolCallUpdate (when tool finishes) should also have locations
    let tool_call_update = captured
        .session_updates
        .iter()
        .find_map(|u| match u {
            SessionUpdate::ToolCallUpdate(update) if update.fields.status == Some(ToolCallStatus::Completed) => {
                Some(update)
            },
            _ => None,
        })
        .expect("should have a completed ToolCallUpdate");

    let update_locations = tool_call_update
        .fields
        .locations
        .as_ref()
        .expect("ToolCallUpdate should have locations");

    assert!(
        !update_locations.is_empty(),
        "ToolCallUpdate locations should not be empty"
    );
    assert_eq!(
        update_locations[0].line,
        Some(3),
        "ToolCallUpdate location line should be 3 (1-indexed)"
    );

    // Cleanup
    let _ = tokio::fs::remove_file(test_file).await;
}

#[tokio::test]
#[ignore = "broken, needs to be fixed"]
#[timeout(30000)]
#[serial]
async fn context_usage_flows_to_user_turn_metadata() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("context_usage_flows_to_user_turn_metadata")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/context_usage.jsonl")
        .await;

    // Send a prompt
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    // Read agent log and verify context usage was received and sent
    let log_path = harness.paths.log_file.clone();
    let log_content = tokio::fs::read_to_string(&log_path).await.expect("read agent log");

    assert!(
        log_content.contains("context_usage_percentage: Some(42.5)"),
        "UserTurnMetadata should contain context_usage_percentage"
    );
    assert!(
        log_content.contains("Sending context usage notification"),
        "Should send context usage notification"
    );
}

/// Helper to extract tool call title from a prompt response
async fn get_tool_call_title(
    harness: &mut AcpTestHarness,
    client: &AcpTestClient,
    session_id: &agent_client_protocol::SessionId,
    mock_file: &str,
    prompt: &str,
) -> String {
    harness.push_mock_responses_from_file(&session_id.0, mock_file).await;

    client
        .prompt_text(session_id.clone(), prompt)
        .await
        .expect("prompt failed");

    let captured = client.captured().await;
    captured
        .session_updates
        .iter()
        .find_map(|u| match u {
            SessionUpdate::ToolCall(tc) => Some(tc.title.clone()),
            _ => None,
        })
        .expect("should have a ToolCall")
}

/// Helper to extract tool call locations from a prompt response
async fn get_tool_call_locations(
    harness: &mut AcpTestHarness,
    client: &AcpTestClient,
    session_id: &agent_client_protocol::SessionId,
    mock_file: &str,
    prompt: &str,
) -> Vec<agent_client_protocol::ToolCallLocation> {
    harness.push_mock_responses_from_file(&session_id.0, mock_file).await;

    client
        .prompt_text(session_id.clone(), prompt)
        .await
        .expect("prompt failed");

    let captured = client.captured().await;
    captured
        .session_updates
        .iter()
        .find_map(|u| match u {
            SessionUpdate::ToolCall(tc) => Some(tc.locations.clone()),
            _ => None,
        })
        .expect("should have a ToolCall")
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn tool_call_has_descriptive_title_fs_read() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("tool_fs_read").build_with_session().await;

    let test_file = harness.paths.cwd.join("test_file.txt");
    tokio::fs::write(&test_file, "test content")
        .await
        .expect("failed to create test file");

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/tool_fs_read.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "read the test file")
        .await
        .expect("prompt failed");

    let captured = client.captured().await;
    // Debug: print all captured session updates
    for (i, update) in captured.session_updates.iter().enumerate() {
        eprintln!("Update {}: {:?}", i, update);
    }

    let title = captured
        .session_updates
        .iter()
        .find_map(|u| match u {
            SessionUpdate::ToolCall(tc) => Some(tc.title.clone()),
            _ => None,
        })
        .expect("should have a ToolCall");

    assert_eq!(title, "Reading test_file.txt:11-60");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn tool_call_has_descriptive_title_fs_write_create() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("tool_fs_write_create")
        .with_trust_all(true)
        .build_with_session()
        .await;

    let title = get_tool_call_title(
        &mut harness,
        &client,
        &session_id,
        "tests/mock_responses/tool_fs_write_create.jsonl",
        "create a new file",
    )
    .await;

    assert_eq!(title, "Creating new_file.txt");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn tool_call_has_descriptive_title_grep() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("tool_grep")
        .with_trust_all(true)
        .build_with_session()
        .await;

    let title = get_tool_call_title(
        &mut harness,
        &client,
        &session_id,
        "tests/mock_responses/tool_grep.jsonl",
        "search for TODO",
    )
    .await;

    assert_eq!(title, "Searching for 'TODO'");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn tool_call_has_descriptive_title_execute_bash() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("tool_execute_bash")
        .with_trust_all(true)
        .build_with_session()
        .await;

    let title = get_tool_call_title(
        &mut harness,
        &client,
        &session_id,
        "tests/mock_responses/tool_execute_bash.jsonl",
        "run echo hello",
    )
    .await;

    assert_eq!(title, "Running: echo hello world");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn tool_call_has_locations_fs_read_multiple() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("tool_fs_read_multi")
        .build_with_session()
        .await;

    tokio::fs::write(harness.paths.cwd.join("file1.txt"), "content1")
        .await
        .unwrap();
    tokio::fs::write(harness.paths.cwd.join("file2.txt"), "content2")
        .await
        .unwrap();

    let locations = get_tool_call_locations(
        &mut harness,
        &client,
        &session_id,
        "tests/mock_responses/tool_fs_read_multi.jsonl",
        "read both files",
    )
    .await;

    assert_eq!(locations.len(), 2);
    assert_eq!(&locations[0].path, "file1.txt");
    assert_eq!(&locations[1].path, "file2.txt");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn tool_call_has_locations_fs_read_with_line() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("tool_fs_read_offset")
        .build_with_session()
        .await;

    let test_file = harness.paths.cwd.join("test_file.txt");
    tokio::fs::write(
        &test_file,
        "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10",
    )
    .await
    .unwrap();

    let locations = get_tool_call_locations(
        &mut harness,
        &client,
        &session_id,
        "tests/mock_responses/tool_fs_read_offset.jsonl",
        "read from line 10",
    )
    .await;

    assert_eq!(locations.len(), 1);
    assert_eq!(&locations[0].path, "test_file.txt");
    assert_eq!(locations[0].line, Some(10)); // offset 9 -> line 10
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn fs_read_in_cwd_does_not_require_permission() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("fs_read_cwd_permission")
        .build_with_session()
        .await;

    let test_file = harness.paths.cwd.join("test_file.txt");
    tokio::fs::write(&test_file, "test content")
        .await
        .expect("failed to create test file");

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/tool_fs_read.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "read the test file")
        .await
        .expect("prompt failed");

    let captured = client.captured().await;
    assert!(
        captured.permission_requests.is_empty(),
        "fs_read in CWD should not require permission, got: {:?}",
        captured.permission_requests
    );

    // Verify telemetry events were emitted (poll to allow async pipeline to flush)
    let events = harness
        .wait_for_telemetry_events(Duration::from_secs(5), |events| {
            let add_msgs = events
                .iter()
                .filter(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }))
                .count();
            let turn_completions = events
                .iter()
                .filter(|e| {
                    matches!(
                        &e.ty,
                        chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { .. }
                    )
                })
                .count();
            add_msgs >= 2 && turn_completions >= 1
        })
        .await;

    let add_chat_msgs: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }))
        .collect();
    assert_eq!(
        add_chat_msgs.len(),
        2,
        "expected 2 addChatMessage events (tool use + text response)"
    );

    // All events should have app_type = "V2" (no client_info sent, but default is ACP — however
    // the TUI sends client_info in initialize, so this depends on the test client)
    for event in &events {
        assert!(event.app_type.is_some(), "app_type should be set on all events");
    }

    let tool_use_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ToolUseSuggested { .. }))
        .collect();
    assert_eq!(tool_use_events.len(), 1, "expected 1 toolUseSuggested event");
    if let chat_cli_v2::telemetry::core::EventType::ToolUseSuggested {
        tool_name,
        is_accepted,
        is_success,
        ..
    } = &tool_use_events[0].ty
    {
        assert_eq!(tool_name.as_deref(), Some("read"));
        assert!(is_accepted, "tool should be accepted");
        assert_eq!(*is_success, Some(true), "tool should succeed");
    }

    let turn_completions: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                &e.ty,
                chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { .. }
            )
        })
        .collect();
    assert_eq!(turn_completions.len(), 1, "expected 1 recordUserTurnCompletion event");
    if let chat_cli_v2::telemetry::core::EventType::RecordUserTurnCompletion { result, args, .. } =
        &turn_completions[0].ty
    {
        assert_eq!(*result, chat_cli_v2::telemetry::TelemetryResult::Succeeded);
        assert!(args.reason.is_none(), "successful turn should have no reason");
    }
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn permissions_are_applied_and_loaded() {
    let (mut harness, client, session_id, cwd) = AcpTestHarnessBuilder::new("permissions_are_applied_and_loaded")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(
            &session_id.0,
            "tests/mock_responses/test_read_write_permissioning.jsonl",
        )
        .await;

    // Queue permission responses:
    // 1. First write (create hello.py) - allow always
    client
        .queue_permission_response(PermissionResponse::Select("allow_always".to_string()))
        .await;
    // 2. First read (hello.py) - auto-allowed (reads under cwd don't need permission)
    // 3. Second write (update hello.py) - auto-allowed due to allow_always

    let result = client
        .prompt_text(
            session_id.clone(),
            "Write hello.py, read it, update it, then read it again",
        )
        .await
        .expect("prompt failed");

    assert_eq!(result.stop_reason, agent_client_protocol::StopReason::EndTurn);

    // Verify we got exactly 1 permission request (first write only, second auto-allowed)
    let captured = client.captured().await;
    assert_eq!(
        captured.permission_requests.len(),
        1,
        "Expected 1 permission request (first write only)"
    );

    // Clear client state before loading session
    client.clear_captured().await;

    // Load the session - permissions should be restored
    client
        .load_session(session_id.clone(), cwd)
        .await
        .expect("load_session failed");

    // Push mock response for the write attempt after reload
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/write_hello_py.jsonl")
        .await;

    // Attempt to write hello.py - should be auto-allowed due to persisted allow_always
    let result = client
        .prompt_text(session_id.clone(), "Write hello.py")
        .await
        .expect("prompt failed");

    assert_eq!(result.stop_reason, agent_client_protocol::StopReason::EndTurn);

    // Verify no new permission requests (write was auto-allowed from persisted permissions)
    let captured = client.captured().await;
    assert_eq!(
        captured.permission_requests.len(),
        0,
        "Expected no permission requests - write should be auto-allowed from persisted permissions"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn session_list_returns_sessions_with_title() {
    let (mut harness, client) = AcpTestHarnessBuilder::new("session_list_returns_sessions_with_title")
        .with_trust_all(true)
        .build()
        .await;
    let cwd = harness.paths.cwd.clone();

    // Create 3 sessions with prompts, small delays between them for distinct timestamps.
    let prompts = ["First session prompt", "Second session prompt", "Third session prompt"];
    let mut session_ids = Vec::new();
    for prompt in &prompts {
        let resp = client.new_session(cwd.clone()).await.expect("new_session failed");
        let sid = resp.session_id.clone();

        harness
            .push_mock_responses_from_file(&sid.0, "tests/mock_responses/write_hello_world_in_bash.jsonl")
            .await;
        client.prompt_text(sid.clone(), prompt).await.expect("prompt failed");
        session_ids.push(sid);
        sleep(Duration::from_millis(50)).await;
    }

    let result = client.list_sessions(cwd.clone()).await.expect("list_sessions failed");
    assert_eq!(result.sessions.len(), 3, "should have exactly 3 sessions");

    // Results are most→least recent, so reverse of creation order.
    // Walk both iterators in lockstep to verify ordering, titles, and metadata.
    for (entry, (sid, prompt)) in result
        .sessions
        .iter()
        .zip(session_ids.iter().rev().zip(prompts.iter().rev()))
    {
        assert_eq!(entry.session_id, sid.0.as_ref(), "ordering should be most→least recent");
        assert!(
            entry.title.as_ref().is_some_and(|t| t.starts_with(prompt)),
            "title should start with '{}', got: {:?}",
            prompt,
            entry.title
        );
        assert!(entry.updated_at.is_some(), "should have updatedAt");
        assert_eq!(
            std::path::Path::new(&entry.cwd).canonicalize().ok(),
            cwd.canonicalize().ok(),
            "cwd should match"
        );
    }

    // Verify /chat command options returns the same sessions
    let current_session = session_ids.last().unwrap();
    let chat_opts = client
        .get_command_options(current_session.clone(), "chat")
        .await
        .expect("get_command_options failed");

    // Same number of sessions
    assert_eq!(chat_opts.options.len(), result.sessions.len());

    // Every chat option should match a list entry by session ID and title
    for opt in &chat_opts.options {
        let list_entry = result
            .sessions
            .iter()
            .find(|s| s.session_id == opt.value)
            .unwrap_or_else(|| panic!("chat option {} not found in list_sessions", opt.value));
        assert!(
            opt.label.contains(list_entry.title.as_deref().unwrap()),
            "chat option label '{}' should contain title '{:?}'",
            opt.label,
            list_entry.title
        );
    }
}

/// Verifies that MCP child processes are cleaned up when the agent receives SIGTERM.
///
/// This reproduces the orphan process bug: when the parent agent process is killed
/// with SIGTERM, MCP server child processes (spawned with process_group(0)) are not
/// cleaned up because the graceful shutdown path never runs.
#[cfg(unix)]
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn sigterm_cleans_up_mcp_child_processes() {
    // MCP stdio handshake is unreliable on CI runners under load
    if std::env::var("CI").is_ok() {
        return;
    }

    use std::path::PathBuf;

    use mock_mcp_server::prebuild_bin;
    use nix::sys::signal::{
        Signal,
        kill,
    };
    use nix::unistd::Pid;
    use sacp::schema::McpServerStdio;

    let binary_path = prebuild_bin().expect("failed to build mock-mcp-server");

    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/stdio_server.jsonl");

    let (mut harness, client) = AcpTestHarnessBuilder::new("sigterm_mcp_cleanup")
        .with_trust_all(true)
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();
    let agent_pid = harness.child.id().expect("agent process should have a PID");

    let mcp_server = sacp::schema::McpServer::Stdio(McpServerStdio::new("test-sigterm-mcp", &binary_path).args(vec![
        "--config".to_string(),
        config_path.to_str().unwrap().to_string(),
        "--linger".to_string(),
    ]));

    let _resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");

    // Wait for the MCP server child process to appear (poll pgrep).
    // We don't need the full MCP handshake — just need the process running.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut child_pids: Vec<i32>;
    loop {
        let pgrep_output = std::process::Command::new("pgrep")
            .args(["-P", &agent_pid.to_string()])
            .output()
            .expect("failed to run pgrep");

        child_pids = String::from_utf8_lossy(&pgrep_output.stdout)
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();

        if !child_pids.is_empty() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("Agent did not spawn MCP server child process within 30s");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Send SIGTERM to the agent process
    kill(Pid::from_raw(agent_pid as i32), Signal::SIGTERM).expect("failed to send SIGTERM");

    // Wait for the agent process to exit (8s shutdown timeout + buffer)
    let _exit_status = tokio::time::timeout(Duration::from_secs(15), harness.child.wait())
        .await
        .expect("agent process did not exit within timeout")
        .expect("failed to wait for agent process");

    // Give a moment for child process cleanup to propagate
    sleep(Duration::from_millis(500)).await;

    // Verify MCP server processes are no longer running
    for pid in &child_pids {
        let is_alive = kill(Pid::from_raw(*pid), None).is_ok();
        if is_alive {
            // Clean up the orphan so the test doesn't leave lingering processes
            let _ = kill(Pid::from_raw(*pid), Signal::SIGKILL);
        }
        assert!(
            !is_alive,
            "MCP server process (PID {}) should have been cleaned up after SIGTERM, but is still running",
            pid
        );
    }
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn chat_new_creates_fresh_session() {
    let (mut harness, client, first_session_id, cwd) = AcpTestHarnessBuilder::new("chat_new_creates_fresh_session")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&first_session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(first_session_id.clone(), "hello from first session")
        .await
        .expect("prompt failed");

    let first_log = harness.paths.sessions_dir.join(format!("{}.jsonl", first_session_id));
    let first_log_size = std::fs::metadata(&first_log).expect("first log should exist").len();
    assert!(first_log_size > 0, "First session log should have content after prompt");

    let second_resp = client
        .new_session(cwd.clone())
        .await
        .expect("second new_session failed");
    let second_session_id = second_resp.session_id;

    assert_ne!(
        first_session_id, second_session_id,
        "New session should have a different ID"
    );

    // Old session's log must not be truncated or modified by creating a new session
    let first_metadata = harness.paths.sessions_dir.join(format!("{}.json", first_session_id));
    assert!(first_metadata.exists(), "First session metadata should still exist");
    assert!(first_log.exists(), "First session log should still exist");
    let post_new_size = std::fs::metadata(&first_log)
        .expect("first log should still exist")
        .len();
    assert_eq!(
        first_log_size, post_new_size,
        "First session log should not be modified"
    );

    let list_resp = client.list_sessions(cwd).await.expect("list_sessions failed");
    let old_session_listed = list_resp
        .sessions
        .iter()
        .any(|s| s.session_id == first_session_id.0.to_string());
    assert!(old_session_listed, "Old session should appear in list_sessions");

    let second_metadata = harness.paths.sessions_dir.join(format!("{}.json", second_session_id));
    let second_log = harness.paths.sessions_dir.join(format!("{}.jsonl", second_session_id));
    assert!(second_metadata.exists(), "Second session metadata should exist");
    assert!(second_log.exists(), "Second session log should exist");
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn exits_when_stdin_closes() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("stdin_close_exit")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Do a real message exchange so the connection is fully warmed up.
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    // Drop the client, which owns stdin/stdout. Closing stdin simulates
    // the parent process (TUI) dying or doing a half-close.
    drop(client);

    let status = tokio::time::timeout(Duration::from_secs(10), harness.child.wait())
        .await
        .expect("agent did not exit after stdin closed")
        .expect("failed to wait for agent");

    assert!(status.success(), "agent should exit cleanly, got {:?}", status);
}

/// Test that session-injected MCP servers survive a mode swap (setSessionMode).
///
/// Regression test: `handle_set_mode` used to look up the base agent config from
/// `self.agent_configs` (which doesn't include session-injected servers) and pass it
/// to `swap_agent`, causing injected MCP servers to be dropped.
#[tokio::test]
#[timeout(240000)]
#[serial]
async fn set_mode_preserves_session_injected_mcp_servers() {
    // MCP stdio handshake is unreliable on CI runners under load
    if std::env::var("CI").is_ok() {
        return;
    }

    use std::path::PathBuf;

    use agent::agent_config::definitions::AgentConfigV2025_08_22;
    use mock_mcp_server::prebuild_bin;
    use sacp::schema::McpServerStdio;

    let binary_path = prebuild_bin().expect("failed to build mock-mcp-server");

    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/stdio_server.jsonl");

    // Create a second agent to swap to
    let alt_config = AgentConfigV2025_08_22 {
        name: "alt_agent".to_string(),
        global_prompt: Some("You are the alt agent".to_string()),
        ..Default::default()
    };

    let (harness, client) = AcpTestHarnessBuilder::new("set_mode_preserves_mcp")
        .with_agent_config("alt_agent", &alt_config)
        .with_trust_all(true)
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();

    // Create session with an injected MCP server
    let mcp_server = sacp::schema::McpServer::Stdio(
        McpServerStdio::new("injected-mcp", &binary_path)
            .args(vec!["--config".to_string(), config_path.to_str().unwrap().to_string()]),
    );

    let resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");
    let session_id = resp.session_id;

    // Wait for the injected MCP server to initialize
    let mcp_initialized_method = methods::MCP_SERVER_INITIALIZED
        .strip_prefix("_")
        .expect("method should have underscore prefix");

    let initialized = client
        .wait_for_timeout(
            |captured| {
                captured.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == mcp_initialized_method && {
                        let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                        params.get("serverName").and_then(|v| v.as_str()) == Some("injected-mcp")
                    }
                })
            },
            std::time::Duration::from_secs(90),
        )
        .await;
    assert!(initialized, "injected MCP server did not initialize within 90s");

    // Clear notifications so we can detect re-initialization after swap
    client.clear_captured().await;

    // Swap to the alt agent
    client
        .set_session_mode(session_id.clone(), "alt_agent".to_string())
        .await
        .expect("set_session_mode failed");

    // The injected MCP server should re-initialize after the swap
    let reinitialized = client
        .wait_for_timeout(
            |captured| {
                captured.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == mcp_initialized_method && {
                        let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                        params.get("serverName").and_then(|v| v.as_str()) == Some("injected-mcp")
                    }
                })
            },
            std::time::Duration::from_secs(90),
        )
        .await;
    assert!(
        reinitialized,
        "injected MCP server did not re-initialize after mode swap within 90s"
    );
}

/// Verifies that switch_to_execution auto-swaps back to the default agent
/// and injects the plan as a new prompt, keeping the TUI prompt response
/// alive through the swap so the plan execution streams back to the client.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn switch_to_execution_swaps_and_executes_plan() {
    use agent::agent_config::definitions::AgentConfigV2025_08_22;
    use agent::tools::BuiltInToolName;

    // Create a planner agent config with switch_to_execution tool
    let planner_config = AgentConfigV2025_08_22 {
        name: "kiro_planner".to_string(),
        global_prompt: Some("You are a planning agent".to_string()),
        tools: vec![
            BuiltInToolName::FsRead.to_string(),
            BuiltInToolName::SwitchToExecution.to_string(),
        ],
        ..Default::default()
    };

    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("switch_to_execution_swaps_and_executes_plan")
            .with_agent_config("kiro_planner", &planner_config)
            .with_trust_all(true)
            .build_with_session()
            .await;

    // Push mock response for the planner (calls switch_to_execution)
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/switch_to_execution.jsonl")
        .await;

    // Push mock response for the default agent (receives the plan and responds)
    harness
        .push_mock_responses_from_file(
            &session_id.0,
            "tests/mock_responses/switch_to_execution_plan_response.jsonl",
        )
        .await;

    // Switch to planner agent first
    client
        .set_session_mode(session_id.clone(), "kiro_planner".to_string())
        .await
        .expect("set_session_mode to planner failed");

    // Clear captured notifications from the mode switch
    client.clear_captured().await;

    // Send prompt to the planner — this should:
    // 1. Planner calls switch_to_execution
    // 2. ACP intercepts, swaps to default agent
    // 3. Injects plan as new prompt to default agent
    // 4. Default agent responds
    // 5. EndTurn fires, prompt response sent to client
    let result = client
        .prompt_text(session_id.clone(), "build me a todo app")
        .await
        .expect("prompt failed");

    // The prompt should complete successfully (not hang)
    assert_eq!(
        result.stop_reason,
        agent_client_protocol::StopReason::EndTurn,
        "prompt should complete with EndTurn"
    );

    // Verify agent switched notification was sent
    let captured = client.captured().await;
    let agent_switched = captured
        .ext_notifications
        .iter()
        .find(|n| n.method.as_ref().contains("agent/switched"));
    assert!(
        agent_switched.is_some(),
        "should have received AgentSwitched notification"
    );

    // Verify the switch was back to the default agent
    let params: serde_json::Value = serde_json::from_str(agent_switched.unwrap().params.get()).unwrap();
    assert_eq!(
        params.get("agentName").and_then(|v| v.as_str()),
        Some("kiro_default"),
        "should have switched to kiro_default"
    );

    // Verify the plan execution happened — the second mock response should have been consumed
    let session_updates = &captured.session_updates;
    let has_plan_response = session_updates.iter().any(|u| {
        if let SessionUpdate::AgentMessageChunk(chunk) = u
            && let agent_client_protocol::ContentBlock::Text(text) = &chunk.content
        {
            return text.text.contains("implement the plan");
        }
        false
    });
    assert!(
        has_plan_response,
        "should have received content from plan execution, got: {:?}",
        session_updates
    );
}

/// Verifies that skills discovered from `.kiro/skills/<name>/SKILL.md` are
/// advertised to the client and invocable as `/skill-name` slash commands.
///
/// Exercises the new GetSkills/ResolveSkill agent protocol added in the skill
/// slash commands feature — plumbing that isn't covered by the unit tests in
/// `crates/agent/src/agent/prompts/skills.rs` or
/// `crates/chat-cli-v2/src/agent/acp/acp_provider.rs`.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn workspace_skill_is_advertised_and_invocable() {
    let (harness, client) = AcpTestHarnessBuilder::new("workspace_skill_is_advertised_and_invocable")
        .build()
        .await;

    // Write a skill in the session's workspace cwd BEFORE creating the session.
    // The default agent's resources include `skill://.kiro/skills/*/SKILL.md`,
    // so this path is what GetSkills globs against via AcpProvider's cwd. If
    // we created the session first, the initial advertise would see no skills
    // and we'd have no trigger to re-advertise.
    let skill_dir = harness.paths.cwd.join(".kiro").join("skills").join("greet");
    std::fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    let skill_body = "# Greet\n\nSay hello to $ARGUMENTS.";
    let skill_content = format!(
        "---\nname: greet\ndescription: Say hello to someone\n---\n{}",
        skill_body
    );
    std::fs::write(skill_dir.join("SKILL.md"), &skill_content).expect("failed to write skill");

    // Now create the session — this triggers advertise_commands which calls
    // get_skills() and should discover the skill we just wrote.
    let cwd = harness.paths.cwd.clone();
    let resp = client.new_session(cwd).await.expect("new_session failed");
    let session_id = resp.session_id;
    let mut harness = harness;

    // Advertisement is fire-and-forget and reaches the TUI via a
    // `_kiro.dev/commands/available` ExtNotification. Poll for it to avoid a
    // race with the async send.
    let available_method = "kiro.dev/commands/available";
    client
        .wait_for(|captured| find_skill_prompt(captured, available_method, "greet").is_some())
        .await;

    let captured = client.captured().await;
    let skill_prompt =
        find_skill_prompt(&captured, available_method, "greet").expect("greet skill should be advertised");
    assert_eq!(
        skill_prompt.get("description").and_then(|v| v.as_str()),
        Some("Say hello to someone")
    );
    assert_eq!(
        skill_prompt.get("serverName").and_then(|v| v.as_str()),
        Some("skill:config"),
        "skill prompts must use the skill:config source label so the TUI can distinguish them"
    );

    // Invoke `/greet World` — ACP should resolve the skill via resolve_skill(),
    // strip the frontmatter, expand $ARGUMENTS, and forward the expanded body
    // as a user prompt to the agent loop.
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "/greet World")
        .await
        .expect("slash prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(
        requests.len(),
        1,
        "skill invocation should produce exactly one LLM request"
    );
    let sent = &requests[0].user_input_message.content;
    assert!(
        sent.contains("Say hello to World"),
        "expanded skill body should be sent as the user prompt, got: {:?}",
        sent
    );
    assert!(
        !sent.contains("name: greet") && !sent.contains("description: Say hello to someone"),
        "YAML frontmatter should be stripped before the skill is sent to the model, got: {:?}",
        sent
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn skill_without_placeholders_appends_trailing_text() {
    let (harness, client) = AcpTestHarnessBuilder::new("skill_without_placeholders_appends_trailing_text")
        .build()
        .await;

    // Skill body has no $ARGUMENTS or ${N} placeholders.
    let skill_dir = harness.paths.cwd.join(".kiro").join("skills").join("plain");
    std::fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    let skill_body = "# Plain\n\nRun the plain skill.";
    let skill_content = format!(
        "---\nname: plain\ndescription: A skill with no placeholders\n---\n{}",
        skill_body
    );
    std::fs::write(skill_dir.join("SKILL.md"), &skill_content).expect("failed to write skill");

    let cwd = harness.paths.cwd.clone();
    let resp = client.new_session(cwd).await.expect("new_session failed");
    let session_id = resp.session_id;
    let mut harness = harness;

    let available_method = "kiro.dev/commands/available";
    client
        .wait_for(|captured| find_skill_prompt(captured, available_method, "plain").is_some())
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    // Invoke `/plain please check tests` — trailing text should be appended to
    // the skill body since the skill has no placeholder for it.
    client
        .prompt_text(session_id.clone(), "/plain please check tests")
        .await
        .expect("slash prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(
        requests.len(),
        1,
        "skill invocation should produce exactly one LLM request"
    );
    let sent = &requests[0].user_input_message.content;
    assert!(
        sent.contains("Run the plain skill."),
        "skill body should be sent as the user prompt, got: {:?}",
        sent
    );
    assert!(
        sent.contains("please check tests"),
        "trailing text should be appended to the skill body, got: {:?}",
        sent
    );
    assert!(
        !sent.contains("name: plain") && !sent.contains("description: A skill with no placeholders"),
        "YAML frontmatter should be stripped before the skill is sent to the model, got: {:?}",
        sent
    );
}

/// Find a prompt advertisement matching `name` in the latest
/// `commands/available` ExtNotification.
fn find_skill_prompt(captured: &common::CapturedNotifications, method: &str, name: &str) -> Option<serde_json::Value> {
    captured
        .ext_notifications
        .iter()
        .rev()
        .find(|n| n.method.as_ref() == method)
        .and_then(|n| serde_json::from_str::<serde_json::Value>(n.params.get()).ok())
        .and_then(|params| {
            params
                .get("prompts")?
                .as_array()?
                .iter()
                .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(name))
                .cloned()
        })
}

/// Regression test for V2184286366: code tool's pattern_search on a file outside CWD
/// must trigger a permission request instead of being auto-approved.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn code_pattern_search_outside_cwd_triggers_permission_request() {
    // trust_all: false — we want to verify the permission request is generated
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("code_pattern_search_outside_cwd")
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(
            &session_id.0,
            "tests/mock_responses/code_pattern_search_outside_cwd.jsonl",
        )
        .await;

    // Queue a permission response so the test doesn't hang waiting for approval
    client
        .queue_permission_response(PermissionResponse::Select("allow_once".to_string()))
        .await;

    client
        .prompt_text(
            session_id.clone(),
            "use code tool pattern_search on /etc/sensitive/cookie-jar.json",
        )
        .await
        .expect("prompt failed");

    let captured = client.captured().await;

    // The code tool targeting a file outside CWD should have triggered a permission request
    assert!(
        !captured.permission_requests.is_empty(),
        "code pattern_search outside CWD should trigger a permission request, but none were captured"
    );

    // Verify the permission request is for the code tool
    let code_perm = captured.permission_requests.iter().find(|r| {
        r.tool_call.fields.title.as_deref().is_some_and(|t| {
            let t_lower = t.to_lowercase();
            t_lower.contains("code") || t_lower.contains("pattern")
        })
    });
    assert!(
        code_perm.is_some(),
        "expected a permission request for the code tool, got: {:?}",
        captured
            .permission_requests
            .iter()
            .map(|r| &r.tool_call.fields.title)
            .collect::<Vec<_>>()
    );
}

/// Verifies that thinking/reasoning content from the model is preserved in conversation
/// history and sent back in the next request's AssistantResponseMessage.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn thinking_block_preserved_in_next_request() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("thinking_block_preserved_in_next_request")
        .build_with_session()
        .await;

    // Turn 1: model responds with thinking + text
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/thinking_then_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "think about this")
        .await
        .expect("first prompt failed");

    // Turn 2: model responds with simple text
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "follow up question")
        .await
        .expect("second prompt failed");

    // Verify the second request's history contains reasoning_content from turn 1
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 2, "should have captured two requests");

    let second_request = &captured[1];
    let history = second_request
        .history
        .as_ref()
        .expect("second request should have history");

    // Find the assistant message from turn 1 in the history (skip canned context messages)
    let assistant_msg = history
        .iter()
        .filter_map(|msg| {
            if let chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(m) = msg {
                Some(m)
            } else {
                None
            }
        })
        .find(|m| m.content.contains("Here is my answer"))
        .expect("history should contain the model's assistant message");

    let reasoning = assistant_msg
        .reasoning_content
        .as_ref()
        .expect("assistant message should have reasoning_content");

    assert_eq!(reasoning.text, "Let me think about this carefully.");
    assert_eq!(reasoning.signature.as_deref(), Some("sig-abc-123"));
    assert!(reasoning.redacted_content.is_empty());
}

#[tokio::test]
async fn signature_only_thinking_preserved_in_next_request() {
    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("signature_only_thinking_preserved_in_next_request")
            .build_with_session()
            .await;

    // Turn 1: model responds with signature-only reasoning (no visible text) + text
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/signature_only_thinking.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "do something")
        .await
        .expect("first prompt failed");

    // Turn 2: simple text
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "follow up")
        .await
        .expect("second prompt failed");

    // Verify the second request preserves the signature from turn 1
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 2);

    let second_request = &captured[1];
    let history = second_request.history.as_ref().expect("should have history");

    let assistant_msg = history
        .iter()
        .filter_map(|msg| {
            if let chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(m) = msg {
                Some(m)
            } else {
                None
            }
        })
        .find(|m| m.content.contains("Response after encrypted thinking"))
        .expect("history should contain the assistant message");

    let reasoning = assistant_msg
        .reasoning_content
        .as_ref()
        .expect("assistant message should have reasoning_content for signature-only thinking");

    assert_eq!(reasoning.text, "");
    assert_eq!(reasoning.signature.as_deref(), Some("sig-encrypted-456"));
}

/// Regression test: when a user cancels mid-stream (ctrl-C), the user's original prompt
/// should be preserved in conversation history so the next turn has context.
/// Today the prompt is silently dropped, causing the model to lose context.
#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn cancelled_prompt_preserved_in_next_turn_history() {
    use chat_cli_v2::api_client::send_message_output::MockStreamItem;

    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("cancelled_prompt_preserved_in_next_turn_history")
            .build_with_session()
            .await;

    // Push partial streaming events but NOT the end-of-stream terminator yet.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                amzn_codewhisperer_streaming_client::types::ChatResponseStream::AssistantResponseEvent(
                    AssistantResponseEventBuilder::default()
                        .content("In silicon valleys, deep and wide,\nWhere streams of data flow like tide,\n")
                        .build()
                        .unwrap(),
                )
                .into(),
            )]),
        )
        .await;

    // Send prompt async (stream stays open since no None terminator)
    let prompt_recv = client
        .prompt_text_async(session_id.clone(), "Can you write a long poem about coding agent war")
        .await;

    // Wait for the agent to be mid-stream. We use sleep here because wait_for(AgentMessageChunk)
    // has its own race with notification delivery.
    sleep(Duration::from_millis(200)).await;

    // Fire cancel (non-blocking), then push end-of-stream to unblock the drain
    client.cancel_async(session_id.clone()).await;
    sleep(Duration::from_millis(50)).await;
    harness.push_mock_response(&session_id.0, None).await;

    // Await the prompt result
    let prompt_res = tokio::time::timeout(Duration::from_secs(5), prompt_recv)
        .await
        .expect("prompt_recv timed out after cancel")
        .expect("channel closed")
        .expect("prompt error");
    assert_eq!(
        prompt_res.stop_reason,
        agent_client_protocol::StopReason::Cancelled,
        "first prompt should be cancelled"
    );

    // Now send a follow-up prompt ("keep going")
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "keep going")
        .await
        .expect("second prompt failed");

    // Capture the second request and verify it contains the first user prompt in history
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(captured.len(), 2, "should have captured two requests");

    let second_request = &captured[1];
    let history = second_request
        .history
        .as_ref()
        .expect("second request should have history");

    // The history should contain the original user prompt from the cancelled turn
    let has_original_prompt = history.iter().any(|msg| {
        if let chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(m) = msg {
            m.content.contains("poem about coding agent war")
        } else {
            false
        }
    });

    assert!(
        has_original_prompt,
        "history should contain the original cancelled prompt so the model has context.\n\
         History messages: {:?}",
        history
            .iter()
            .map(|m| match m {
                chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(m) =>
                    format!("User: {}", &m.content[..m.content.len().min(80)]),
                chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(m) =>
                    format!("Assistant: {}", &m.content[..m.content.len().min(80)]),
            })
            .collect::<Vec<_>>()
    );
}

/// When a tool executes successfully but the LLM's follow-up response is cancelled mid-stream,
/// the tool results should be preserved in conversation history (not replaced with "cancelled").
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn cancelled_tool_response_preserves_tool_results() {
    use chat_cli_v2::api_client::send_message_output::MockStreamItem;

    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("cancelled_tool_response_preserves_tool_results")
            .with_trust_all(true)
            .build_with_session()
            .await;

    // Create a file in the test cwd for the read tool to find
    let cwd = &harness.paths.cwd;
    std::fs::write(cwd.join("test_file.txt"), "hello from test file\n").unwrap();

    // First response: LLM asks to read the file (tool executes automatically with trust_all)
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/tool_read_then_cancel.jsonl")
        .await;

    // Second response: LLM starts streaming its reply to the tool results, but we won't terminate it
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                amzn_codewhisperer_streaming_client::types::ChatResponseStream::AssistantResponseEvent(
                    AssistantResponseEventBuilder::default()
                        .content("The file contains ")
                        .build()
                        .unwrap(),
                )
                .into(),
            )]),
        )
        .await;

    // Send prompt async (will complete tool use, then hang on partial second response)
    let prompt_recv = client.prompt_text_async(session_id.clone(), "read test_file.txt").await;

    // Wait for the tool to execute and the second stream to start
    sleep(Duration::from_millis(500)).await;

    // Cancel mid-stream, then push end-of-stream to unblock drain
    client.cancel_async(session_id.clone()).await;
    sleep(Duration::from_millis(50)).await;
    harness.push_mock_response(&session_id.0, None).await;

    // Await the prompt result
    let prompt_res = tokio::time::timeout(Duration::from_secs(5), prompt_recv)
        .await
        .expect("prompt_recv timed out")
        .expect("channel closed")
        .expect("prompt error");
    assert_eq!(prompt_res.stop_reason, agent_client_protocol::StopReason::Cancelled,);

    // Send a follow-up prompt
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "what was in the file?")
        .await
        .expect("second prompt failed");

    // Verify the captured history contains the tool results (not "cancelled")
    let captured = harness.get_captured_requests(&session_id.0).await;
    assert_eq!(
        captured.len(),
        3,
        "should have 3 requests: initial, tool results, follow-up"
    );

    let third_request = &captured[2];
    let history = third_request
        .history
        .as_ref()
        .expect("third request should have history");

    // The history should contain a user message with tool_results that has the actual file content
    let has_tool_results_with_content = history.iter().any(|msg| {
        if let chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(m) = msg {
            m.user_input_message_context
                .as_ref()
                .and_then(|ctx| ctx.tool_results.as_ref())
                .is_some_and(|results| {
                    results.iter().any(|r| {
                        r.content.iter().any(|block| match block {
                            chat_cli_v2::api_client::model::ToolResultContentBlock::Text(text) => {
                                text.contains("hello from test file")
                            },
                            _ => false,
                        })
                    })
                })
        } else {
            false
        }
    });

    // The history should NOT contain "Tool use was cancelled by the user" for this tool
    let has_cancelled_message = history.iter().any(|msg| {
        if let chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(m) = msg {
            m.user_input_message_context
                .as_ref()
                .and_then(|ctx| ctx.tool_results.as_ref())
                .is_some_and(|results| {
                    results.iter().any(|r| {
                        r.content.iter().any(|block| match block {
                            chat_cli_v2::api_client::model::ToolResultContentBlock::Text(text) => {
                                text.contains("Tool use was cancelled")
                            },
                            _ => false,
                        })
                    })
                })
        } else {
            false
        }
    });

    assert!(
        has_tool_results_with_content,
        "history should contain tool results with actual file content.\nHistory: {:?}",
        history
            .iter()
            .map(|m| match m {
                chat_cli_v2::api_client::model::ChatMessage::UserInputMessage(m) =>
                    format!("User: {}", &m.content[..m.content.len().min(100)]),
                chat_cli_v2::api_client::model::ChatMessage::AssistantResponseMessage(m) =>
                    format!("Assistant: {}", &m.content[..m.content.len().min(100)]),
            })
            .collect::<Vec<_>>()
    );

    assert!(
        !has_cancelled_message,
        "history should NOT contain 'Tool use was cancelled' since the tool completed successfully"
    );
}

/// A zero-arg MCP tool invoked with empty-string input should dispatch
/// normally (input coerced to `{}`), not trigger the "tool was too large"
/// retry.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn empty_mcp_tool_content_invokes_zero_arg_tool_without_retry() {
    // MCP stdio handshake is unreliable on CI runners under load, same as
    // mcp_stdio_server_tool_call.
    if std::env::var("CI").is_ok() {
        return;
    }

    use std::path::PathBuf;

    use mock_mcp_server::prebuild_bin;
    use sacp::schema::McpServerStdio;

    // Ensure mock-mcp-server binary is built
    let binary_path = prebuild_bin().expect("failed to build mock-mcp-server");

    // Zero-arg MCP tool config — `properties: {}` is the schema shape that triggers
    // the LLM-side quirk the test case documents.
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/mcp_configs/stdio_server_zero_arg.jsonl");

    let (harness, client) = AcpTestHarnessBuilder::new("empty_mcp_tool_content_invokes_zero_arg_tool_without_retry")
        .with_trust_all(true)
        .build()
        .await;

    let cwd = harness.paths.cwd.clone();

    let mcp_server = sacp::schema::McpServer::Stdio(
        McpServerStdio::new("test-stdio-mcp", binary_path)
            .args(vec!["--config".to_string(), config_path.to_str().unwrap().to_string()]),
    );

    let resp = client
        .new_session_with_mcp(cwd, vec![mcp_server])
        .await
        .expect("new_session failed");
    let session_id = resp.session_id;

    // Wait for MCP server to initialize before prompting.
    let mcp_initialized_method = methods::MCP_SERVER_INITIALIZED
        .strip_prefix("_")
        .expect("method should have underscore prefix");

    let initialized = client
        .wait_for_timeout(
            |captured| {
                captured.ext_notifications.iter().any(|n| {
                    n.method.as_ref() == mcp_initialized_method && {
                        let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                        params.get("serverName").and_then(|v| v.as_str()) == Some("test-stdio-mcp")
                    }
                })
            },
            std::time::Duration::from_secs(90),
        )
        .await;
    assert!(initialized, "MCP server 'test-stdio-mcp' did not initialize within 90s");

    let mut harness = harness;
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/mcp_stdio_empty_tool_content.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "list my cron jobs")
        .await
        .expect("prompt failed");

    // Post-fix expectation: empty-string tool content is coerced to {}, so the
    // agent dispatches the zero-arg tool normally without entering the
    // InvalidJson retry path.

    let captured = client.captured().await;
    let has_tool_call = captured
        .session_updates
        .iter()
        .any(|u| matches!(u, SessionUpdate::ToolCall(tc) if tc.title.contains("list_crons")));
    assert!(
        has_tool_call,
        "expected a ToolCall for 'list_crons'; updates: {:?}",
        captured.session_updates
    );

    // No SessionUpdate should mention "tool was too large" — that phrase is
    // only emitted on the retry path we're fixing.
    let too_large_mention = captured
        .session_updates
        .iter()
        .find(|u| format!("{u:?}").contains("tool was too large"));
    assert!(
        too_large_mention.is_none(),
        "unexpected retry path: {:?}",
        too_large_mention
    );
}

/// E2E: /effort command flow — get options, select one via command execute,
/// verify override flows through to additional_model_request_fields on next prompt.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn effort_command_e2e() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("effort_command_e2e")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // 1. Get effort options (simulates user running /effort)
    let options = client
        .get_command_options(session_id.clone(), "effort")
        .await
        .expect("get_command_options for effort failed");

    assert!(!options.options.is_empty(), "effort should have options");
    let values: Vec<&str> = options.options.iter().map(|o| o.value.as_str()).collect();
    assert!(values.contains(&"low"), "should contain 'low': {:?}", values);
    assert!(values.contains(&"high"), "should contain 'high': {:?}", values);

    // 2. User selects "low" via /effort command execute
    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "effort", "args": { "value": "low" } }),
        )
        .await
        .expect("execute_command for effort failed");
    assert!(result.success, "effort execute should succeed: {}", result.message);

    // 3. Verify the override is applied on the next prompt
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    assert!(!requests.is_empty(), "should have captured at least one request");

    let last_request = requests.last().unwrap();
    let additional_fields = last_request
        .additional_model_request_fields
        .as_ref()
        .expect("additional_model_request_fields should be set");

    assert_eq!(
        additional_fields["output_config"]["effort"], "low",
        "effort should be 'low' in the request"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn command_execute_emits_chat_slash_command_telemetry() {
    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("command_execute_emits_chat_slash_command_telemetry")
            .with_trust_all(true)
            .build_with_session()
            .await;

    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "effort", "args": { "value": "low" } }),
        )
        .await
        .expect("execute_command for effort failed");
    assert!(result.success, "effort execute should succeed: {}", result.message);

    let events = harness
        .wait_for_telemetry_events(Duration::from_secs(5), |events| {
            events.iter().any(|event| {
                matches!(
                    &event.ty,
                    chat_cli_v2::telemetry::core::EventType::ChatSlashCommandExecuted {
                        command,
                        subcommand,
                        result,
                        ..
                    } if command == "/effort"
                        && subcommand.is_none()
                        && *result == chat_cli_v2::telemetry::TelemetryResult::Succeeded
                )
            })
        })
        .await;

    let event = events
        .iter()
        .find(|event| {
            matches!(
                &event.ty,
                chat_cli_v2::telemetry::core::EventType::ChatSlashCommandExecuted { command, .. }
                    if command == "/effort"
            )
        })
        .expect("expected /effort telemetry event");

    let record = event_to_otel_metric_record(&event).expect("slash command metric");
    expect_metric(std::slice::from_ref(&record), metric::slash_command_invoked("/effort"));
    expect_metric_attrs(&record, &[("command", "/effort")]);
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn prompt_emits_chat_session_started_telemetry_once() {
    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("prompt_emits_chat_session_started_telemetry_once")
            .with_trust_all(true)
            .build_with_session()
            .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/two_simple_responses.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("first prompt failed");
    client
        .prompt_text(session_id.clone(), "again")
        .await
        .expect("second prompt failed");

    let events = harness
        .wait_for_telemetry_events(Duration::from_secs(5), |events| {
            let start_count = events
                .iter()
                .filter(|event| {
                    matches!(
                        &event.ty,
                        chat_cli_v2::telemetry::core::EventType::ChatSessionStarted { .. }
                    )
                })
                .count();
            let turn_count = events
                .iter()
                .filter(|event| {
                    matches!(
                        &event.ty,
                        chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }
                    )
                })
                .count();
            start_count >= 1 && turn_count >= 2
        })
        .await;

    let start_events = events
        .iter()
        .filter(|event| {
            matches!(
                &event.ty,
                chat_cli_v2::telemetry::core::EventType::ChatSessionStarted { .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(start_events.len(), 1);

    let record = event_to_otel_metric_record(start_events[0]).expect("chat session started metric");
    expect_metric(
        std::slice::from_ref(&record),
        metric::chat_session_started(metric::Mode::AcpExternal, metric::ClientApplication::AcpExternal),
    );
    expect_metric_attrs(&record, &[
        ("mode", "acp_external"),
        ("client_application", "acp_external"),
    ]);
}

/// E2E: per-model additional field defaults from settings file.
/// When cli.json contains `"claude-opus-4.7": {"output_config.effort": "low"}`,
/// a new session should use effort=low instead of the hardcoded default (xhigh).
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn model_defaults_from_settings_override_hardcoded() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("model_defaults_from_settings")
        .with_setting(
            "chat.modelDefaults",
            serde_json::json!({"claude-opus-4.7": {"output_config": {"effort": "low"}}}),
        )
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Send a prompt so we can inspect the captured request
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    assert!(!requests.is_empty(), "should have captured at least one request");

    let last_request = requests.last().unwrap();
    let additional_fields = last_request
        .additional_model_request_fields
        .as_ref()
        .expect("additional_model_request_fields should be set");

    // Should be "low" from settings, NOT "xhigh" (the hardcoded default for opus-4.7)
    assert_eq!(
        additional_fields["output_config"]["effort"], "low",
        "effort should be 'low' from settings, not the hardcoded 'xhigh' default"
    );
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn model_switch_applies_settings_defaults() {
    // Start on sonnet-4.6, configure opus-4.7 defaults, then switch and verify
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("model_switch_defaults")
        .with_setting("chat.defaultModel", serde_json::json!("claude-sonnet-4.6"))
        .with_setting(
            "chat.modelDefaults",
            serde_json::json!({"claude-opus-4.7": {"output_config": {"effort": "low"}}}),
        )
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Switch to opus-4.7 via /model command (slash commands don't consume mock responses)
    client
        .prompt_text(session_id.clone(), "/model claude-opus-4.7")
        .await
        .expect("model switch failed");

    // Send a prompt to capture the request with the new model's settings
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    assert!(!requests.is_empty(), "should have captured at least one request");

    let last_request = requests.last().unwrap();
    let additional_fields = last_request
        .additional_model_request_fields
        .as_ref()
        .expect("additional_model_request_fields should be set");

    // Should be "low" from settings, NOT "xhigh" (the hardcoded default for opus-4.7)
    assert_eq!(
        additional_fields["output_config"]["effort"], "low",
        "effort should be 'low' from settings after /model switch, not the hardcoded 'xhigh'"
    );
}

/// E2E: /effort command on a GPT-style model whose schema declares
/// `reasoning.effort` (instead of Claude's `output_config.effort`). Verifies
/// the override flows through the schema-resolved path on the next prompt.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn effort_command_e2e_reasoning_path() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("effort_command_e2e_reasoning_path")
        .with_setting("chat.defaultModel", serde_json::json!("gpt-5.1"))
        .with_trust_all(true)
        .build_with_session()
        .await;

    // 1. Get effort options — should still come from the schema.
    let options = client
        .get_command_options(session_id.clone(), "effort")
        .await
        .expect("get_command_options for effort failed");
    assert!(
        !options.options.is_empty(),
        "gpt-5.1 should expose effort options via reasoning.effort schema"
    );

    // 2. Set effort to "low".
    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "effort", "args": { "value": "low" } }),
        )
        .await
        .expect("execute_command for effort failed");
    assert!(result.success, "effort execute should succeed: {}", result.message);

    // 3. Verify the override lands at `reasoning.effort` (not `output_config.effort`).
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    assert!(!requests.is_empty(), "should have captured at least one request");
    let last = requests.last().unwrap();
    let additional_fields = last
        .additional_model_request_fields
        .as_ref()
        .expect("additional_model_request_fields should be set");
    assert_eq!(
        additional_fields["reasoning"]["effort"], "low",
        "effort should land at reasoning.effort for gpt-5.1"
    );
    assert!(
        additional_fields.get("output_config").is_none(),
        "no output_config bucket should be present for reasoning.effort models: {:?}",
        additional_fields
    );
}

/// E2E: `chat.modelDefaults` for a GPT-style model whose schema uses
/// `reasoning.effort`. The user's nested settings should be applied.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn model_defaults_reasoning_path_applied() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("model_defaults_reasoning_path")
        .with_setting("chat.defaultModel", serde_json::json!("gpt-5.1"))
        .with_setting(
            "chat.modelDefaults",
            serde_json::json!({"gpt-5.1": {"reasoning": {"effort": "medium"}}}),
        )
        .with_trust_all(true)
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    let last = requests.last().expect("should have captured a request");
    let additional_fields = last
        .additional_model_request_fields
        .as_ref()
        .expect("additional_model_request_fields should be set");
    assert_eq!(
        additional_fields["reasoning"]["effort"], "medium",
        "settings default should land at reasoning.effort"
    );
}

/// E2E: `chat.modelDefaults` declared with the wrong (output_config) shape for
/// a model whose schema uses `reasoning.effort`. Validation logs a warning and
/// the override is dropped — no `output_config` bucket should appear in the
/// outgoing request.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn model_defaults_reasoning_path_rejects_wrong_shape() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("model_defaults_reasoning_wrong_shape")
        .with_setting("chat.defaultModel", serde_json::json!("gpt-5.1"))
        .with_setting(
            "chat.modelDefaults",
            // gpt-5.1's schema only has reasoning.effort; output_config.effort
            // is unknown and should be skipped with a warning.
            serde_json::json!({"gpt-5.1": {"output_config": {"effort": "low"}}}),
        )
        .with_trust_all(true)
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    let last = requests.last().expect("should have captured a request");
    let additional_fields = last.additional_model_request_fields.as_ref();
    // Either no additional_model_request_fields at all, or no output_config bucket.
    if let Some(fields) = additional_fields {
        assert!(
            fields.get("output_config").is_none(),
            "output_config should be rejected by validation, got: {:?}",
            fields
        );
        assert!(
            fields.get("reasoning").is_none(),
            "reasoning bucket should be empty since the user's override was for the wrong path: {:?}",
            fields
        );
    }
}

/// E2E: `--effort` CLI flag for a model whose schema uses `reasoning.effort`.
/// The flag should be applied at the schema-resolved path on session bootstrap.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn cli_effort_flag_reasoning_path() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("cli_effort_flag_reasoning_path")
        .with_setting("chat.defaultModel", serde_json::json!("gpt-5.1"))
        .with_acp_args(["--effort", "high"])
        .with_trust_all(true)
        .build_with_session()
        .await;

    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;
    client
        .prompt_text(session_id.clone(), "hello")
        .await
        .expect("prompt failed");

    let requests = harness.get_captured_requests(&session_id.0).await;
    let last = requests.last().expect("should have captured a request");
    let additional_fields = last
        .additional_model_request_fields
        .as_ref()
        .expect("additional_model_request_fields should be set");
    assert_eq!(
        additional_fields["reasoning"]["effort"], "high",
        "--effort flag should land at reasoning.effort for gpt-5.1"
    );
}

/// /model switch should persist the model as the default (no need for set-current-as-default)
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn model_switch_persists_as_default() {
    let (harness, client, session_id, _) = AcpTestHarnessBuilder::new("model_switch_persists")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Switch to a different model via /model command
    client
        .prompt_text(session_id.clone(), "/model claude-sonnet-4.6")
        .await
        .expect("model switch failed");

    // Verify the success message indicates persistence
    let captured = client.captured().await;
    let has_saved_message = captured.session_updates.iter().any(|update| {
        if let SessionUpdate::AgentMessageChunk(chunk) = update {
            let text = format!("{:?}", chunk);
            text.to_lowercase().contains("saved")
        } else {
            false
        }
    });
    assert!(
        has_saved_message,
        "Model switch message should indicate the preference was saved"
    );

    // Verify the setting was persisted to disk
    let settings_content = std::fs::read_to_string(&harness.paths.settings_path).expect("failed to read settings file");
    let settings: serde_json::Value = serde_json::from_str(&settings_content).expect("failed to parse settings");
    assert_eq!(
        settings.get("chat.defaultModel").and_then(|v| v.as_str()),
        Some("claude-sonnet-4.6"),
        "chat.defaultModel should be persisted when switching models"
    );
}

/// /effort should persist the effort level as a per-model default
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn effort_command_persists_to_model_defaults() {
    let (_harness, client, session_id, _) = AcpTestHarnessBuilder::new("effort_persists")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Set effort to "low" via execute_command
    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "effort", "args": { "value": "low" } }),
        )
        .await
        .expect("execute_command for effort failed");
    assert!(result.success, "effort execute should succeed: {}", result.message);
    assert!(
        result.message.to_lowercase().contains("saved"),
        "effort message should indicate persistence: {}",
        result.message
    );

    // Verify the setting was persisted to disk under chat.modelDefaults
    let settings_content =
        std::fs::read_to_string(&_harness.paths.settings_path).expect("failed to read settings file");
    let settings: serde_json::Value = serde_json::from_str(&settings_content).expect("failed to parse settings");
    let model_defaults = settings
        .get("chat.modelDefaults")
        .expect("chat.modelDefaults should exist");
    // The default model is claude-opus-4.7, so effort should be saved under that key
    let effort = model_defaults
        .pointer("/claude-opus-4.7/output_config/effort")
        .and_then(|v| v.as_str());
    assert_eq!(
        effort,
        Some("low"),
        "effort should be persisted under chat.modelDefaults for the current model"
    );
}

/// Regression: setting effort on two different models in one session must preserve both.
/// Previously the agent's stale local settings copy caused the second write to clobber the first.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn effort_persists_for_multiple_models_in_one_session() {
    let (harness, client, session_id, _) = AcpTestHarnessBuilder::new("effort_multi_model")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Set effort on the default model (claude-opus-4.7)
    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "effort", "args": { "value": "low" } }),
        )
        .await
        .expect("effort on opus-4.7 failed");
    assert!(result.success);

    // Switch to a different model via set_session_model (bypasses model list)
    client
        .set_session_model(session_id.clone(), "claude-opus-4.6".to_string())
        .await
        .expect("set_session_model failed");

    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "effort", "args": { "value": "high" } }),
        )
        .await
        .expect("effort on opus-4.6 failed");
    assert!(result.success, "effort should succeed: {}", result.message);

    // Both models' effort settings must be present on disk
    let settings_content = std::fs::read_to_string(&harness.paths.settings_path).expect("failed to read settings file");
    let settings: serde_json::Value = serde_json::from_str(&settings_content).expect("failed to parse settings");
    let model_defaults = settings
        .get("chat.modelDefaults")
        .expect("chat.modelDefaults should exist");

    assert_eq!(
        model_defaults
            .pointer("/claude-opus-4.7/output_config/effort")
            .and_then(|v| v.as_str()),
        Some("low"),
        "opus-4.7 effort should still be 'low' after setting effort on another model"
    );
    assert_eq!(
        model_defaults
            .pointer("/claude-opus-4.6/output_config/effort")
            .and_then(|v| v.as_str()),
        Some("high"),
        "opus-4.6 effort should be 'high'"
    );
}

/// Verifies that /goal command sets a goal and returns success.
#[tokio::test]
#[timeout(60000)]
#[serial]
async fn goal_set_and_status() {
    // /goal is always available (no rollout gate).
    let (_harness, client, session_id, _) = AcpTestHarnessBuilder::new("goal_set_and_status")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Set a goal
    let result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({
                "command": "goal",
                "args": { "value": "\"implement pagination with passing tests\" --max 3" }
            }),
        )
        .await
        .expect("execute_command for goal failed");
    assert!(result.success, "goal set should succeed: {}", result.message);
    assert!(
        result.message.contains("goal set"),
        "message should confirm goal set: {}",
        result.message
    );

    // Clear
    let clear_result = client
        .execute_command(
            session_id.clone(),
            serde_json::json!({ "command": "goal", "args": { "value": "clear" } }),
        )
        .await
        .expect("execute_command for goal clear failed");
    assert!(clear_result.success);
}

/// Regression test for the goal race condition where an error after goal(complete)
/// tool execution (e.g., content-filtered follow-up response) previously caused:
/// 1. "Kiro failed to generate a response" surfaced to user (even though goal succeeded)
/// 2. Ghost re-injection task spawning "Agent is not idle" errors
///
/// The fix ensures that when goal(complete) already fired, a subsequent error on the
/// model's follow-up response is suppressed — the goal completed successfully.
#[tokio::test]
#[timeout(60000)]
#[serial]
async fn goal_complete_with_thinking_suppresses_followup_error() {
    let (mut harness, client, session_id, _) =
        AcpTestHarnessBuilder::new("goal_complete_with_thinking_suppresses_followup_error")
            .with_trust_all(true)
            .build_with_session()
            .await;

    // Push mock responses BEFORE sending the prompt so they're queued when the agent
    // calls send_message:
    // 1. First response: thinking + text + goal(complete) tool call
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/goal_complete_with_thinking.jsonl")
        .await;
    // 2. Second response (after tool result): empty — triggers EmptyResponse error
    harness.push_mock_response(&session_id.0, Some(vec![])).await;
    harness.push_mock_response(&session_id.0, None).await;
    // 3. Third response: empty retry (agent retries empty responses once)
    harness.push_mock_response(&session_id.0, Some(vec![])).await;
    harness.push_mock_response(&session_id.0, None).await;

    // Send "/goal sup" as a prompt. The ACP parses this as a slash command, sets up
    // the goal controller, holds the prompt response, and injects the first prompt
    // to the model (which will consume mock response #1 above).
    //
    // With the fix: goal(complete) fires from mock response #1, then the empty
    // follow-up (mock #2/#3) triggers enter_error_state → Stop(Error). Since the
    // goal is already Completed, the error is suppressed and the prompt resolves
    // successfully.
    //
    // Without the fix: Stop(Error) surfaces "Kiro failed to generate a response"
    // as an error to the caller, and the subsequent EndTurn spawns a ghost
    // re-injection task.
    let result = client.prompt_text(session_id.clone(), "/goal sup").await;
    assert!(
        result.is_ok(),
        "prompt should succeed (goal completed before error), got: {:?}",
        result.err()
    );
}
