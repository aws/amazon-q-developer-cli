//! End-to-end coverage for the crew stall timer's child-progress bridge.
//!
//! Lives in its own integration-test binary because it drives a full grouped
//! subagent through the real `chat_cli_v2 acp` subprocess, pacing the child's
//! activity over real wall-clock time to exercise the parent's `agent_crew`
//! blocking stall loop.
//!
//! The stall timer is an idle window on genuine child progress: children ping
//! the SessionManager on every UpdateEvent (assistant tokens, tool calls, tool
//! results), the parent polls the group's freshest activity, and the window
//! resets whenever it advances. These tests prove both halves of that contract:
//!
//!   1. a child that keeps producing activity past several stall windows is NEVER cancelled (the
//!      reset fires), and
//!   2. a child that goes silent longer than the window IS cancelled (the timer still trips on a
//!      true stall).

mod common;

use std::time::Duration;

use agent::agent_config::definitions::AgentConfigV2025_08_22;
use chat_cli_v2::agent::acp::extensions::methods;
use chat_cli_v2::api_client::model::ChatResponseStream;
use chat_cli_v2::api_client::send_message_output::MockStreamItem;
use common::AcpTestHarnessBuilder;
use ntest::timeout;
use serial_test::serial;

/// Poll the captured subagent-list-update notifications for the first subagent
/// belonging to a crew group, returning its session id. The parent's
/// `agent_crew` tool spawns children with random UUIDs, so the test learns the
/// child's id from this notification rather than pre-computing it.
async fn wait_for_group_child_session_id(client: &common::AcpTestClient, timeout: Duration) -> Option<String> {
    // Notifications arrive with the extension method's leading `_` stripped. Fail
    // loudly if the constant loses that prefix rather than degrading to an empty
    // match that would silently never match and surface as a confusing timeout.
    let method = methods::SUBAGENT_LIST_UPDATE.strip_prefix('_').unwrap_or_else(|| {
        panic!(
            "SUBAGENT_LIST_UPDATE ('{}') must start with '_'",
            methods::SUBAGENT_LIST_UPDATE
        )
    });
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let captured = client.captured().await;
        for n in captured.ext_notifications.iter().rev() {
            if n.method.as_ref() == method {
                let params: serde_json::Value = serde_json::from_str(n.params.get()).unwrap_or_default();
                if let Some(subagents) = params.get("subagents").and_then(|v| v.as_array()) {
                    for s in subagents {
                        let in_group = s
                            .get("group")
                            .and_then(|v| v.as_str())
                            .is_some_and(|g| g.starts_with("crew-"));
                        if in_group && let Some(sid) = s.get("sessionId").and_then(|v| v.as_str()) {
                            return Some(sid.to_string());
                        }
                    }
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Poll the captured subagent-list-update notifications until `n` crew-group
/// children are visible, returning a stage-name → session-id map.
async fn wait_for_group_children(
    client: &common::AcpTestClient,
    n: usize,
    timeout: Duration,
) -> Option<std::collections::HashMap<String, String>> {
    let method = methods::SUBAGENT_LIST_UPDATE.strip_prefix('_').unwrap_or_else(|| {
        panic!(
            "SUBAGENT_LIST_UPDATE ('{}') must start with '_'",
            methods::SUBAGENT_LIST_UPDATE
        )
    });
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let captured = client.captured().await;
        for notif in captured.ext_notifications.iter().rev() {
            if notif.method.as_ref() == method {
                let params: serde_json::Value = serde_json::from_str(notif.params.get()).unwrap_or_default();
                if let Some(subagents) = params.get("subagents").and_then(|v| v.as_array()) {
                    let mut map = std::collections::HashMap::new();
                    for s in subagents {
                        let in_group = s
                            .get("group")
                            .and_then(|v| v.as_str())
                            .is_some_and(|g| g.starts_with("crew-"));
                        if in_group
                            && let (Some(name), Some(sid)) = (
                                s.get("sessionName").and_then(|v| v.as_str()),
                                s.get("sessionId").and_then(|v| v.as_str()),
                            )
                        {
                            map.insert(name.to_string(), sid.to_string());
                        }
                    }
                    if map.len() >= n {
                        return Some(map);
                    }
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A crew driver agent that can run the `agent_crew` tool, plus the worker agent
/// each stage runs. `availableAgents`/`trustedAgents` on the crew tool settings
/// let the parent both see and auto-approve the worker stage.
fn crew_and_worker_configs() -> (AgentConfigV2025_08_22, AgentConfigV2025_08_22) {
    let driver: AgentConfigV2025_08_22 = serde_json::from_value(serde_json::json!({
        "name": "crew_driver",
        "tools": ["*"],
        "toolsSettings": {
            "crew": {
                "availableAgents": ["stall_worker"],
                "trustedAgents": ["stall_worker"],
            }
        }
    }))
    .expect("crew_driver config");

    let worker = AgentConfigV2025_08_22 {
        name: "stall_worker".to_string(),
        tools: vec!["*".to_string()],
        ..Default::default()
    };
    (driver, worker)
}

/// The parent's single mock turn: call `agent_crew` (name `subagent`) with one
/// stage that runs `stall_worker`. The parent then blocks in `await_blocking`
/// until the group completes or the stall timer trips.
async fn push_parent_crew_turn(harness: &mut common::AcpTestHarness, parent_session_id: &str) {
    let crew_input = serde_json::json!({
        "task": "exercise the stall timer",
        "stages": [{
            "name": "worker",
            "role": "stall_worker",
            "prompt_template": "{task}"
        }]
    })
    .to_string();

    harness
        .push_mock_response(
            parent_session_id,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "crew_1".to_string(),
                    name: "subagent".to_string(),
                    input: None,
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "crew_1".to_string(),
                    name: "subagent".to_string(),
                    input: Some(crew_input),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "crew_1".to_string(),
                    name: "subagent".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(parent_session_id, None).await;

    // After the crew tool returns, the parent ends its turn with a plain message.
    harness
        .push_mock_response(
            parent_session_id,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "crew done".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(parent_session_id, None).await;
}

/// PROVES THE RESET: a child that streams fresh activity every ~200ms for
/// longer than a full stall window must never be cancelled. The stall window
/// is 3s; the child stays in a single long turn producing content chunks for
/// ~5s (past one and a half windows) before calling `summary`. If the bridge
/// were a no-op (the pre-existing bug) or a flat wall-clock cap, the parent
/// would cancel at 3s and the crew tool would report a deadline; here it must
/// complete.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn crew_stall_timer_resets_on_child_progress() {
    let (driver, worker) = crew_and_worker_configs();

    let (mut harness, client, parent_session_id, _cwd) = AcpTestHarnessBuilder::new("crew_stall_reset")
        .with_agent_config("crew_driver", &driver)
        .with_agent_config("stall_worker", &worker)
        .with_setting("chat.defaultAgent", "crew_driver")
        // 3s idle window: the ~200ms activity cadence keeps resetting it with a
        // ~2.8s margin, so a scheduling hiccup on a loaded runner cannot trip a
        // spurious cancellation across the process boundary. Injected explicitly
        // on the spawned process (the env override wins over the setting and any
        // ambient KIRO_SUBAGENT_STALL_TIMEOUT_MS the runner inherits), so the
        // window is pinned regardless of the runner's environment — keeps the
        // test hermetic.
        .with_env("KIRO_SUBAGENT_STALL_TIMEOUT_MS", "3000")
        .with_trust_all(true)
        .build_with_session()
        .await;

    push_parent_crew_turn(&mut harness, parent_session_id.0.as_ref()).await;

    // Kick the parent turn; it will call agent_crew and block. Don't await it yet.
    let parent_done = client
        .prompt_text_async(parent_session_id.clone(), "run the crew")
        .await;

    // Learn the spawned child's session id from the subagent-list-update.
    let child_sid = wait_for_group_child_session_id(&client, Duration::from_secs(30))
        .await
        .expect("crew child session id should appear in a subagent list update");

    // Feed the child a single long turn: many small assistant chunks, each ~200ms
    // apart. Every chunk is an UpdateEvent that pings the SessionManager, so the
    // parent's stall window (3s) keeps resetting across ~5s of real time — past
    // where a non-resetting window would have cancelled. The stream stays open
    // (no None terminator) until we finally push the summary call, so the child
    // never leaves this turn early.
    for i in 0..25 {
        harness
            .push_mock_response(
                &child_sid,
                Some(vec![MockStreamItem::Event(
                    ChatResponseStream::AssistantResponseEvent {
                        content: format!("progress chunk {i}\n"),
                    },
                )]),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Now let the child finish this turn by calling summary, then end cleanly.
    harness
        .push_mock_response(
            &child_sid,
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
                        r#"{"taskDescription":"stall test","taskResult":"kept making progress the whole time"}"#
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
    harness.push_mock_response(&child_sid, None).await;
    // Trailing empty turn so the child loop finishes cleanly after summary.
    harness
        .push_mock_response(
            &child_sid,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "done".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&child_sid, None).await;

    // The parent turn must complete (the crew tool returned a real result, not a
    // stall). If the reset had failed, the crew tool would have cancelled the
    // child at ~1s and returned a deadline summary long before the child called
    // summary at ~4s — and this await would still succeed but the crew tool
    // result would say "deadline expired". We assert on the tool result below.
    parent_done
        .await
        .expect("parent prompt channel dropped")
        .expect("parent prompt should complete");

    // The crew tool result flows back into the parent's next LLM request as a
    // tool_result in history. A successful pipeline says "Pipeline completed";
    // a stalled one says the "deadline" expired. Assert we completed.
    let captured = harness.get_captured_requests(parent_session_id.0.as_ref()).await;
    let crew_result_text = extract_tool_result_text(&captured, "crew_1");
    assert!(
        crew_result_text.contains("Pipeline completed"),
        "crew should complete on sustained child progress, not stall; got: {crew_result_text:?}"
    );
    assert!(
        !crew_result_text.contains("deadline"),
        "sustained progress must not trip the stall timer; got: {crew_result_text:?}"
    );
}

/// PROVES THE TIMER STILL FIRES: a child that goes silent (never streams past
/// its initial spawn) for longer than the stall window must be cancelled. Here
/// the child is given no mock responses at all, so it blocks on its first
/// `send_message` with zero activity; the parent's 1s window elapses and the
/// crew tool cancels and returns a deadline summary.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn crew_stall_timer_trips_on_silent_child() {
    let (driver, worker) = crew_and_worker_configs();

    let (mut harness, client, parent_session_id, _cwd) = AcpTestHarnessBuilder::new("crew_stall_trip")
        .with_agent_config("crew_driver", &driver)
        .with_agent_config("stall_worker", &worker)
        .with_setting("chat.defaultAgent", "crew_driver")
        // Pin the 1s window on the spawned process so an ambient
        // KIRO_SUBAGENT_STALL_TIMEOUT_MS the runner inherits cannot leak in and change
        // it; the env override wins over the setting, keeping this test hermetic.
        .with_env("KIRO_SUBAGENT_STALL_TIMEOUT_MS", "1000")
        .with_trust_all(true)
        .build_with_session()
        .await;

    push_parent_crew_turn(&mut harness, parent_session_id.0.as_ref()).await;

    // Deliberately push NO child responses: the child spawns, sends its first
    // request, and blocks with no streamed activity — a genuine stall. The 1s
    // window must elapse and the crew tool must cancel it.
    let parent_done = client
        .prompt_text_async(parent_session_id.clone(), "run the crew")
        .await;

    parent_done
        .await
        .expect("parent prompt channel dropped")
        .expect("parent prompt should complete after the stall cancel");

    // The crew tool result must report the expired deadline (the cancel-and-
    // summarize path), proving the timer fired on a truly silent child.
    let captured = harness.get_captured_requests(parent_session_id.0.as_ref()).await;
    let crew_result_text = extract_tool_result_text(&captured, "crew_1");
    assert!(
        crew_result_text.contains("idle deadline"),
        "a silent child must trip the stall timer and be cancelled; got: {crew_result_text:?}"
    );
    // The cancellation must identify the cancelled work: the unfinished stage is
    // named, with an explicit cancellation note instead of a generic "No result".
    assert!(
        crew_result_text.contains("worker") && crew_result_text.contains("Cancelled by the idle deadline"),
        "the cancelled stage must be named with an explicit cancellation note; got: {crew_result_text:?}"
    );
    assert!(
        !crew_result_text.contains("No result"),
        "cancelled stages must not surface as a generic 'No result'; got: {crew_result_text:?}"
    );
}

/// PROVES PER-STAGE ISOLATION: with one busy stage and one silent stage, the
/// silent stage is cancelled on ITS OWN 3s idle window while the busy stage —
/// still streaming well past that window — keeps running, completes, and keeps
/// its real result. The old group-scoped window failed in both directions: the
/// busy sibling's activity masked the silent stage, and the eventual trip
/// cancelled the busy sibling as collateral.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn crew_stall_timer_cancels_only_the_stalled_stage() {
    let (driver, worker) = crew_and_worker_configs();

    let (mut harness, client, parent_session_id, _cwd) = AcpTestHarnessBuilder::new("crew_stall_per_stage")
        .with_agent_config("crew_driver", &driver)
        .with_agent_config("stall_worker", &worker)
        .with_setting("chat.defaultAgent", "crew_driver")
        // 3s idle window: wide enough that a scheduling hiccup in the busy
        // stage's ~200ms feed cadence cannot spuriously cancel it, while the
        // silent stage still trips within a few seconds.
        .with_env("KIRO_SUBAGENT_STALL_TIMEOUT_MS", "3000")
        .with_trust_all(true)
        .build_with_session()
        .await;

    // Two independent stages of the same worker role: one will stream, one
    // will stay silent from spawn.
    let crew_input = serde_json::json!({
        "task": "exercise per-stage isolation",
        "stages": [
            { "name": "busy_stage", "role": "stall_worker", "prompt_template": "{task}" },
            { "name": "quiet_stage", "role": "stall_worker", "prompt_template": "{task}" }
        ]
    })
    .to_string();
    harness
        .push_mock_response(
            parent_session_id.0.as_ref(),
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "crew_1".to_string(),
                    name: "subagent".to_string(),
                    input: Some(crew_input),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "crew_1".to_string(),
                    name: "subagent".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(parent_session_id.0.as_ref(), None).await;
    harness
        .push_mock_response(
            parent_session_id.0.as_ref(),
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "crew done".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(parent_session_id.0.as_ref(), None).await;

    let parent_done = client
        .prompt_text_async(parent_session_id.clone(), "run the crew")
        .await;

    let children = wait_for_group_children(&client, 2, Duration::from_secs(30))
        .await
        .expect("both crew children should appear in a subagent list update");
    let busy_sid = children.get("busy_stage").expect("busy_stage session id").clone();
    // quiet_stage deliberately gets NO mock responses: silent from spawn.

    // The busy stage streams a chunk every ~200ms for ~5s — well past the 3s
    // window the quiet stage is meanwhile burning through, so the quiet stage's
    // cancellation lands while the busy stage is demonstrably still working.
    for i in 0..25 {
        harness
            .push_mock_response(
                &busy_sid,
                Some(vec![MockStreamItem::Event(
                    ChatResponseStream::AssistantResponseEvent {
                        content: format!("busy progress {i}\n"),
                    },
                )]),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    harness
        .push_mock_response(
            &busy_sid,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "busy_summary_1".to_string(),
                    name: "summary".to_string(),
                    input: Some(
                        r#"{"taskDescription":"busy stage","taskResult":"busy stage real result"}"#.to_string(),
                    ),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "busy_summary_1".to_string(),
                    name: "summary".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(&busy_sid, None).await;
    harness
        .push_mock_response(
            &busy_sid,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "done".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&busy_sid, None).await;

    parent_done
        .await
        .expect("parent prompt channel dropped")
        .expect("parent prompt should complete");

    let captured = harness.get_captured_requests(parent_session_id.0.as_ref()).await;
    let crew_result_text = extract_tool_result_text(&captured, "crew_1");
    assert!(
        crew_result_text.contains("busy stage real result"),
        "the busy stage must survive the quiet sibling's deadline and keep its result; got: {crew_result_text:?}"
    );
    assert!(
        crew_result_text.contains("quiet_stage") && crew_result_text.contains("Cancelled by the idle deadline"),
        "the quiet stage must be cancelled alone, with an explicit note; got: {crew_result_text:?}"
    );
    assert!(
        crew_result_text.contains("cancelled by the idle deadline"),
        "the header must report the partial cancellation; got: {crew_result_text:?}"
    );
}

/// PROVES THE HUMAN-WAIT HOLD: a child parked on a tool-approval prompt is
/// waiting on a human, not stalled. The stall window is 1s and the "human"
/// takes ~3s (three windows) to approve; without the human-wait hold the crew
/// tool would cancel the child mid-approval and report a deadline. With it,
/// the approval lands, the tool runs, and the pipeline completes.
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn crew_stall_timer_holds_while_child_awaits_approval() {
    use crate::common::PermissionResponse;

    // The driver trusts its own crew tool (so the parent turn needs no prompt)
    // and the worker trusts only `summary`, so its `fs_write` must prompt.
    let driver: AgentConfigV2025_08_22 = serde_json::from_value(serde_json::json!({
        "name": "crew_driver",
        "tools": ["*"],
        "allowedTools": ["subagent"],
        "toolsSettings": {
            "crew": {
                "availableAgents": ["stall_worker"],
                "trustedAgents": ["stall_worker"],
            }
        }
    }))
    .expect("crew_driver config");
    let worker: AgentConfigV2025_08_22 = serde_json::from_value(serde_json::json!({
        "name": "stall_worker",
        "tools": ["*"],
        "allowedTools": ["summary"]
    }))
    .expect("stall_worker config");

    let (mut harness, client, parent_session_id, cwd) = AcpTestHarnessBuilder::new("crew_stall_approval_hold")
        .with_agent_config("crew_driver", &driver)
        .with_agent_config("stall_worker", &worker)
        .with_setting("chat.defaultAgent", "crew_driver")
        // Pin the 1s window on the spawned process (env wins over setting and any
        // ambient KIRO_SUBAGENT_STALL_TIMEOUT_MS), keeping the test hermetic.
        .with_env("KIRO_SUBAGENT_STALL_TIMEOUT_MS", "1000")
        .build_with_session()
        .await;

    // The "human" takes three full stall windows to approve the child's tool.
    client
        .queue_permission_response(PermissionResponse::DelayedSelect {
            option_id: "allow_once".to_string(),
            delay: Duration::from_millis(3000),
        })
        .await;

    push_parent_crew_turn(&mut harness, parent_session_id.0.as_ref()).await;

    let parent_done = client
        .prompt_text_async(parent_session_id.clone(), "run the crew")
        .await;

    let child_sid = wait_for_group_child_session_id(&client, Duration::from_secs(30))
        .await
        .expect("crew child session id should appear in a subagent list update");

    // The child's first turn: an untrusted fs_write, which prompts for approval
    // and parks the child on the (slow) human for ~3 windows.
    let target = cwd.join("approved.txt");
    let fs_write_input = serde_json::json!({
        "command": "create",
        "path": target.to_string_lossy(),
        "content": "approved"
    })
    .to_string();
    harness
        .push_mock_response(
            &child_sid,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_write_1".to_string(),
                    name: "write".to_string(),
                    input: Some(fs_write_input),
                    stop: None,
                }),
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_write_1".to_string(),
                    name: "write".to_string(),
                    input: None,
                    stop: Some(true),
                }),
            ]),
        )
        .await;
    harness.push_mock_response(&child_sid, None).await;

    // After the (eventually approved) write, the child summarizes and finishes.
    harness
        .push_mock_response(
            &child_sid,
            Some(vec![
                MockStreamItem::Event(ChatResponseStream::ToolUseEvent {
                    tool_use_id: "sub_summary_1".to_string(),
                    name: "summary".to_string(),
                    input: Some(
                        r#"{"taskDescription":"approval test","taskResult":"write approved and applied"}"#.to_string(),
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
    harness.push_mock_response(&child_sid, None).await;
    harness
        .push_mock_response(
            &child_sid,
            Some(vec![MockStreamItem::Event(
                ChatResponseStream::AssistantResponseEvent {
                    content: "done".to_string(),
                },
            )]),
        )
        .await;
    harness.push_mock_response(&child_sid, None).await;

    parent_done
        .await
        .expect("parent prompt channel dropped")
        .expect("parent prompt should complete");

    // The approval must actually have been requested (otherwise this test isn't
    // exercising the human-wait path at all)...
    let captured = client.captured().await;
    assert!(
        !captured.permission_requests.is_empty(),
        "the child's untrusted fs_write should have raised a permission request"
    );

    // ...and the crew must have completed rather than cancelling mid-approval.
    let captured_reqs = harness.get_captured_requests(parent_session_id.0.as_ref()).await;
    let crew_result_text = extract_tool_result_text(&captured_reqs, "crew_1");
    assert!(
        crew_result_text.contains("Pipeline completed"),
        "a child awaiting human approval must not be cancelled as stalled; got: {crew_result_text:?}"
    );
    assert!(
        !crew_result_text.contains("deadline"),
        "the stall timer must hold while an approval is pending; got: {crew_result_text:?}"
    );
}

/// Pull the text of the tool_result for `tool_use_id` out of any captured
/// request (the crew tool's output is injected as a tool_result on the parent's
/// follow-up request — either as the live `user_input_message` or, on later
/// turns, in `history`).
fn extract_tool_result_text(
    captured: &[chat_cli_v2::api_client::model::ConversationState],
    tool_use_id: &str,
) -> String {
    use chat_cli_v2::api_client::model::{
        ChatMessage,
        ToolResultContentBlock,
        UserInputMessage,
    };

    fn from_user_message(user: &UserInputMessage, tool_use_id: &str) -> Option<String> {
        let ctx = user.user_input_message_context.as_ref()?;
        let tool_results = ctx.tool_results.as_ref()?;
        let tr = tool_results.iter().find(|tr| tr.tool_use_id == tool_use_id)?;
        let mut text = String::new();
        for block in &tr.content {
            if let ToolResultContentBlock::Text(t) = block {
                text.push_str(t);
            }
        }
        Some(text)
    }

    for conv in captured {
        if let Some(text) = from_user_message(&conv.user_input_message, tool_use_id) {
            return text;
        }
        if let Some(history) = &conv.history {
            for msg in history {
                if let ChatMessage::UserInputMessage(user) = msg
                    && let Some(text) = from_user_message(user, tool_use_id)
                {
                    return text;
                }
            }
        }
    }
    String::new()
}
