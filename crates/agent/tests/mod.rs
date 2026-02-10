mod common;

use std::time::Duration;

use agent::agent_config::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
    CommandHook,
    HookConfig,
    HookTrigger,
};
use agent::agent_config::types::ResourcePath;
use agent::agent_loop::types::{
    ContentBlock,
    Role,
    ToolResultContentBlock,
};
use agent::protocol::{
    AgentEvent,
    ApprovalResult,
    CompactionEvent,
    PermissionOptionId,
    SendApprovalResultArgs,
};
use agent::{
    ActiveState,
    SKILL_FILES_MESSAGE,
};
use common::*;

/// Tests that skill:// resources only include frontmatter metadata in context,
/// while file:// resources include full content.
#[tokio::test]
async fn test_mixed_file_and_skill_resources() {
    let _ = tracing_subscriber::fmt::try_init();

    const REGULAR_FILE_CONTENT: &str = "# Regular File\nThis is the full content of the regular file.";
    const SKILL_FILE_CONTENT: &str = "---\nname: database-helper\ndescription: Helps with database queries and schema design\n---\n# Database Helper Skill\nThis is the full skill content that should NOT appear.";

    // Create agent config with both file:// and skill:// resources
    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        resources: vec![
            ResourcePath::FilePath("file://README.md".to_string()),
            ResourcePath::Skill("skill://skills/db-helper.md".to_string()),
        ],
        ..Default::default()
    });

    let mut test = TestCase::builder()
        .test_name("mixed file and skill resources")
        .with_agent_config(agent_config)
        .with_file(("README.md", REGULAR_FILE_CONTENT))
        .with_file(("skills/db-helper.md", SKILL_FILE_CONTENT))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Get the first request sent to the model
    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let first_msg = requests[0]
        .messages()
        .first()
        .expect("first message should exist")
        .text();

    // Regular file:// resource should have FULL content
    assert!(
        first_msg.contains(REGULAR_FILE_CONTENT),
        "expected regular file full content in context"
    );

    // Skill:// resource should have hint format
    assert!(
        first_msg.contains("database-helper: Helps with database queries and schema design"),
        "expected skill hint with name and description"
    );
    assert!(
        first_msg.contains("(file:") && first_msg.contains("db-helper.md)"),
        "expected skill hint with file path"
    );

    // SKILL_FILES_MESSAGE should be present
    assert!(
        first_msg.contains(SKILL_FILES_MESSAGE),
        "expected SKILL_FILES_MESSAGE instruction"
    );

    // Verify order: SKILL_FILES_MESSAGE comes before skill entry
    let msg_pos = first_msg.find(SKILL_FILES_MESSAGE).unwrap();
    let skill_pos = first_msg.find("database-helper:").unwrap();
    assert!(
        msg_pos < skill_pos,
        "SKILL_FILES_MESSAGE should appear before skill entries"
    );

    // Skill:// resource should NOT have body content
    assert!(
        !first_msg.contains("# Database Helper Skill"),
        "skill heading should NOT be in context"
    );
    assert!(
        !first_msg.contains("This is the full skill content that should NOT appear"),
        "skill body content should NOT be in context"
    );
}

#[tokio::test]
async fn test_agent_defaults() {
    let _ = tracing_subscriber::fmt::try_init();

    const AMAZON_Q_MD_CONTENT: &str = "AmazonQ.md-FILE-CONTENT";
    const AGENTS_MD_CONTENT: &str = "AGENTS.md-FILE-CONTENT";
    const README_MD_CONTENT: &str = "README.md-FILE-CONTENT";
    const LOCAL_RULE_MD_CONTENT: &str = "local_rule.md-FILE-CONTENT";
    const SUB_LOCAL_RULE_MD_CONTENT: &str = "sub_local_rule.md-FILE-CONTENT";

    let mut test = TestCase::builder()
        .test_name("agent default config behavior")
        .with_default_agent_config()
        .with_file(("AmazonQ.md", AMAZON_Q_MD_CONTENT))
        .with_file(("AGENTS.md", AGENTS_MD_CONTENT))
        .with_file(("README.md", README_MD_CONTENT))
        .with_file((".amazonq/rules/local_rule.md", LOCAL_RULE_MD_CONTENT))
        .with_file((".amazonq/rules/subfolder/sub_local_rule.md", SUB_LOCAL_RULE_MD_CONTENT))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/builtin_tools.jsonl"))
                .await
                .unwrap(),
        )
        .with_tool_use_approvals([
            SendApprovalResultArgs {
                id: "tooluse_first".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_second".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_third".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
        ])
        .build()
        .await
        .unwrap();

    test.send_prompt("start turn".to_string()).await;

    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    for req in test.requests() {
        let first_msg = req.messages().first().expect("first message should exist").text();
        let assert_contains = |expected: &str| {
            assert!(
                first_msg.contains(expected),
                "expected to find '{}' inside content: '{}'",
                expected,
                first_msg
            );
        };
        assert_contains(AMAZON_Q_MD_CONTENT);
        assert_contains(AGENTS_MD_CONTENT);
        assert_contains(README_MD_CONTENT);
        assert_contains(LOCAL_RULE_MD_CONTENT);
        assert_contains(SUB_LOCAL_RULE_MD_CONTENT);
    }
}

#[tokio::test]
async fn test_log_entry_appended_events() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("log entry appended events")
        .with_default_agent_config()
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/builtin_tools.jsonl"))
                .await
                .unwrap(),
        )
        .with_tool_use_approvals([
            SendApprovalResultArgs {
                id: "tooluse_first".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_second".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_third".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
        ])
        .build()
        .await
        .unwrap();

    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let log_events = test.log_entry_appended_events();

    // Should have at least: 1 user prompt + assistant responses + tool results
    assert!(!log_events.is_empty(), "expected LogEntryAppended events to be emitted");

    // Verify indices are sequential
    let mut last_index = None;
    for evt in &log_events {
        if let agent::protocol::AgentEvent::LogEntryAppended { index, .. } = evt {
            if let Some(last) = last_index {
                assert_eq!(*index, last + 1, "log entry indices should be sequential");
            }
            last_index = Some(*index);
        }
    }
}

#[tokio::test]
async fn test_auto_compaction_on_context_overflow() {
    let _ = tracing_subscriber::fmt::try_init();

    // Responses: hello_ack -> tool use -> tool use -> context overflow -> compaction summary -> retry
    // success
    let responses = parse_response_streams(include_str!("./mock_responses/context_window_overflow.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("auto compaction on context overflow")
        .with_default_agent_config()
        .with_responses(responses)
        .with_trust_all_tools(true)
        .build()
        .await
        .unwrap();

    // First send hello and wait for response
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Then send the actual prompt that triggers tool uses and overflow
    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify compaction events were emitted
    let compaction_events = test.compaction_events();

    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started))),
        "expected CompactionEvent::Started"
    );
    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Completed))),
        "expected CompactionEvent::Completed"
    );

    // Verify the retry request (last one) has the ls tool result
    let retry_request = test.requests().last().unwrap();
    assert!(
        retry_request.has_tool_result(|tr| tr.tool_use_id.contains("kexAaD9RRkyTgeHlCu7bRA")),
        "retry request should contain ls tool result"
    );
}

#[tokio::test]
async fn test_manual_compaction() {
    let _ = tracing_subscriber::fmt::try_init();

    let responses = parse_response_streams(include_str!("./mock_responses/manual_compaction.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("manual compaction")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // Send initial prompt and wait for response
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Manually trigger compaction (last message is from assistant)
    test.compact_conversation().await.unwrap();
    test.wait_until_compaction_complete(Duration::from_secs(2)).await;

    // Verify compaction events were emitted
    let compaction_events = test.compaction_events();
    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started))),
        "expected CompactionEvent::Started"
    );
    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Completed))),
        "expected CompactionEvent::Completed"
    );

    // Verify agent is idle (no retry since last message was from assistant)
    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, agent::ActiveState::Idle),
        "expected agent to be idle after manual compaction"
    );
}

#[tokio::test]
async fn test_stop_hook_runs_synchronously() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut agent_config = AgentConfig::default();
    agent_config.add_hook(
        HookTrigger::Stop,
        HookConfig::ShellCommand(CommandHook {
            command: "sleep 1".to_string(), // sleep for 1 second
            opts: Default::default(),
        }),
    );

    let mut test = TestCase::builder()
        .test_name("stop hook runs synchronously")
        .with_agent_config(agent_config)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("test prompt".to_string()).await;
    assert!(matches!(
        test.create_snapshot().await.execution_state.active_state,
        ActiveState::ExecutingHooks(_)
    ));
    assert!(test.wait_until_agent_stop(Duration::from_millis(500)).await.is_err());

    tokio::time::sleep(Duration::from_secs(1)).await;

    test.wait_until_agent_stop(Duration::from_millis(500))
        .await
        .expect("stop hook should have finished");
}

#[tokio::test]
async fn test_allow_always_grants_exact_file_permission() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("allow_always grants exact file permission")
        .with_default_agent_config()
        .with_file(("subdir/file.txt", "content"))
        .with_file(("other/output.txt", "other content"))
        // Use a different CWD so test files aren't auto-allowed for read
        .with_cwd_subdir("unused_cwd")
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/allow_always_permissions.jsonl"))
                .await
                .unwrap(),
        )
        .with_tool_use_approvals([
            // First read tool - AllowAlwaysToolArgs grants exact file read permission
            SendApprovalResultArgs {
                id: "tooluse_read".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowAlwaysToolArgs,
                    reason: None,
                },
            },
            // Write tool - RejectAlwaysToolArgs denies exact file write (but NOT read)
            SendApprovalResultArgs {
                id: "tooluse_write".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::RejectAlwaysToolArgs,
                    reason: None,
                },
            },
            // tooluse_read2: second read of subdir/file.txt - auto-approved (AllowAlwaysToolArgs)
            // tooluse_read_denied: read of other/output.txt - needs approval (write deny doesn't deny read)
            SendApprovalResultArgs {
                id: "tooluse_read_denied".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                },
            },
        ])
        .build()
        .await
        .unwrap();

    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify we got 3 approval requests (first read, write, and read of other/output.txt)
    // - tooluse_read2 auto-approved because exact file has read permission
    // - tooluse_read_denied needs approval because RejectAlways on write does NOT deny read
    let approval_requests = test.approval_request_events();

    assert_eq!(
        approval_requests.len(),
        3,
        "expected 3 approval requests (read, write, read_denied), got {}: {:?}",
        approval_requests.len(),
        approval_requests
    );
}

/// Tests that canceling during SendingRequest or ConsumingResponse removes the user message
#[tokio::test]
async fn test_cancel_during_executing_request() {
    let _ = tracing_subscriber::fmt::try_init();

    // Create a response with a delay so we can cancel during execution
    let response_stream = parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
        .await
        .unwrap();

    let delayed_response =
        agent::agent_loop::model::MockResponse::with_delay(response_stream[0].clone(), Duration::from_secs(10));

    let mut test = TestCase::builder()
        .test_name("cancel during executing request")
        .with_default_agent_config()
        .with_mock_response(delayed_response)
        .build()
        .await
        .unwrap();

    // Send a prompt
    test.send_prompt("test prompt".to_string()).await;

    // Wait a bit then cancel while waiting for response
    tokio::time::sleep(Duration::from_millis(50)).await;
    test.cancel().await.unwrap();

    // Wait for cancellation to complete
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify the user message was removed from conversation history
    let mut snapshot = test.create_snapshot().await;
    let messages = snapshot.conversation_state.messages();

    assert_eq!(
        messages.len(),
        0,
        "expected 0 messages after cancel during execution, got {}: {:?}",
        messages.len(),
        messages
    );

    // Verify turn metadata was still saved
    assert_eq!(
        snapshot.conversation_metadata.user_turn_metadatas.len(),
        1,
        "expected 1 turn metadata entry"
    );
}

/// Tests that canceling after tool uses are generated adds cancelled tool result messages
#[tokio::test]
async fn test_cancel_with_pending_tool_uses() {
    let _ = tracing_subscriber::fmt::try_init();

    // Use a response that includes tool uses
    let response_stream = parse_response_streams(include_str!("./mock_responses/builtin_tools.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("cancel with pending tool uses")
        .with_default_agent_config()
        .with_responses(vec![response_stream[0].clone()])
        .build()
        .await
        .unwrap();

    // Send a prompt
    test.send_prompt("test prompt".to_string()).await;

    // Wait for approval request
    test.wait_until_agent_event(Duration::from_secs(2), |evt| {
        matches!(evt, AgentEvent::ApprovalRequest(_))
    })
    .await
    .unwrap();

    // Cancel before approving
    test.cancel().await.unwrap();

    // Wait for cancellation to complete
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify conversation state
    let mut snapshot = test.create_snapshot().await;
    let messages = snapshot.conversation_state.messages();

    // Should have: user message, assistant message with tool uses, user message with cancelled results,
    // assistant message
    assert_eq!(
        messages.len(),
        4,
        "expected 4 messages after cancel with tool uses, got {}: {:?}",
        messages.len(),
        messages
    );

    // Verify the third message contains cancelled tool results
    let tool_result_msg = &messages[2];
    assert_eq!(tool_result_msg.role, Role::User);
    let has_cancelled_result = tool_result_msg.content.iter().any(|c| {
        if let ContentBlock::ToolResult(result) = c {
            result.content.iter().any(|content| {
                if let ToolResultContentBlock::Text(text) = content {
                    text.contains("Tool use was cancelled by the user")
                } else {
                    false
                }
            })
        } else {
            false
        }
    });
    assert!(has_cancelled_result, "expected cancelled tool result message");

    // Verify the fourth message is the interruption message
    let interruption_msg = &messages[3];
    assert_eq!(interruption_msg.role, Role::Assistant);
    let has_interruption_text = interruption_msg.content.iter().any(|c| {
        if let ContentBlock::Text(text) = c {
            text.contains("Tool uses were interrupted")
        } else {
            false
        }
    });
    assert!(has_interruption_text, "expected interruption message");
}

async fn run_pretooluse_hook_matcher_test(matcher: &str) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let hook_log = temp_dir.path().join("hook_log.txt");
    let hook_log_str = hook_log.to_string_lossy().to_string();

    let mut test = TestCase::builder()
        .test_name(&format!("pretooluse hook matches {}", matcher))
        .with_default_agent_config()
        .with_hook(
            HookTrigger::PreToolUse,
            HookConfig::ShellCommand(CommandHook {
                command: format!("cat >> {}", hook_log_str),
                opts: agent::agent_config::definitions::BaseHookConfig {
                    matcher: Some(matcher.to_string()),
                    ..Default::default()
                },
            }),
        )
        .with_file(("test.txt", "content"))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/fs_read_only.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("read test.txt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let hook_output = std::fs::read_to_string(&hook_log).expect("hook log file should exist");
    assert!(
        hook_output.contains("preToolUse"),
        "hook with matcher '{}' should have been triggered",
        matcher
    );
}

/// Tests that preToolUse hook matcher works with tool aliases
#[tokio::test]
async fn test_pretooluse_hook_matches_read_alias() {
    let _ = tracing_subscriber::fmt::try_init();
    run_pretooluse_hook_matcher_test("read").await;
}

#[tokio::test]
async fn test_pretooluse_hook_matches_fs_read() {
    let _ = tracing_subscriber::fmt::try_init();
    run_pretooluse_hook_matcher_test("fs_read").await;
}

#[tokio::test]
async fn test_compaction_retry_on_context_overflow_success() {
    let _ = tracing_subscriber::fmt::try_init();

    // Responses: hello_ack -> context overflow -> compaction overflow -> compaction success -> retry
    // success
    let responses = parse_response_streams(include_str!("./mock_responses/compaction_retry_success.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("compaction retry on context overflow success")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // First send hello and wait for response
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Send prompt that triggers context overflow -> compaction retry
    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify compaction events: should have Started and Completed (only one Started due to retry)
    let compaction_events = test.compaction_events();

    let started_count = compaction_events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    assert_eq!(started_count, 1, "expected exactly one CompactionEvent::Started");

    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Completed))),
        "expected CompactionEvent::Completed"
    );

    // Verify agent ended in idle state (successful retry)
    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Idle),
        "expected agent to be idle after successful retry"
    );
}

#[tokio::test]
async fn test_compaction_retry_on_context_overflow_failure() {
    let _ = tracing_subscriber::fmt::try_init();

    // Responses: hello_ack -> context overflow -> compaction overflow -> compaction overflow again
    // (fail)
    let responses = parse_response_streams(include_str!("./mock_responses/compaction_retry_failure.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("compaction retry on context overflow failure")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // First send hello and wait for response
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Send prompt that triggers context overflow -> compaction retry -> failure
    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify compaction events: should have Started and Failed
    let compaction_events = test.compaction_events();

    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started))),
        "expected CompactionEvent::Started"
    );

    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Failed { .. }))),
        "expected CompactionEvent::Failed"
    );

    // Verify agent ended in errored state
    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Errored(_)),
        "expected agent to be in errored state after compaction failure"
    );
}
