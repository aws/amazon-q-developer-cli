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
    assert_eq!(resp.protocol_version, agent_client_protocol::V1);

    // Verify auth_methods contains login guidance
    assert_eq!(resp.auth_methods.len(), 1);
    let auth_method = &resp.auth_methods[0];
    assert_eq!(auth_method.id.0.as_ref(), "kiro-login");
    assert_eq!(auth_method.name, "Kiro Login");
    assert!(
        auth_method
            .description
            .as_ref()
            .unwrap()
            .contains("https://kiro.dev/docs/cli/authentication/")
    );
}

/// Verifies that new_session waits for MCP server initialization before returning.
#[tokio::test]
#[timeout(30000)]
#[serial]
async fn new_session_waits_for_mcp_server_initialization() {
    use std::path::PathBuf;
    use std::time::Instant;

    use mock_mcp_server::prebuild_bin;
    use sacp::schema::McpServerStdio;

    prebuild_bin().expect("failed to build mock-mcp-server");

    let binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/debug/mock-mcp-server");

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
            events
                .iter()
                .any(|e| matches!(&e.ty, chat_cli_v2::telemetry::core::EventType::ChatAddedMessage { .. }))
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
    assert_eq!(
        conv.user_input_message.content, "hello",
        "current message should be the prompt"
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

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn set_mode_switches_agent() {
    use agent::agent_config::definitions::AgentConfigV2025_08_22;

    let swapped_config = AgentConfigV2025_08_22 {
        name: "swapped_agent".to_string(),
        global_prompt: Some("You are the swapped agent".to_string()),
        tools: vec!["read".to_string(), "ls".to_string()],
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
    assert!(tool_names.contains(&"ls"), "should have ls tool");
    assert!(!tool_names.contains(&"write"), "should NOT have write tool after swap");

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
#[ignore = "still running into hangs. will need to investigate"]
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

    let prompt_res_recv = client.prompt_text_async(session_id.clone(), "Hi").await;

    let cancel_res = client.cancel(session_id).await;
    assert!(cancel_res.is_ok());

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
        acp::ContentBlock::ResourceLink(acp::ResourceLink {
            uri: "file:///test/project/auth.rs".to_string(),
            name: "auth.rs".to_string(),
            mime_type: Some("text/x-rust".to_string()),
            title: None,
            description: None,
            size: None,
            annotations: None,
            meta: None,
        }),
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
        acp::ContentBlock::Image(acp::ImageContent {
            data: base64_data,
            mime_type: "image/png".to_string(),
            uri: None,
            annotations: None,
            meta: None,
        }),
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
#[timeout(30000)]
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
#[timeout(30000)]
#[serial]
async fn mcp_stdio_server_tool_call() {
    use std::path::PathBuf;

    use mock_mcp_server::prebuild_bin;
    use sacp::schema::McpServerStdio;

    // Ensure mock-mcp-server binary is built
    prebuild_bin().expect("failed to build mock-mcp-server");

    let binary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/debug/mock-mcp-server");

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
#[timeout(30000)]
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
                        _ => false,
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
                    _ => false,
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

    // Verify telemetry events
    let events = harness.get_captured_telemetry_events().await;

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
        assert_eq!(tool_name.as_deref(), Some("ls"));
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

    // Verify telemetry events were emitted
    let events = harness.get_captured_telemetry_events().await;

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
}

#[tokio::test]
#[timeout(30000)]
#[serial]
async fn task_create_injects_context_into_next_request() {
    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("task_context_injection")
        .build_with_session()
        .await;

    // Turn 1: task_create tool call
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/tool_task_create_then_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "create a task for the auth bug")
        .await
        .expect("prompt 1 failed");

    // Turn 2: simple text response — this request should contain task context in history
    harness
        .push_mock_responses_from_file(&session_id.0, "tests/mock_responses/simple_text.jsonl")
        .await;

    client
        .prompt_text(session_id.clone(), "what tasks do we have")
        .await
        .expect("prompt 2 failed");

    // Verify context injection by reading the agent log.
    // The IPC get_captured_requests can't serialize ToolResultContentBlock::Json,
    // so we verify via the log which shows the full request messages.
    let log_content = std::fs::read_to_string(&harness.paths.log_file).expect("failed to read agent log");

    // The second request should contain the task context injection
    assert!(
        log_content.contains("Active Task List") && log_content.contains("Fix auth bug"),
        "Agent log should contain task context injection with 'Active Task List' and 'Fix auth bug'"
    );

    // Verify the context is wrapped in context entry markers
    assert!(
        log_content.contains("CONTEXT ENTRY BEGIN") && log_content.contains("CONTEXT ENTRY END"),
        "Task context should be wrapped in CONTEXT ENTRY markers"
    );

    // Verify the assistant acknowledgment message is present
    assert!(
        log_content.contains("I will fully incorporate this information"),
        "Task context injection should be part of the context message with assistant acknowledgment"
    );
}
