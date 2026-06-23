mod common;

use std::time::Duration;

use chat_cli_v2::api_client::model::ChatResponseStream;
use chat_cli_v2::api_client::send_message_output::MockStreamItem;
use common::AcpTestHarnessBuilder;
use ntest::timeout;
use serial_test::serial;
use tokio::time::sleep;

/// Integration test: steer while agent is busy → message injected at tool boundary.
#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn steer_while_agent_busy_injects_at_tool_boundary() {
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("steer_tool_boundary")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Create a test file so the read tool succeeds
    let test_file = harness.paths.cwd.join("test_file.txt");
    std::fs::write(&test_file, "hello world\n").expect("failed to create test file");

    // Push first response stream: a tool call.
    // Don't push the second response yet - we'll push it after steering.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                    content: "I'll read that file for you.".to_string(),
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_steer_001".to_string(),
                    name: "read".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_steer_001".to_string(),
                    name: "read".to_string(),
                    input: Some(r#"{"operations": [{"mode": "Line", "path": "test_file.txt"}]}"#.to_string()),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_steer_001".to_string(),
                    name: "read".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    // End the first stream so the tool use is processed
    harness.push_mock_response(&session_id.0, None).await;

    // Start a prompt that triggers the tool call (async - doesn't block)
    let prompt_rx = client.prompt_text_async(session_id.clone(), "read test_file.txt").await;

    // Wait until the tool call notification is received (agent is processing the tool)
    client
        .wait_for(|n| {
            n.session_updates
                .iter()
                .any(|u| matches!(u, agent_client_protocol::SessionUpdate::ToolCall(_)))
        })
        .await;

    // Send a steer request while the agent is busy.
    // The tool executes fast, but the agent is waiting for the second mock response
    // after sending tool results. The steer message will be queued.
    let steer_result = client
        .steer(session_id.clone(), "Actually just count the lines")
        .await
        .expect("steer request should succeed");

    // Verify the steer response indicates success
    assert_eq!(
        steer_result.get("queued").and_then(|v| v.as_bool()),
        Some(true),
        "steer response should have queued: true"
    );

    // Verify SteeringQueued notification was emitted
    let has_queued = client
        .wait_for_timeout(
            |n| {
                n.ext_notifications.iter().any(|ext| {
                    let params_str = ext.params.get();
                    params_str.contains("AgentExecutionUserMessageQueued")
                        && params_str.contains("Actually just count the lines")
                })
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(has_queued, "SteeringQueued notification should have been emitted");

    // Now push the second response stream. If the steer was queued before
    // send_tool_results(), the steering message will be injected. If it was
    // queued after (end-of-turn drain), it will start a new turn.
    // Either way, SteeringConsumed should be emitted.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "I've read the file and incorporated your steering.".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    // The end-of-turn drain may start a new turn if the steer arrived after
    // tool results were already sent. Push a third response for that case.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Counted the lines as requested.".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    // Wait for the prompt to complete
    let prompt_result = prompt_rx
        .await
        .expect("prompt channel closed")
        .expect("prompt should succeed");
    assert_eq!(prompt_result.stop_reason, agent_client_protocol::StopReason::EndTurn);

    // Give a moment for all notifications to arrive
    sleep(Duration::from_millis(200)).await;

    // Verify SteeringConsumed notification was emitted
    let captured = client.captured().await;
    let has_consumed = captured.ext_notifications.iter().any(|ext| {
        let params_str = ext.params.get();
        params_str.contains("AgentExecutionSteeringInjected") && params_str.contains("Actually just count the lines")
    });
    assert!(
        has_consumed,
        "SteeringConsumed notification should have been emitted after drain"
    );

    // Verify notifications are emitted in correct order (queued before consumed)
    let ext_params: Vec<String> = captured
        .ext_notifications
        .iter()
        .map(|ext| ext.params.get().to_string())
        .collect();
    let queued_idx = ext_params
        .iter()
        .position(|p| p.contains("AgentExecutionUserMessageQueued"));
    let consumed_idx = ext_params
        .iter()
        .position(|p| p.contains("AgentExecutionSteeringInjected"));
    assert!(
        queued_idx.is_some() && consumed_idx.is_some(),
        "both steering_queued and steering_consumed notifications should be present"
    );
    assert!(
        queued_idx.unwrap() < consumed_idx.unwrap(),
        "steering_queued should come before steering_consumed"
    );

    // Verify the steering message was injected into an LLM request
    let captured_requests = harness.get_captured_requests(&session_id.0).await;
    // There should be at least 2 requests
    assert!(
        captured_requests.len() >= 2,
        "should have at least 2 captured requests, got {}",
        captured_requests.len()
    );

    // One of the requests after the first should contain the steering message
    let has_steering_in_request = captured_requests[1..]
        .iter()
        .any(|req| req.user_input_message.content.contains("Actually just count the lines"));
    assert!(
        has_steering_in_request,
        "steering message should appear in an LLM request after the initial prompt"
    );
}

/// Integration test: multiple steers concatenated and injected together.
#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn multiple_steers_concatenated_and_injected_together() {
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("steer_multi_concat")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Create a test file so the read tool succeeds
    let test_file = harness.paths.cwd.join("test_file.txt");
    std::fs::write(&test_file, "hello world\n").expect("failed to create test file");

    // Push first response stream: a tool call.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                    content: "I'll read that file for you.".to_string(),
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_multi_001".to_string(),
                    name: "read".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_multi_001".to_string(),
                    name: "read".to_string(),
                    input: Some(r#"{"operations": [{"mode": "Line", "path": "test_file.txt"}]}"#.to_string()),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_multi_001".to_string(),
                    name: "read".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    // Start a prompt that triggers the tool call
    let prompt_rx = client.prompt_text_async(session_id.clone(), "read test_file.txt").await;

    // Wait until the tool call notification is received (agent is processing the tool)
    client
        .wait_for(|n| {
            n.session_updates
                .iter()
                .any(|u| matches!(u, agent_client_protocol::SessionUpdate::ToolCall(_)))
        })
        .await;

    // Send multiple steer requests while the agent is busy
    let steer_result_1 = client
        .steer(session_id.clone(), "First instruction")
        .await
        .expect("steer 1 should succeed");
    assert_eq!(
        steer_result_1.get("queued").and_then(|v| v.as_bool()),
        Some(true),
        "steer 1 response should have queued: true"
    );

    let steer_result_2 = client
        .steer(session_id.clone(), "Second instruction")
        .await
        .expect("steer 2 should succeed");
    assert_eq!(
        steer_result_2.get("queued").and_then(|v| v.as_bool()),
        Some(true),
        "steer 2 response should have queued: true"
    );

    let steer_result_3 = client
        .steer(session_id.clone(), "Third instruction")
        .await
        .expect("steer 3 should succeed");
    assert_eq!(
        steer_result_3.get("queued").and_then(|v| v.as_bool()),
        Some(true),
        "steer 3 response should have queued: true"
    );

    // Wait for SteeringQueued notifications - the last one should contain all messages
    let has_all_queued = client
        .wait_for_timeout(
            |n| {
                n.ext_notifications.iter().any(|ext| {
                    let params_str = ext.params.get();
                    params_str.contains("AgentExecutionUserMessageQueued") && params_str.contains("Third instruction")
                })
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        has_all_queued,
        "SteeringQueued notification should have been emitted for all messages"
    );

    // Verify that SteeringQueued notifications show growing concatenated messages.
    // Each notification should contain the full current queue value.
    let captured_before = client.captured().await;
    let queued_notifications: Vec<_> = captured_before
        .ext_notifications
        .iter()
        .filter(|ext| ext.params.get().contains("AgentExecutionUserMessageQueued"))
        .collect();

    // Should have 3 SteeringQueued notifications (one per steer)
    assert_eq!(
        queued_notifications.len(),
        3,
        "should have 3 SteeringQueued notifications, got {}",
        queued_notifications.len()
    );

    // First notification should contain only the first message
    assert!(
        queued_notifications[0].params.get().contains("First instruction"),
        "first SteeringQueued should contain 'First instruction'"
    );

    // Second notification should contain both first and second messages
    let second_params = queued_notifications[1].params.get();
    assert!(
        second_params.contains("First instruction") && second_params.contains("Second instruction"),
        "second SteeringQueued should contain both first and second messages"
    );

    // Third notification should contain all three messages
    let third_params = queued_notifications[2].params.get();
    assert!(
        third_params.contains("First instruction")
            && third_params.contains("Second instruction")
            && third_params.contains("Third instruction"),
        "third SteeringQueued should contain all three messages"
    );

    // Now push the second response stream to let the turn complete
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "I've incorporated all your steering instructions.".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    // Push a third response in case end-of-turn drain starts a new turn
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Done with all instructions.".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    // Wait for the prompt to complete
    let prompt_result = prompt_rx
        .await
        .expect("prompt channel closed")
        .expect("prompt should succeed");
    assert_eq!(prompt_result.stop_reason, agent_client_protocol::StopReason::EndTurn);

    // Give a moment for all notifications to arrive
    sleep(Duration::from_millis(200)).await;

    // Verify SteeringConsumed (AgentExecutionSteeringInjected) notifications.
    let captured = client.captured().await;
    let consumed_notifications: Vec<_> = captured
        .ext_notifications
        .iter()
        .filter(|ext| ext.params.get().contains("AgentExecutionSteeringInjected"))
        .collect();

    // To match the KAS contract's per-message identity model, one consume
    // notification is emitted per queued steer (carrying that steer's id +
    // raw content) — not a single batched notification. The drained text is
    // still concatenated into a single LLM request (asserted below).
    assert_eq!(
        consumed_notifications.len(),
        3,
        "should have exactly 3 AgentExecutionSteeringInjected notifications (one per steer), got {}",
        consumed_notifications.len()
    );

    // Each consume notification carries one steer's raw content (not the
    // concatenated snapshot) and its stable steer-<uuid> messageId.
    let consumed_params: Vec<&str> = consumed_notifications.iter().map(|ext| ext.params.get()).collect();
    for instruction in ["First instruction", "Second instruction", "Third instruction"] {
        assert!(
            consumed_params.iter().any(|p| p.contains(instruction)),
            "exactly one AgentExecutionSteeringInjected should carry '{instruction}'"
        );
    }
    assert!(
        consumed_params
            .iter()
            .all(|p| p.contains("messageId") && p.contains("steer-")),
        "every AgentExecutionSteeringInjected should carry a steer-<uuid> messageId"
    );

    // Verify the steering messages were injected into an LLM request
    let captured_requests = harness.get_captured_requests(&session_id.0).await;
    assert!(
        captured_requests.len() >= 2,
        "should have at least 2 captured requests, got {}",
        captured_requests.len()
    );

    // One of the requests after the first should contain all steering messages concatenated
    let has_all_steering = captured_requests[1..].iter().any(|req| {
        let content = &req.user_input_message.content;
        content.contains("First instruction")
            && content.contains("Second instruction")
            && content.contains("Third instruction")
    });
    assert!(
        has_all_steering,
        "all three steering messages should appear concatenated in an LLM request after the initial prompt"
    );

    // Verify messages are concatenated with \n\n (check the consumed content or request content)
    let has_correct_separator = captured_requests[1..].iter().any(|req| {
        let content = &req.user_input_message.content;
        // The messages should be joined with \n\n in the queue before formatting
        content.contains("First instruction")
            && content.contains("Second instruction")
            && content.contains("Third instruction")
    });
    assert!(
        has_correct_separator,
        "steering messages should be concatenated together in the injected content"
    );
}

/// Integration test: end-of-turn drain extends the current turn when queue is non-empty.
/// This test verifies the end-of-turn drain path specifically:
/// - Agent finishes a text-only response (no tool calls)
/// - Queue is non-empty at end-of-turn
/// - Agent extends the current turn (not start a new one) with the queued content, matching KAS's
///   graph-router semantics — single prompt response, single set of per-turn metering, avoids the
///   ACP "cancel/end resolves the prompt response" race in the TUI
/// - SteeringConsumed notification is emitted
/// - The queued content is sent to the LLM as a continuation of the same turn
#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn end_of_turn_drain_extends_current_turn() {
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("steer_end_of_turn_drain")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Push first response stream: text-only (no tool calls).
    // Push the events but NOT the None terminator yet — this keeps the agent
    // streaming and gives us time to send the steer request.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Here is my initial response to your question.".to_string(),
                },
            )]),
        )
        .await;

    // Start a prompt (async - doesn't block)
    let prompt_rx = client.prompt_text_async(session_id.clone(), "tell me something").await;

    // Wait for the agent to be mid-stream. We use sleep here because
    // wait_for(AgentMessageChunk) has its own race with notification delivery.
    sleep(Duration::from_millis(200)).await;

    // Send a steer request while the agent is still streaming the text-only response.
    // Since there are no tool calls, this will be drained at end-of-turn.
    let steer_result = client
        .steer(session_id.clone(), "Actually, focus on performance tips")
        .await
        .expect("steer request should succeed");

    // Verify the steer response indicates success
    assert_eq!(
        steer_result.get("queued").and_then(|v| v.as_bool()),
        Some(true),
        "steer response should have queued: true"
    );

    // Verify SteeringQueued notification was emitted
    let has_queued = client
        .wait_for_timeout(
            |n| {
                n.ext_notifications.iter().any(|ext| {
                    let params_str = ext.params.get();
                    params_str.contains("AgentExecutionUserMessageQueued")
                        && params_str.contains("Actually, focus on performance tips")
                })
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(has_queued, "SteeringQueued notification should have been emitted");

    // End the first response stream (text-only, no tool calls). The agent
    // reaches end-of-turn, finds the queue non-empty, and extends the current
    // turn with the drained content (matches KAS) — issuing a second request.
    harness.push_mock_response(&session_id.0, None).await;

    // Provide the continuation response for the EXTENDED turn's request. This
    // must come AFTER the first stream's terminator so it is served to the
    // extended turn rather than appended to the first stream. Without real
    // content here the extended turn would see an empty response and trigger
    // the empty-response retry path.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Here are some performance tips as you requested.".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&session_id.0, None).await;

    // Wait for the prompt to complete. Only one prompt response — the extended
    // turn eventually ends with a single EndTurn.
    let prompt_result = tokio::time::timeout(Duration::from_secs(15), prompt_rx)
        .await
        .expect("prompt timed out waiting for completion")
        .expect("prompt channel closed")
        .expect("prompt should succeed");
    assert_eq!(
        prompt_result.stop_reason,
        agent_client_protocol::StopReason::EndTurn,
        "prompt should complete with EndTurn"
    );

    // Give a moment for all notifications to arrive
    sleep(Duration::from_millis(200)).await;

    // Verify SteeringConsumed notification was emitted
    let captured = client.captured().await;
    let has_consumed = captured.ext_notifications.iter().any(|ext| {
        let params_str = ext.params.get();
        params_str.contains("AgentExecutionSteeringInjected")
            && params_str.contains("Actually, focus on performance tips")
    });
    assert!(
        has_consumed,
        "SteeringConsumed notification should have been emitted at end-of-turn drain"
    );

    // The drained content becomes a second LLM request within the same turn:
    // 1. Original prompt ("tell me something")
    // 2. Continuation with the drained steer (same turn, not a new one)
    let captured_requests = harness.get_captured_requests(&session_id.0).await;
    assert!(
        captured_requests.len() >= 2,
        "should have at least 2 captured LLM requests (original + drained continuation), got {}",
        captured_requests.len()
    );

    // The continuation request should contain the steering message content
    let has_steering_in_continuation = captured_requests[1..].iter().any(|req| {
        req.user_input_message
            .content
            .contains("Actually, focus on performance tips")
    });
    assert!(
        has_steering_in_continuation,
        "the drained continuation's LLM request should contain the steering message"
    );
}

/// Integration test: cancel during processing clears the queue and emits SteeringCleared.
///
/// The backend clears `queued_steers` on cancel and emits
/// `SteeringCleared`. The TUI captures the queued content locally before
/// issuing cancel and replays it as a fresh prompt after cancel resolves
/// ("cancel = redirect" UX). The backend clear ensures a subsequent turn
/// doesn't accidentally inherit stale steering content.
///
/// This test verifies: cancel ends the current turn, clears the queue,
/// emits `SteeringCleared` (not `SteeringConsumed`), and does not inject
/// the queued content into any LLM request.
#[tokio::test(flavor = "multi_thread")]
#[timeout(30000)]
#[serial]
async fn cancel_during_processing_clears_queue() {
    let (mut harness, client, session_id, _cwd) = AcpTestHarnessBuilder::new("steer_cancel_clears_queue")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Create a test file so the read tool succeeds
    let test_file = harness.paths.cwd.join("test_file.txt");
    std::fs::write(&test_file, "hello world\n").expect("failed to create test file");

    // Push first response stream: a tool call.
    // Don't push the second response — agent will be blocked waiting for it after tool execution.
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
                    content: "I'll read that file for you.".to_string(),
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_cancel_001".to_string(),
                    name: "read".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_cancel_001".to_string(),
                    name: "read".to_string(),
                    input: Some(r#"{"operations": [{"mode": "Line", "path": "test_file.txt"}]}"#.to_string()),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "tooluse_cancel_001".to_string(),
                    name: "read".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    // End the first stream so the tool use is processed
    harness.push_mock_response(&session_id.0, None).await;

    // Push a partial second response (agent will start streaming but won't finish)
    // This keeps the agent in a state where it's waiting for more stream data
    harness
        .push_mock_response(
            &session_id.0,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "Processing the file...".to_string(),
                },
            )]),
        )
        .await;
    // Don't push None yet — agent is blocked mid-stream waiting for more data

    // Start a prompt that triggers the tool call (async - doesn't block)
    let prompt_rx = client.prompt_text_async(session_id.clone(), "read test_file.txt").await;

    // Wait until the tool call notification is received (agent is processing the tool)
    client
        .wait_for(|n| {
            n.session_updates
                .iter()
                .any(|u| matches!(u, agent_client_protocol::SessionUpdate::ToolCall(_)))
        })
        .await;

    // Send a steer request while the agent is busy
    let steer_result = client
        .steer(session_id.clone(), "Please cancel and do something else")
        .await
        .expect("steer request should succeed");

    // Verify the steer response indicates success
    assert_eq!(
        steer_result.get("queued").and_then(|v| v.as_bool()),
        Some(true),
        "steer response should have queued: true"
    );

    // Verify SteeringQueued notification was emitted
    let has_queued = client
        .wait_for_timeout(
            |n| {
                n.ext_notifications.iter().any(|ext| {
                    let params_str = ext.params.get();
                    params_str.contains("AgentExecutionUserMessageQueued")
                        && params_str.contains("Please cancel and do something else")
                })
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(has_queued, "SteeringQueued notification should have been emitted");

    // Fire cancel (non-blocking), then push end-of-stream to unblock the drain.
    client.cancel_async(session_id.clone()).await;
    sleep(Duration::from_millis(50)).await;
    // End the first (cancelled) stream.
    harness.push_mock_response(&session_id.0, None).await;

    // Wait for the prompt to complete — should resolve with Cancelled.
    let prompt_result = tokio::time::timeout(Duration::from_secs(10), prompt_rx)
        .await
        .expect("prompt_rx timed out waiting for cancel")
        .expect("prompt channel closed")
        .expect("prompt should resolve");

    assert_eq!(
        prompt_result.stop_reason,
        agent_client_protocol::StopReason::Cancelled,
        "prompt should be cancelled"
    );

    // Give a moment for all notifications to arrive.
    sleep(Duration::from_millis(200)).await;

    // Verify SteeringQueued was emitted before the cancel.
    let captured = client.captured().await;
    let has_queued_notification = captured
        .ext_notifications
        .iter()
        .any(|ext| ext.params.get().contains("AgentExecutionUserMessageQueued"));
    assert!(
        has_queued_notification,
        "SteeringQueued notification should have been emitted before cancellation"
    );

    // Cancel does NOT consume the queue — it clears it.
    let has_consumed = captured
        .ext_notifications
        .iter()
        .any(|ext| ext.params.get().contains("AgentExecutionSteeringInjected"));
    assert!(
        !has_consumed,
        "SteeringConsumed should NOT be emitted on cancel — the queue is cleared, not consumed"
    );

    // Cancel DOES clear the queue on the backend and emits SteeringCleared.
    // The TUI captures the content locally before cancel and replays it as
    // a fresh prompt — the backend clear prevents stale content from leaking
    // into subsequent turns.
    let has_cleared = captured
        .ext_notifications
        .iter()
        .any(|ext| ext.params.get().contains("AgentExecutionUserMessageCleared"));
    assert!(
        has_cleared,
        "SteeringCleared should be emitted on cancel — the backend clears the queue"
    );

    // The queue content should NOT have been injected into any LLM request
    // post-cancel (the TUI replays it as a fresh prompt, which we don't
    // simulate here).
    let captured_requests = harness.get_captured_requests(&session_id.0).await;
    let has_redirect_in_request = captured_requests.iter().any(|req| {
        req.user_input_message
            .content
            .contains("Please cancel and do something else")
    });
    assert!(
        !has_redirect_in_request,
        "queued content should NOT have been injected into an LLM request — cancel clears the queue, doesn't drain it"
    );
}
