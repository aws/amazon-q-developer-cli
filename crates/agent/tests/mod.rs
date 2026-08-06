mod common;

use std::time::Duration;

use agent::agent_config::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
    CommandHook,
    FsReadSettings,
    HookConfig,
    HookTrigger,
    ToolsSettings,
};
use agent::agent_config::types::ResourcePath;
use agent::agent_loop::types::{
    ContentBlock,
    Role,
    ToolResultContentBlock,
    ToolResultStatus,
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
    DEFERRED_TOOLS_MESSAGE,
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

    // Verify user prompt is included in the request
    assert!(
        requests[0].prompt_contains_text("test prompt"),
        "expected user prompt in request"
    );

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

/// Tests that the deferred tools list is injected into context on the first request,
/// and only contains MCP tools allowed by the agent's `tools` config.
#[tokio::test]
async fn test_deferred_tools_filtered_by_agent_tools_config() {
    use std::collections::HashMap;

    use agent::agent_config::definitions::{
        AgentConfigV2025_08_22,
        McpServerConfig,
        RemoteMcpServerConfig,
    };
    use mock_mcp_server::{
        MockMcpServerBuilder,
        ToolDef,
        prebuild_bin,
    };

    let _ = tracing_subscriber::fmt::try_init();

    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    // MCP server exposes 3 tools, but agent config only allows 1
    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "allowed_tool".to_string(),
            description: "This tool is allowed by agent config".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        })
        .add_tool(ToolDef {
            name: "blocked_tool_1".to_string(),
            description: "This tool should NOT appear in deferred list".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        })
        .add_tool(ToolDef {
            name: "blocked_tool_2".to_string(),
            description: "This tool should also NOT appear in deferred list".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        })
        .spawn_http()
        .expect("failed to spawn mock MCP server");

    handle
        .wait_ready(std::time::Duration::from_secs(5))
        .expect("mock MCP server not ready");

    let mcp_config = McpServerConfig::Remote(RemoteMcpServerConfig {
        url: handle.url(),
        headers: HashMap::new(),
        timeout_ms: 30000,
        oauth_scopes: Vec::new(),
        oauth: None,
        disabled: false,
        disabled_tools: Vec::new(),
        force_auth: false,
    });

    // Custom agent config: only tool_search + one specific MCP tool
    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        tools: vec!["tool_search".to_string(), "@testdb/allowed_tool".to_string()],
        mcp_servers: HashMap::from([("testdb".to_string(), mcp_config)]),
        ..Default::default()
    });

    let settings = agent::types::AgentSettings {
        tool_search_enabled: true,
        tool_search_min_pct: None,
        tool_search_min_tokens: None,
        ..Default::default()
    };

    let mut test = TestCase::builder()
        .test_name("deferred tools filtered by agent tools config")
        .with_agent_config(agent_config)
        .with_settings(settings)
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.wait_until_agent_event(Duration::from_secs(10), |evt| matches!(evt, AgentEvent::Initialized))
        .await
        .expect("timed out waiting for agent initialization");

    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(10)).await.unwrap();

    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let context_msg = requests[0]
        .messages()
        .first()
        .expect("first message should exist")
        .text();

    // Deferred tools list structure should be present
    assert!(
        context_msg.contains(DEFERRED_TOOLS_MESSAGE),
        "expected DEFERRED_TOOLS_MESSAGE in context"
    );
    assert!(
        context_msg.contains("<available-deferred-tools>"),
        "expected <available-deferred-tools> XML block"
    );

    // DEFERRED_TOOLS_MESSAGE should appear before tool list
    let msg_pos = context_msg.find(DEFERRED_TOOLS_MESSAGE).unwrap();
    let list_pos = context_msg.find("<available-deferred-tools>").unwrap();
    assert!(
        msg_pos < list_pos,
        "DEFERRED_TOOLS_MESSAGE should appear before tool list"
    );

    // The allowed tool should be in the deferred list
    assert!(
        context_msg.contains("testdb::allowed_tool"),
        "expected allowed_tool in deferred tools list"
    );
    assert!(
        context_msg.contains("This tool is allowed by agent config"),
        "expected tool description in deferred tools list"
    );

    // Blocked tools should NOT be in the deferred list
    assert!(
        !context_msg.contains("testdb::blocked_tool_1"),
        "blocked_tool_1 should NOT appear in deferred tools list"
    );
    assert!(
        !context_msg.contains("testdb::blocked_tool_2"),
        "blocked_tool_2 should NOT appear in deferred tools list"
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
                    trust_option: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_second".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                    trust_option: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_third".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                    trust_option: None,
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
async fn test_build_default_agent_with_steering() {
    let _ = tracing_subscriber::fmt::try_init();

    const GLOBAL_STEERING: &str = "---\ninclusion: always\n---\n# Global Rule\nAlways use snake_case.";
    const WORKSPACE_STEERING: &str = "---\ninclusion: always\n---\n# Workspace Rule\nPrefer async functions.";

    let mut test = TestCase::builder()
        .test_name("steering files included in context")
        .with_default_agent_config()
        .with_file(("~/.kiro/steering/global.md", GLOBAL_STEERING))
        .with_file((".kiro/steering/workspace.md", WORKSPACE_STEERING))
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

    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let first_msg = requests[0]
        .messages()
        .first()
        .expect("first message should exist")
        .text();

    assert!(
        first_msg.contains("Always use snake_case"),
        "expected global steering content in context"
    );
    assert!(
        first_msg.contains("Prefer async functions"),
        "expected workspace steering content in context"
    );
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
                    trust_option: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_second".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                    trust_option: None,
                },
            },
            SendApprovalResultArgs {
                id: "tooluse_third".into(),
                result: ApprovalResult {
                    option_id: PermissionOptionId::AllowOnce,
                    reason: None,
                    trust_option: None,
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

/// Tests that `UserTurnMetadata::message_ids` matches the IDs of the messages
/// persisted in the conversation log 1-to-1.
///
/// For a turn with 2 requests (initial prompt → tool use → tool result → end turn),
/// the conversation log should contain 4 messages [prompt, assistant, tool_results,
/// assistant] and `message_ids` should reference those exact IDs in order.
#[tokio::test]
async fn test_user_turn_metadata_message_ids_match_conversation_log() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("user turn metadata message ids match conversation log")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_file(("test.txt", "hello"))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/fs_read_only.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("read test.txt".to_string()).await;
    let end_turn_evt = test
        .wait_until_agent_event(Duration::from_secs(5), |evt| matches!(evt, AgentEvent::EndTurn(_)))
        .await
        .unwrap();

    let metadata = match end_turn_evt {
        AgentEvent::EndTurn(md) => md,
        _ => unreachable!("predicate matched EndTurn"),
    };

    let mut snapshot = test.create_snapshot().await;
    let messages = snapshot.conversation_state.messages();

    let log_ids: Vec<Option<String>> = messages.iter().map(|m| m.id.clone()).collect();

    assert_eq!(
        metadata.message_ids, log_ids,
        "message_ids in user turn metadata should match the IDs of messages in the conversation log,\n\
         metadata.message_ids = {:?},\n\
         conversation_log_ids = {:?}",
        metadata.message_ids, log_ids
    );
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
    assert_eq!(
        compaction_events
            .iter()
            .filter(|event| matches!(
                event,
                AgentEvent::Compaction(CompactionEvent::ContextRecoveryAttempt { final_attempt: false })
            ))
            .count(),
        1
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
async fn test_stop_hook_block_decision_continues_conversation() {
    let _ = tracing_subscriber::fmt::try_init();

    // Stop hook that returns {"decision": "block", "reason": "Keep going, you haven't run the tests."}
    let mut agent_config = AgentConfig::default();
    agent_config.add_hook(
        HookTrigger::Stop,
        HookConfig::ShellCommand(CommandHook {
            command: r#"echo '{"decision":"block","reason":"Keep going, you haven'"'"'t run the tests."}'"#.to_string(),
            opts: Default::default(),
        }),
    );

    // Two mock responses: first turn (triggers stop hook) + second turn (after hook continues)
    let mut test = TestCase::builder()
        .test_name("stop hook block decision continues conversation")
        .with_agent_config(agent_config)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("test prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5))
        .await
        .expect("agent should eventually stop after second turn");

    // The second request sent to the LLM should contain the stop hook reason
    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (initial + stop hook continue), got {}",
        requests.len()
    );
    assert!(
        requests[1].prompt_contains_text("Keep going, you haven't run the tests."),
        "second request should contain the stop hook reason"
    );
}

#[tokio::test]
async fn test_stop_hook_without_block_decision_stops_normally() {
    let _ = tracing_subscriber::fmt::try_init();

    // Stop hook that returns JSON without decision:block — agent should stop normally
    let mut agent_config = AgentConfig::default();
    agent_config.add_hook(
        HookTrigger::Stop,
        HookConfig::ShellCommand(CommandHook {
            command: r#"echo '{"some_key":"some_value"}'"#.to_string(),
            opts: Default::default(),
        }),
    );

    let mut test = TestCase::builder()
        .test_name("stop hook without block decision stops normally")
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
    test.wait_until_agent_stop(Duration::from_secs(5))
        .await
        .expect("agent should stop normally");

    // Only one request should have been sent
    assert_eq!(
        test.requests().len(),
        1,
        "expected exactly 1 request, agent should not continue"
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
                result: ApprovalResult { option_id: PermissionOptionId::AllowAlwaysToolArgs, reason: None, trust_option: None },
            },
            // Write tool - RejectAlwaysToolArgs denies exact file write (but NOT read)
            SendApprovalResultArgs {
                id: "tooluse_write".into(),
                result: ApprovalResult { option_id: PermissionOptionId::RejectAlwaysToolArgs, reason: None, trust_option: None },
            },
            // tooluse_read2: second read of subdir/file.txt - auto-approved (AllowAlwaysToolArgs)
            // tooluse_read_denied: read of other/output.txt - needs approval (write deny doesn't deny read)
            SendApprovalResultArgs {
                id: "tooluse_read_denied".into(),
                result: ApprovalResult { option_id: PermissionOptionId::AllowOnce, reason: None, trust_option: None },
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

/// Tests that canceling during SendingRequest or ConsumingResponse preserves the user message
/// with a placeholder assistant response
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

    // Verify the user message is preserved with a placeholder assistant response
    let mut snapshot = test.create_snapshot().await;
    let messages = snapshot.conversation_state.messages();

    assert_eq!(
        messages.len(),
        2,
        "expected 2 messages (user + placeholder) after cancel during execution, got {}: {:?}",
        messages.len(),
        messages
    );
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(messages[1].role, Role::Assistant);
    assert!(
        matches!(messages[1].content.as_slice(), [ContentBlock::Text(t)] if t == "Response was interrupted by the user"),
        "expected placeholder assistant message, got: {:?}",
        messages[1].content
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

/// Subagent summary delivery vs. cancellation.
///
/// `Summary::execute()` is the only natural emitter of `SubagentSummary`. If a
/// subagent is cancelled before that tool runs, no `SubagentSummary` is emitted
/// and any in-flight result is dropped. That is the accepted behavior: a
/// cancelled subagent has no result to report, and the orchestrator surfaces
/// "[Cancelled by user]" for the stage rather than the model's partial work.
/// This test pins both halves of that contract:
///   1. cancel during `SendingRequest` (before any assistant content streams) emits no
///      `SubagentSummary`, and
///   2. a turn that streams the summary tool to completion still broadcasts `SubagentSummary` via
///      the natural `execute()` path.
#[tokio::test]
async fn test_cancel_drops_pending_summary_but_natural_execute_broadcasts() {
    let _ = tracing_subscriber::fmt::try_init();

    // Subagent stream: text + summary tool use, no end_turn. We delay the
    // first chunk so the cancel deterministically lands during the
    // SendingRequest stage — the agent has not yet started streaming any
    // content and the assistant message with the summary tool_use has not been
    // appended yet. This exercises the baseline cancellation path without races
    // against the model.
    let response_stream = parse_response_streams(include_str!("./mock_responses/summary_tool.jsonl"))
        .await
        .unwrap();
    let delayed =
        agent::agent_loop::model::MockResponse::with_delay(response_stream[0].clone(), Duration::from_secs(5));

    let mut test = TestCase::builder()
        .test_name("cancel with pending summary tool extracts taskResult")
        .with_default_agent_config()
        .with_is_subagent(true)
        .with_trust_all_tools(true)
        .with_mock_response(delayed)
        .build()
        .await
        .unwrap();

    test.send_prompt("count LOC".to_string()).await;

    // Wait briefly so the request lands and the agent is in SendingRequest /
    // ConsumingResponse — long enough for the model.stream() future to be
    // sleeping on its first-chunk delay.
    tokio::time::sleep(Duration::from_millis(100)).await;
    test.cancel().await.unwrap();
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Cancellation during SendingRequest with no assistant message yet
    // should produce no SubagentSummary — the cancelled subagent reports no
    // result.
    let saw_summary_during_request = test
        .agent_events()
        .iter()
        .any(|evt| matches!(evt, AgentEvent::SubagentSummary(_)));
    assert!(
        !saw_summary_during_request,
        "expected no SubagentSummary when cancel fires before any assistant content streams; events: {:?}",
        test.agent_events()
            .iter()
            .map(|e| format!("{:?}", std::mem::discriminant(e)))
            .collect::<Vec<_>>()
    );

    // Now drive a second turn that DOES stream the summary tool_use to
    // completion — verifies the natural execute() broadcast works when the
    // tool actually runs (the happy path).
    let mut test2 = TestCase::builder()
        .test_name("summary tool natural execute broadcasts summary")
        .with_default_agent_config()
        .with_is_subagent(true)
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/summary_tool.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();
    test2.send_prompt("count LOC".to_string()).await;
    test2
        .wait_until_agent_event(Duration::from_secs(2), |evt| {
            matches!(evt, AgentEvent::SubagentSummary(_))
        })
        .await
        .unwrap();
    let saw_summary = test2.agent_events().iter().any(|evt| {
        matches!(
            evt,
            AgentEvent::SubagentSummary(s)
                if s.task_description == "count LOC" && s.task_result == "42 LOC across 1 file"
        )
    });
    assert!(saw_summary, "expected SubagentSummary from natural execute() path");
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

/// Verifies that userPromptSubmit hook output is included in the LLM request
/// as additional_context metadata on the user message.
#[tokio::test]
async fn test_user_prompt_submit_hook_output_in_request() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("prompt hook output in request")
        .with_default_agent_config()
        .with_hook(
            HookTrigger::UserPromptSubmit,
            HookConfig::ShellCommand(CommandHook {
                command: "echo PROMPT_HOOK_MARKER_42".to_string(),
                opts: Default::default(),
            }),
        )
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("hello from user".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(10))
        .await
        .expect("agent should stop");

    // The last user message in the sent request should carry the hook output
    // in its metadata.additional_context field.
    let requests = test.requests();
    assert!(!requests.is_empty(), "should have at least one request");

    let last_user_msg = requests[0]
        .messages()
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .expect("should have a user message");

    let meta = last_user_msg.meta.as_ref().expect("user message should have metadata");

    assert!(
        meta.additional_context.contains("PROMPT_HOOK_MARKER_42"),
        "hook output should be in additional_context, got: {:?}",
        meta.additional_context
    );

    // Also verify the original prompt text is in the content
    assert!(
        last_user_msg
            .content
            .iter()
            .any(|c| matches!(c, ContentBlock::Text(t) if t.contains("hello from user"))),
        "original prompt should be in content blocks"
    );
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

#[tokio::test]
async fn test_proactive_compaction_on_high_context_usage() {
    let _ = tracing_subscriber::fmt::try_init();

    // Responses: first prompt response (100% context usage) -> compaction summary -> retry (second
    // prompt). The second prompt consumes no response of its own: the agent loop synthesizes a
    // ContextWindowOverflow instead of dispatching it, which drives the existing recovery path.
    let responses = parse_response_streams(include_str!("./mock_responses/proactive_compaction.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("proactive compaction on high context usage")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // Pin the policy value itself. Behavioral tests sit exactly on it, so this plus the
    // fixture pins both the constant and the inclusive comparison.
    assert_eq!(
        agent::consts::SYNTHETIC_OVERFLOW_THRESHOLD,
        100.0,
        "272K pricing boundary policy; changing this changes when we start paying 2x"
    );

    // First prompt - response reports 103%, i.e. the backend accepted a request past the
    // limit it vended, which is the only way a reading can exceed 100.
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Second prompt should trigger proactive compaction before sending
    test.send_prompt("follow up question".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify compaction events were emitted
    let compaction_events = test.compaction_events();

    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started))),
        "expected CompactionEvent::Started from proactive compaction"
    );
    assert!(
        compaction_events
            .iter()
            .any(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Completed))),
        "expected CompactionEvent::Completed"
    );

    // Verify the retry happened (recovery attempt event)
    assert!(
        compaction_events.iter().any(|e| matches!(
            e,
            AgentEvent::Compaction(CompactionEvent::ContextRecoveryAttempt { final_attempt: false })
        )),
        "expected ContextRecoveryAttempt after proactive compaction"
    );

    // Verify agent ended in idle state (successful)
    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Idle),
        "expected agent to be idle after proactive compaction and retry"
    );
}

#[tokio::test]
async fn test_stale_context_usage_is_not_reused_after_compaction() {
    let _ = tracing_subscriber::fmt::try_init();

    // Regression: a reader that searches turn history for the most recent reported
    // percentage skips the retry turn (which reports none) and finds the pre-compaction
    // 100.0, compacting again on every later prompt. Correct behavior is one compaction.
    let responses = parse_response_streams(include_str!("./mock_responses/stale_context_usage_not_reused.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("stale context usage is not reused after compaction")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // Prompt 1 - response reports 100%, the clamped over-limit signal
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Prompt 2 - synthesizes an overflow, compacts, retries. The retry reports no
    // percentage, so afterwards the current reading must be unset.
    test.send_prompt("second prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let started_after_two = test
        .compaction_events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    assert_eq!(started_after_two, 1, "prompt 2 should compact exactly once");

    // The compaction count alone cannot distinguish success from the recovery ladder
    // running to exhaustion, so pin the outcome too: no last-ditch truncation attempt,
    // and the turn actually completed.
    let final_attempts = test
        .compaction_events()
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentEvent::Compaction(CompactionEvent::ContextRecoveryAttempt { final_attempt: true })
            )
        })
        .count();
    assert_eq!(
        final_attempts, 0,
        "prompt 2 must not reach the final truncation attempt"
    );
    let mid = test.create_snapshot().await;
    assert!(
        matches!(mid.execution_state.active_state, ActiveState::Idle),
        "prompt 2 must complete rather than enter the error state"
    );

    // Prompt 3 - must dispatch normally. If the stale 100.0 is resurrected this
    // compacts again, which is the bug.
    test.send_prompt("third prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let started_after_three = test
        .compaction_events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    assert_eq!(
        started_after_three, 1,
        "prompt 3 must not compact again; the pre-compaction reading was resurrected"
    );

    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Idle),
        "expected idle after three prompts"
    );
}

#[tokio::test]
async fn test_reading_exactly_at_limit_synthesizes_overflow() {
    let _ = tracing_subscriber::fmt::try_init();

    // The backend clamps the reported percentage at 100, verified live against a GPT
    // model: a conversation taken well past the vended limit still reports exactly 100.0.
    // So the comparison must be inclusive; an exclusive one makes the feature dead code.
    let responses = parse_response_streams(include_str!(
        "./mock_responses/context_usage_at_limit_synthesizes.jsonl"
    ))
    .await
    .unwrap();

    let mut test = TestCase::builder()
        .test_name("reading exactly at limit synthesizes overflow")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let snapshot = test.create_snapshot().await;
    assert_eq!(
        snapshot
            .conversation_metadata
            .last_context_usage
            .as_ref()
            .map(|u| u.percentage),
        Some(100.0),
        "the 100% reading should be recorded"
    );

    // The second prompt must synthesize rather than dispatch, since 100 is the only
    // over-limit signal the backend ever emits.
    test.send_prompt("second prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let started = test
        .compaction_events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    assert_eq!(
        started, 1,
        "a reading of exactly 100 must trigger compaction; the backend never reports more"
    );

    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Idle),
        "expected idle after two prompts"
    );
}

#[tokio::test]
async fn test_mid_turn_compaction_does_not_restore_prior_reading() {
    let _ = tracing_subscriber::fmt::try_init();

    // Regression: when compaction happens part way through a turn, the turn's recorded
    // percentage must come from the final response, not from the latest value reported
    // anywhere in the turn. Otherwise the pre-compaction 100.0 is written back on top of
    // the clear and the next prompt compacts freshly compacted history.
    let responses = parse_response_streams(include_str!("./mock_responses/context_usage_mid_turn_compaction.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("mid turn compaction does not restore prior reading")
        .with_default_agent_config()
        .with_file(("test.txt", "hello world"))
        .with_trust_all_tools(true)
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // Prompt 1: reports 100% on a tool-use response, the continuation overflows for real,
    // compaction runs mid-turn, and the retry ends the turn reporting nothing.
    test.send_prompt("read the file".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let started_after_one = test
        .compaction_events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    assert_eq!(started_after_one, 1, "the real overflow should compact exactly once");

    // The reading must not survive the compaction that ran during this turn.
    let snapshot = test.create_snapshot().await;
    assert!(
        snapshot.conversation_metadata.last_context_usage.is_none(),
        "a pre-compaction reading was written back after the clear: {:?}",
        snapshot.conversation_metadata.last_context_usage
    );

    // Prompt 2 must dispatch normally rather than synthesizing another overflow.
    test.send_prompt("second prompt".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let started_after_two = test
        .compaction_events()
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    assert_eq!(
        started_after_two, 1,
        "prompt 2 must not compact again after a mid-turn compaction"
    );

    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Idle),
        "expected idle after two prompts"
    );
}

#[tokio::test]
async fn test_overflow_after_compaction_retry_truncates_user_message() {
    let _ = tracing_subscriber::fmt::try_init();

    // Flow: hello -> hello_ack -> large_prompt -> overflow -> compaction -> retry overflow -> truncate
    // -> retry success
    let responses = parse_response_streams(include_str!("./mock_responses/overflow_after_compaction_retry.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("overflow after compaction retry truncates user message")
        .with_default_agent_config()
        .with_responses(responses)
        .build()
        .await
        .unwrap();

    // First send hello to build up some history
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Send a large prompt that will overflow even after compaction
    let large_prompt = "x".repeat(30_000);
    test.send_prompt(large_prompt).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify compaction events: should have 1 Started and 1 Completed
    // (truncation retries directly without another compaction)
    let compaction_events = test.compaction_events();
    let started_count = compaction_events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Started)))
        .count();
    let completed_count = compaction_events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Compaction(CompactionEvent::Completed)))
        .count();

    assert_eq!(started_count, 1, "expected 1 CompactionEvent::Started");
    assert_eq!(completed_count, 1, "expected 1 CompactionEvent::Completed");
    assert_eq!(
        compaction_events
            .iter()
            .filter(|event| matches!(
                event,
                AgentEvent::Compaction(CompactionEvent::ContextRecoveryAttempt { .. })
            ))
            .count(),
        2
    );
    assert!(compaction_events.iter().any(|event| matches!(
        event,
        AgentEvent::Compaction(CompactionEvent::ContextRecoveryAttempt { final_attempt: true })
    )));

    // Verify agent ended in idle state (success)
    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(snapshot.execution_state.active_state, ActiveState::Idle),
        "expected agent to be idle after successful retry"
    );

    // Verify the retry request had a truncated user message
    // The last request (after truncation) should have the truncated message
    let requests = test.requests();
    let last_request = requests.last().unwrap();
    let last_user_msg = last_request
        .messages()
        .iter()
        .filter(|m| m.role == agent::agent_loop::types::Role::User)
        .last()
        .expect("expected user message in request");

    let user_content = last_user_msg.text();
    assert!(
        user_content.ends_with("...truncated due to length"),
        "expected user message to be truncated, got: {}...",
        &user_content[..100.min(user_content.len())]
    );
}

/// Tests that file:// URIs in global_prompt are resolved correctly.
#[tokio::test]
async fn test_file_uri_global_prompt() {
    let _ = tracing_subscriber::fmt::try_init();

    const PROMPT_FILE_CONTENT: &str = "You are a helpful coding assistant. Always explain your reasoning.";

    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        global_prompt: Some("file://prompts/system.md".to_string()),
        ..Default::default()
    });

    let mut test = TestCase::builder()
        .test_name("file uri system prompt")
        .with_agent_config(agent_config)
        .with_file(("prompts/system.md", PROMPT_FILE_CONTENT))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let first_msg = requests[0]
        .messages()
        .first()
        .expect("first message should exist")
        .text();

    // The resolved file content should be in the context message
    assert!(
        first_msg.contains(PROMPT_FILE_CONTENT),
        "expected resolved file content in context, got: {}",
        first_msg
    );

    // The file:// URI should NOT appear literally
    assert!(
        !first_msg.contains("file://prompts/system.md"),
        "file:// URI should be resolved, not appear literally"
    );
}

/// Tests that the InvalidJson error path correctly recovers when the model
/// produces truncated JSON in a tool use. This reproduces a bug where the
/// conversation history invariant was violated because the fake assistant
/// message was appended before the pending user message.
///
/// The sequence is:
/// 1. Send prompt
/// 2. Model responds with a tool use containing truncated JSON
/// 3. Agent detects InvalidJson, appends messages, and retries
/// 4. Retry succeeds with endTurn
///
/// Before the fix, step 3 would fail with "invalid conversation history received"
/// because the assistant message was appended before the user message, breaking
/// the User→Assistant alternation invariant.
#[tokio::test]
async fn test_invalid_json_recovery() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("invalid json recovery")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/invalid_json_recovery.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("write a summary to summary.md".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    // Verify two requests were sent: the original and the retry
    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (original + retry), got {}",
        requests.len()
    );

    // The retry prompt should contain the "too large" message
    assert!(
        requests[1].prompt_contains_text("split up the work"),
        "retry prompt should ask model to split up the work"
    );

    // Verify conversation history in the retry request maintains proper alternation
    let retry_messages = requests[1].messages();
    for pair in retry_messages.windows(2) {
        let curr = &pair[0];
        let next = &pair[1];
        match curr.role {
            Role::User => assert_eq!(
                next.role,
                Role::Assistant,
                "User message at should be followed by Assistant, messages: {:?}",
                retry_messages.iter().map(|m| &m.role).collect::<Vec<_>>()
            ),
            Role::Assistant => assert_eq!(
                next.role,
                Role::User,
                "Assistant message should be followed by User, messages: {:?}",
                retry_messages.iter().map(|m| &m.role).collect::<Vec<_>>()
            ),
        }
    }
}

/// Tests that when the model returns both valid and invalid tool uses, the valid
/// tool uses are preserved in the conversation history and the retry includes
/// tool results for all tool uses (maintaining the ToolUse↔ToolResult invariant).
#[tokio::test]
async fn test_invalid_json_preserves_valid_tool_uses() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("invalid json preserves valid tools")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/invalid_json_with_valid_tools.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("read input.txt and write a summary to summary.md".to_string())
        .await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (original + retry), got {}",
        requests.len()
    );

    // The retry's assistant message (second-to-last in history) should contain both tool uses
    let retry_messages = requests[1].messages();
    let assistant_msg = retry_messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant)
        .expect("retry should have an assistant message in history");
    let tool_uses = assistant_msg
        .tool_uses()
        .expect("assistant message should have tool uses");
    assert_eq!(
        tool_uses.len(),
        2,
        "assistant message should contain both valid and invalid tool uses"
    );
    assert_eq!(tool_uses[0].tool_use_id, "tu_valid_1");
    assert_eq!(tool_uses[1].tool_use_id, "tu_invalid_1");

    // The retry prompt (last user message) should have tool results for both tool uses
    assert!(
        requests[1].has_tool_result(|tr| tr.tool_use_id == "tu_valid_1"),
        "retry should include a tool result for the valid tool use"
    );
    assert!(
        requests[1].has_tool_result(|tr| tr.tool_use_id == "tu_invalid_1"),
        "retry should include a tool result for the invalid tool use"
    );

    // Also verify the retry text is present
    assert!(
        requests[1].prompt_contains_text("split up the work"),
        "retry prompt should ask model to split up the work"
    );
}

/// Tests that when the model dispatches a parallel batch of tool uses and one
/// of them fails parse-time validation (e.g. fs_read against a nonexistent
/// directory), the parsed-OK siblings still execute and their results are
/// merged with the parse-error result into a single tool_results message
/// back to the model. The previous behavior short-circuited at the parse-
/// error branch, dropping every parsed-OK sibling on the floor and relying
/// on enforce_conversation_invariants to fabricate "Tool use was cancelled
/// by the user" results — which falsely told the model the user had
/// interrupted siblings the user never touched.
#[tokio::test]
async fn test_parse_error_preserves_parsed_ok_siblings() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("parse error preserves parsed-ok siblings")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_file(("a.txt", "alpha"))
        .with_file(("b.txt", "beta"))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/parse_error_partial_batch.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("read a.txt and b.txt and list the missing dir".to_string())
        .await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (initial + tool_results), got {}",
        requests.len()
    );

    // The follow-up request after the tool batch must contain a tool_result
    // for every tool_use the model emitted in the assistant turn — including
    // the parsed-OK siblings, not just the parse-error one.
    let follow_up = &requests[1];
    assert!(
        follow_up
            .has_tool_result(|tr| tr.tool_use_id == "tooluse_ok_1" && matches!(tr.status, ToolResultStatus::Success)),
        "follow-up should include a successful tool_result for tooluse_ok_1"
    );
    assert!(
        follow_up
            .has_tool_result(|tr| tr.tool_use_id == "tooluse_ok_2" && matches!(tr.status, ToolResultStatus::Success)),
        "follow-up should include a successful tool_result for tooluse_ok_2"
    );
    assert!(
        follow_up.has_tool_result(|tr| tr.tool_use_id == "tooluse_bad" && matches!(tr.status, ToolResultStatus::Error)),
        "follow-up should include an error tool_result for tooluse_bad"
    );

    // The parse-error result must carry a real "Directory not found" / parse
    // error message — NOT the synthetic "Tool use was cancelled by the user"
    // text that enforce_conversation_invariants used to paper over the gap.
    let bad_result_text = follow_up
        .messages()
        .last()
        .expect("last message should exist")
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::ToolResult(tr) if tr.tool_use_id == "tooluse_bad" => Some(
                tr.content
                    .iter()
                    .filter_map(|b| match b {
                        ToolResultContentBlock::Text(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .expect("tooluse_bad must have a tool_result");
    assert!(
        !bad_result_text.contains("Tool use was cancelled by the user"),
        "parse error must not be misreported as user cancellation; got: {bad_result_text}"
    );
    assert!(
        bad_result_text.to_ascii_lowercase().contains("directory")
            || bad_result_text.contains("Failed to parse the tool use"),
        "parse error should describe the validation failure; got: {bad_result_text}"
    );
}

/// When a model batch mixes a parse-error tool with a sibling the user then
/// DENIES, the parse-error result (held aside in `pre_built_*`) must still be
/// threaded into the outbound batch. Regression test for the user-deny branch
/// of `handle_approval_result`, which previously rebuilt the batch solely from
/// `needs_approval` and dropped the parse-error sibling — leaving its tool_use
/// with no tool_result, so enforce_conversation_invariants back-filled a false
/// "Tool use was cancelled by the user" message for a tool the user never saw.
#[tokio::test]
async fn test_parse_error_preserved_when_sibling_denied() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("parse error preserved when sibling denied")
        .with_default_agent_config()
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/parse_error_with_denied_sibling.jsonl"))
                .await
                .unwrap(),
        )
        // Not trust-all: the fs_write pauses for approval, which we reject.
        .with_tool_use_approvals([SendApprovalResultArgs {
            id: "tooluse_write".into(),
            result: ApprovalResult {
                option_id: PermissionOptionId::RejectOnce,
                reason: None,
                trust_option: None,
            },
        }])
        .build()
        .await
        .unwrap();

    test.send_prompt("write hello.py and list the missing dir".to_string())
        .await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (initial + tool_results after deny), got {}",
        requests.len()
    );

    // The follow-up request after the deny must contain a tool_result for BOTH
    // tool_uses the model emitted: the denied write AND the parse-error read.
    let follow_up = &requests[1];
    assert!(
        follow_up
            .has_tool_result(|tr| tr.tool_use_id == "tooluse_write" && matches!(tr.status, ToolResultStatus::Error)),
        "follow-up should include an error tool_result for the denied tooluse_write"
    );
    assert!(
        follow_up.has_tool_result(|tr| tr.tool_use_id == "tooluse_bad" && matches!(tr.status, ToolResultStatus::Error)),
        "follow-up should include the parse-error tool_result for tooluse_bad — \
         dropping it is the bug this test guards against"
    );

    // The parse-error result must carry the real validation message, NOT the
    // synthetic "Tool use was cancelled by the user" text that
    // enforce_conversation_invariants would back-fill if the result were dropped.
    let bad_result_text = follow_up
        .messages()
        .last()
        .expect("last message should exist")
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::ToolResult(tr) if tr.tool_use_id == "tooluse_bad" => Some(
                tr.content
                    .iter()
                    .filter_map(|b| match b {
                        ToolResultContentBlock::Text(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .expect("tooluse_bad must have a tool_result");
    assert!(
        !bad_result_text.contains("Tool use was cancelled by the user"),
        "parse error must not be misreported as user cancellation; got: {bad_result_text}"
    );
    assert!(
        bad_result_text.to_ascii_lowercase().contains("directory")
            || bad_result_text.contains("Failed to parse the tool use"),
        "parse error should describe the validation failure; got: {bad_result_text}"
    );
}

/// Answering one of several queued approvals must not continue the turn while
/// others are unanswered. Regression for the premature-continue bug in
/// `handle_approval_result`; fails without the guard (agent goes Idle).
#[tokio::test]
async fn test_multi_approval_waits_for_all_answers() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("multi approval waits for all answers")
        .with_default_agent_config()
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/two_writes_await_approval.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    let reject = || ApprovalResult {
        option_id: PermissionOptionId::RejectOnce,
        reason: None,
        trust_option: None,
    };

    test.send_prompt("write a.txt and b.txt".to_string()).await;

    // Both writes queue for approval. Reject the first; the turn must not
    // continue (no follow-up request, still WaitingForApproval) while the
    // second is unanswered.
    let first_id = test.wait_for_approval_request(Duration::from_secs(5)).await.unwrap();
    assert_eq!(test.requests().len(), 1);
    test.send_approval(first_id.clone(), reject()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        test.requests().len(),
        1,
        "rejecting one queued approval must not send a follow-up"
    );
    let snapshot = test.create_snapshot().await;
    assert!(
        matches!(
            snapshot.execution_state.active_state,
            ActiveState::WaitingForApproval(_)
        ),
        "must stay in WaitingForApproval until all answered; got {:?}",
        snapshot.execution_state.active_state
    );

    // Answer the second; now the deny follow-up fires with error results for both.
    let second_id = test.wait_for_approval_request(Duration::from_secs(5)).await.unwrap();
    assert_ne!(second_id, first_id);
    test.send_approval(second_id, reject()).await.unwrap();
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let follow_up = &test.requests()[1];
    for id in ["tooluse_write_a", "tooluse_write_b"] {
        assert!(
            follow_up.has_tool_result(|tr| tr.tool_use_id == id && matches!(tr.status, ToolResultStatus::Error)),
            "follow-up must include an error tool_result for {id}"
        );
    }
}

/// A note steered alongside a REJECTED tool must drain into the same follow-up
/// request that carries the denial — not wait until end-of-turn. Regression
/// test for the lite "attach note then reject" flow: the user saw the note
/// land immediately on approve/trust but "queued for afterwards" on reject,
/// because the deny branch sent its tool_results via `send_request` directly,
/// bypassing the `send_tool_results` steering drain. The fix drains queued
/// steering in the deny branch too, so all dispositions behave identically.
#[tokio::test]
async fn test_steered_note_drains_on_reject() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("steered note drains on reject")
        .with_default_agent_config()
        // The fs_write pauses for approval; we steer a note, then reject it.
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/parse_error_with_denied_sibling.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("write hello.py and list the missing dir".to_string())
        .await;

    // When the approval lands, steer a note BEFORE rejecting — mirroring the
    // lite respondWithNote flow (note is buffered, then the disposition is
    // sent). The note must ride the very next request, alongside the denial.
    test.steer_then_approve(
        Duration::from_secs(5),
        "please use a different filename",
        ApprovalResult {
            option_id: PermissionOptionId::RejectOnce,
            reason: None,
            trust_option: None,
        },
    )
    .await
    .unwrap();

    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (initial + tool_results after deny), got {}",
        requests.len()
    );

    // The follow-up request (the deny send) must carry BOTH the denial tool
    // result AND the steered note as user text — proving the note drained at
    // the reject boundary rather than waiting for end-of-turn.
    let follow_up = &requests[1];
    assert!(
        follow_up
            .has_tool_result(|tr| tr.tool_use_id == "tooluse_write" && matches!(tr.status, ToolResultStatus::Error)),
        "follow-up should include the error tool_result for the denied write"
    );
    assert!(
        follow_up.prompt_contains_text("please use a different filename"),
        "the steered note must drain into the SAME follow-up request as the denial \
         (it rides as a [LIVE STEERING ...] user text block); if this fails the note \
         was deferred to end-of-turn — the bug this guards against"
    );
}

/// Each tool's approval is self-contained: rejecting one tool in a parallel
/// batch must NOT cancel its approved siblings. Approve write_a, reject write_b
/// — write_a must execute (success result) while write_b is denied (error
/// result). Both results ride the same follow-up request so the model still
/// sees a tool_result for every tool_use it emitted.
#[tokio::test]
async fn test_reject_one_still_executes_approved_sibling() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("reject one still executes approved sibling")
        .with_default_agent_config()
        .with_cwd_subdir("work")
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/two_writes_await_approval.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("write a.txt and b.txt".to_string()).await;

    // Both writes queue. Approve the first, reject the second.
    let first_id = test.wait_for_approval_request(Duration::from_secs(5)).await.unwrap();
    test.send_approval(first_id.clone(), ApprovalResult {
        option_id: PermissionOptionId::AllowOnce,
        reason: None,
        trust_option: None,
    })
    .await
    .unwrap();

    let second_id = test.wait_for_approval_request(Duration::from_secs(5)).await.unwrap();
    assert_ne!(second_id, first_id);
    test.send_approval(second_id.clone(), ApprovalResult {
        option_id: PermissionOptionId::RejectOnce,
        reason: None,
        trust_option: None,
    })
    .await
    .unwrap();

    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    // The follow-up request carries a success result for the approved write and
    // an error result for the rejected one — the approved sibling executed
    // despite its batch-mate being denied.
    let follow_up = &test.requests()[1];
    assert!(
        follow_up.has_tool_result(|tr| tr.tool_use_id == first_id && matches!(tr.status, ToolResultStatus::Success)),
        "approved sibling must have a success tool_result"
    );
    assert!(
        follow_up.has_tool_result(|tr| tr.tool_use_id == second_id && matches!(tr.status, ToolResultStatus::Error)),
        "rejected tool must have an error tool_result"
    );
}

/// A batch mixing an auto-allowed tool (a trusted fs_read) with an Ask tool
/// (fs_write). Auto-allowed tools live in `state.tools` but never in
/// `needs_approval`. Approving the write must still execute the auto-allowed
/// read — the partition must not drop siblings that were never queued for
/// approval (regression: they got a false "cancelled by the user" back-fill).
#[tokio::test]
async fn test_auto_allowed_sibling_executes_alongside_approved_ask() {
    let _ = tracing_subscriber::fmt::try_init();

    // fs_read auto-allows (allow_read_only); fs_write still asks.
    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        tools: vec!["*".to_string()],
        tools_settings: Some(ToolsSettings {
            fs_read: FsReadSettings {
                allow_read_only: true,
                ..Default::default()
            },
            ..Default::default()
        }),
        ..Default::default()
    });

    let mut test = TestCase::builder()
        .test_name("auto allowed sibling executes alongside approved ask")
        .with_agent_config(agent_config)
        .with_cwd_subdir("work")
        .with_file(("test.txt", "hello"))
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/auto_allowed_sibling_with_ask.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("read test.txt and create out.txt".to_string()).await;

    // Only the write queues for approval; approve it.
    let write_id = test.wait_for_approval_request(Duration::from_secs(5)).await.unwrap();
    assert_eq!(write_id, "tooluse_write", "only the fs_write should require approval");
    test.send_approval(write_id, ApprovalResult {
        option_id: PermissionOptionId::AllowOnce,
        reason: None,
        trust_option: None,
    })
    .await
    .unwrap();

    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    // Both tools ran: the auto-allowed read and the approved write each carry a
    // success tool_result. Before the fix the read was dropped and back-filled
    // as an error "cancelled by the user".
    let follow_up = &test.requests()[1];
    assert!(
        follow_up
            .has_tool_result(|tr| tr.tool_use_id == "tooluse_read" && matches!(tr.status, ToolResultStatus::Success)),
        "auto-allowed sibling read must execute (success tool_result), not be back-filled as cancelled"
    );
    assert!(
        follow_up
            .has_tool_result(|tr| tr.tool_use_id == "tooluse_write" && matches!(tr.status, ToolResultStatus::Success)),
        "approved write must execute (success tool_result)"
    );
}

/// Tests that an empty response (messageStart + messageStop + metadata, no
/// text/tools/thinking) triggers exactly one retry. When the retry succeeds, the agent
/// completes normally.
#[tokio::test]
async fn test_empty_response_retry_success() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("empty response retry success")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/empty_response_retry_success.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let requests = test.requests();
    assert_eq!(
        requests.len(),
        2,
        "expected exactly 2 requests (original + retry), got {}",
        requests.len()
    );

    // The retry must resend the same conversation: no synthetic placeholder, no nudge text,
    // no extra messages. Compare structurally (Message lacks PartialEq).
    let original = requests[0].messages();
    let retry = requests[1].messages();
    assert_eq!(
        original.len(),
        retry.len(),
        "retry should have the same number of messages as the original"
    );
    for (i, (a, b)) in original.iter().zip(retry.iter()).enumerate() {
        assert_eq!(a.role, b.role, "message {} role mismatch", i);
        assert_eq!(a.text(), b.text(), "message {} text mismatch", i);
    }

    // Final stop reason should be EndTurn (not Error)
    let stop = test
        .agent_events()
        .iter()
        .rev()
        .find_map(|e| match e {
            AgentEvent::Stop(reason) => Some(reason.clone()),
            _ => None,
        })
        .expect("agent should emit a Stop event");
    assert!(
        matches!(stop, agent::protocol::AgentStopReason::EndTurn),
        "expected EndTurn, got {:?}",
        stop
    );
}

/// Tests that two consecutive empty responses produce a hard failure with
/// AgentStopReason::Error wrapping LoopError::EmptyResponse. No third request.
#[tokio::test]
async fn test_empty_response_retry_failure() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("empty response retry failure")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/empty_response_retry_failure.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let requests = test.requests();
    assert_eq!(
        requests.len(),
        2,
        "expected exactly 2 requests (original + 1 retry, then hard fail), got {}",
        requests.len()
    );

    // Final stop reason should be Error(AgentLoopError(EmptyResponse))
    let stop = test
        .agent_events()
        .iter()
        .rev()
        .find_map(|e| match e {
            AgentEvent::Stop(reason) => Some(reason.clone()),
            _ => None,
        })
        .expect("agent should emit a Stop event");
    match stop {
        agent::protocol::AgentStopReason::Error(agent::protocol::AgentError::AgentLoopError(
            agent::agent_loop::protocol::LoopError::EmptyResponse,
        )) => (),
        other => panic!("expected Stop(Error(AgentLoopError(EmptyResponse))), got {:?}", other),
    }
}

/// Tests that switch_to_execution ends the turn without sending tool results
/// back to the LLM, so the caller can swap agents and inject the plan.
#[tokio::test]
async fn test_switch_to_execution_ends_turn_without_tool_results() {
    let _ = tracing_subscriber::fmt::try_init();

    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        name: "kiro_planner".to_string(),
        global_prompt: Some("You are a planning agent".to_string()),
        tools: vec!["switch_to_execution".to_string()],
        ..Default::default()
    });

    let mut test = TestCase::builder()
        .test_name("switch_to_execution ends turn")
        .with_agent_config(agent_config)
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/switch_to_execution.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("build me a todo app".to_string()).await;
    test.wait_until_agent_event(Duration::from_secs(5), |evt| matches!(evt, AgentEvent::EndTurn(_)))
        .await
        .unwrap();

    // Only 1 request should have been sent to the LLM (the initial prompt).
    // If the tool result was sent back, there would be 2 requests.
    let requests = test.requests();
    assert_eq!(
        requests.len(),
        1,
        "should send only 1 request — switch_to_execution must not send tool results back to the LLM"
    );

    // Verify the EndTurn event was emitted
    let has_end_turn = test.log_entry_appended_events().iter().any(
        |e| matches!(e, AgentEvent::LogEntryAppended { entry, .. } if format!("{:?}", entry).contains("cancelled")),
    );
    assert!(
        has_end_turn,
        "should have ended the turn (cancelled tool results in log)"
    );
}

#[tokio::test]
async fn test_duplicate_agent_spawn_hooks_all_complete() {
    let _ = tracing_subscriber::fmt::try_init();

    let hook = HookConfig::ShellCommand(CommandHook {
        command: "echo duplicate".to_string(),
        opts: Default::default(),
    });

    let mut test = TestCase::builder()
        .test_name("duplicate agent spawn hooks all complete")
        .with_hook(HookTrigger::AgentSpawn, hook.clone())
        .with_hook(HookTrigger::AgentSpawn, hook)
        .build()
        .await
        .unwrap();

    test.wait_until_agent_event(Duration::from_secs(5), |evt| matches!(evt, AgentEvent::Initialized))
        .await
        .expect("agent should initialize after duplicate spawn hooks complete");

    // Verify both hooks were executed (HookExecutionEnd events now arrive before Initialized).
    let hook_end_count = test
        .agent_events()
        .iter()
        .filter(|evt| {
            matches!(evt, AgentEvent::Internal(
                agent::protocol::InternalEvent::TaskExecutor(te)
            ) if matches!(te.as_ref(), agent::task_executor::TaskExecutorEvent::HookExecutionEnd(_)))
        })
        .count();
    assert_eq!(hook_end_count, 2, "both duplicate hooks should have executed");
}

/// Verifies that agentSpawn hook stdout is injected into the first context
/// message of every LLM request for the entire conversation.
#[tokio::test]
async fn test_agent_spawn_hook_output_in_context() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("agent spawn hook output in context")
        .with_default_agent_config()
        .with_hook(
            HookTrigger::AgentSpawn,
            HookConfig::ShellCommand(CommandHook {
                command: "echo SPAWN_HOOK_CONTEXT_MARKER_789".to_string(),
                opts: Default::default(),
            }),
        )
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    // Wait for hooks to complete during initialization
    test.wait_until_agent_event(Duration::from_secs(5), |evt| matches!(evt, AgentEvent::Initialized))
        .await
        .expect("agent should initialize after spawn hook completes");

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5))
        .await
        .expect("agent should stop");

    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let context_msg = requests[0]
        .messages()
        .first()
        .expect("first message should exist")
        .text();

    assert!(
        context_msg.contains("SPAWN_HOOK_CONTEXT_MARKER_789"),
        "agentSpawn hook stdout should be injected into the first context message, got: {}",
        context_msg
    );
}

/// Tests the full MCP tool filtering and activation flow via tool_search:
/// 1. Agent starts with MCP tools filtered out of tool_specs (low context/token usage)
/// 2. Model calls tool_search — BM25 finds matching MCP tool — tool gets activated
/// 3. Activated MCP tool now appears in tool_specs sent to model
#[tokio::test]
async fn test_tool_search_enabled_includes_tool_search() {
    use std::collections::HashMap;

    use agent::agent_config::definitions::{
        McpServerConfig,
        RemoteMcpServerConfig,
    };
    use mock_mcp_server::{
        MockMcpServerBuilder,
        ToolDef,
        prebuild_bin,
    };

    let _ = tracing_subscriber::fmt::try_init();

    // Prebuild the mock MCP server binary
    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    // Spawn mock MCP server with a database tool
    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "database_query".to_string(),
            description: "Execute SQL database queries and return results".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
        })
        .spawn_http()
        .expect("failed to spawn mock MCP server");

    handle
        .wait_ready(std::time::Duration::from_secs(5))
        .expect("mock MCP server not ready");

    let mcp_config = McpServerConfig::Remote(RemoteMcpServerConfig {
        url: handle.url(),
        headers: HashMap::new(),
        timeout_ms: 30000,
        oauth_scopes: Vec::new(),
        oauth: None,
        disabled: false,
        disabled_tools: Vec::new(),
        force_auth: false,
    });

    let settings = agent::types::AgentSettings {
        tool_search_enabled: true,
        tool_search_min_pct: None,
        tool_search_min_tokens: None,
        ..Default::default()
    };

    let mut test = TestCase::builder()
        .test_name("tool search mcp filtering and activation")
        .with_default_agent_config()
        .with_settings(settings)
        .with_mcp_server("testdb", mcp_config)
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/tool_search_flow.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    // Wait for agent initialization (MCP servers to be ready)
    test.wait_until_agent_event(Duration::from_secs(10), |evt| matches!(evt, AgentEvent::Initialized))
        .await
        .expect("timed out waiting for agent initialization");

    test.send_prompt("search for database tools".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(10)).await.unwrap();

    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests, got {}",
        requests.len()
    );

    // Request 0: MCP tool should be ABSENT (filtered out), tool_search should be present
    let tool_specs_0 = requests[0].tool_specs().expect("request 0 should have tool specs");
    let has_tool_search = tool_specs_0.iter().any(|t| t.name == "tool_search");
    let has_mcp_tool_0 = tool_specs_0.iter().any(|t| t.name.contains("database_query"));

    assert!(has_tool_search, "tool_search should be present in request 0");
    assert!(
        !has_mcp_tool_0,
        "MCP tool database_query should NOT be present in request 0 (filtered out)"
    );

    // Request 1: MCP tool should be PRESENT (activated after tool_search)
    let tool_specs_1 = requests[1].tool_specs().expect("request 1 should have tool specs");
    let has_mcp_tool_1 = tool_specs_1.iter().any(|t| t.name.contains("database_query"));

    assert!(
        has_mcp_tool_1,
        "MCP tool database_query should be present in request 1 (activated after tool_search)"
    );
}

/// Tests that tool_search_enabled=false excludes tool_search from tool specs.
#[tokio::test]
async fn test_tool_search_disabled_excludes_tool_search() {
    let _ = tracing_subscriber::fmt::try_init();

    let settings = agent::types::AgentSettings {
        tool_search_enabled: false,
        ..Default::default()
    };

    let mut test = TestCase::builder()
        .test_name("tool search disabled excludes search tool")
        .with_default_agent_config()
        .with_settings(settings)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(2)).await.unwrap();

    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let tool_specs = requests[0].tool_specs();
    assert!(tool_specs.is_some(), "request should have tool specs");

    let tools = tool_specs.unwrap();
    let has_tool_search = tools.iter().any(|t| t.name == "tool_search");
    assert!(
        !has_tool_search,
        "tool_search should NOT be present when tool_search_enabled=false"
    );
}

/// Tests that a custom agent without ToolSearch in its tools list still gets full MCP tool specs
/// even when tool_search_enabled=true in settings. The ToolSearch deferred-tool logic should
/// only activate when the ToolSearch tool is actually available to the agent.
#[tokio::test]
async fn test_custom_agent_without_tool_search_gets_full_mcp_tools() {
    use std::collections::HashMap;

    use agent::agent_config::definitions::{
        McpServerConfig,
        RemoteMcpServerConfig,
    };
    use mock_mcp_server::{
        MockMcpServerBuilder,
        ToolDef,
        prebuild_bin,
    };

    let _ = tracing_subscriber::fmt::try_init();

    prebuild_bin().expect("failed to prebuild mock-mcp-server");

    let handle = MockMcpServerBuilder::new()
        .add_tool(ToolDef {
            name: "database_query".to_string(),
            description: "Execute SQL database queries".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"query": {"type": "string"}}}),
        })
        .spawn_http()
        .expect("failed to spawn mock MCP server");

    handle
        .wait_ready(std::time::Duration::from_secs(5))
        .expect("mock MCP server not ready");

    let mcp_config = McpServerConfig::Remote(RemoteMcpServerConfig {
        url: handle.url(),
        headers: HashMap::new(),
        timeout_ms: 30000,
        oauth_scopes: Vec::new(),
        oauth: None,
        disabled: false,
        disabled_tools: Vec::new(),
        force_auth: false,
    });

    // Custom agent with specific tools — no ToolSearch, no wildcard "*"
    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        name: "custom_restricted".to_string(),
        tools: vec![
            "read".to_string(),
            "write".to_string(),
            "shell".to_string(),
            "@testdb".to_string(),
        ],
        ..Default::default()
    });

    // Enable tool_search in settings with no thresholds (always activate)
    let settings = agent::types::AgentSettings {
        tool_search_enabled: true,
        tool_search_min_pct: None,
        tool_search_min_tokens: None,
        ..Default::default()
    };

    let mut test = TestCase::builder()
        .test_name("custom agent without tool_search gets full mcp tools")
        .with_agent_config(agent_config)
        .with_settings(settings)
        .with_mcp_server("testdb", mcp_config)
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/single_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.wait_until_agent_event(Duration::from_secs(10), |evt| matches!(evt, AgentEvent::Initialized))
        .await
        .expect("timed out waiting for agent initialization");

    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(10)).await.unwrap();

    let requests = test.requests();
    assert!(!requests.is_empty(), "expected at least one request");

    let tool_specs = requests[0].tool_specs().expect("request 0 should have tool specs");

    // ToolSearch should NOT be in tool specs (not in agent's tools list)
    let has_tool_search = tool_specs.iter().any(|t| t.name == "tool_search");
    assert!(
        !has_tool_search,
        "tool_search should NOT be present (not in agent's tools list)"
    );

    // MCP tool SHOULD still be present — since ToolSearch is not available,
    // the deferred tool logic should not filter out MCP tools
    let has_mcp_tool = tool_specs.iter().any(|t| t.name.contains("database_query"));
    assert!(
        has_mcp_tool,
        "MCP tool database_query SHOULD be present when ToolSearch is not in agent's tools"
    );

    // Deferred tools list should NOT be in context messages
    let context_msg = requests[0]
        .messages()
        .first()
        .expect("first message should exist")
        .text();
    assert!(
        !context_msg.contains(DEFERRED_TOOLS_MESSAGE),
        "DEFERRED_TOOLS_MESSAGE should NOT be in context when ToolSearch is unavailable"
    );
}

/// Tests that tool dispatch recovers when cached_tool_specs is invalidated
/// between format_request and parse_tools (e.g., by a late MCP ToolListChanged event).
///
/// This reproduces a bug where subagent tool calls silently failed because:
/// 1. Agent sends request to model (caches tool specs)
/// 2. MCP server fires Initialized/ToolListChanged → cached_tool_specs = None
/// 3. Model responds with tool use → parse_tools finds cached_tool_specs = None
/// 4. Old behavior: tool silently dropped. Fixed behavior: specs rebuilt lazily.
#[tokio::test]
async fn test_tool_dispatch_recovers_after_cached_specs_invalidated() {
    let _ = tracing_subscriber::fmt::try_init();

    let response_streams = parse_response_streams(include_str!("./mock_responses/fs_read_only.jsonl"))
        .await
        .unwrap();

    // Use a delay on the first response so we can invalidate specs before the tool use arrives
    let delayed_first_response =
        agent::agent_loop::model::MockResponse::with_delay(response_streams[0].clone(), Duration::from_millis(200));

    let mut test = TestCase::builder()
        .test_name("tool dispatch recovers after cached specs invalidated")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_file(("test.txt", "hello world"))
        .with_mock_response(delayed_first_response)
        .with_mock_response(response_streams[1].clone().into())
        .build()
        .await
        .unwrap();

    // Send prompt — model will respond with a read tool use after 200ms delay
    test.send_prompt("read test.txt".to_string()).await;

    // Invalidate cached tool specs while the model response is delayed.
    // This simulates an MCP ToolListChanged/Initialized event arriving
    // between format_request (which caches specs) and parse_tools.
    test.invalidate_cached_tool_specs().await;

    // The agent should recover: rebuild specs in parse_tools and execute the tool.
    // Without the fix, the tool would be silently dropped and the agent would hang.
    test.wait_until_agent_stop(Duration::from_secs(5))
        .await
        .expect("agent should complete — tool specs should be rebuilt lazily in parse_tools");

    // Verify the tool was actually executed (there should be a tool result in the second request)
    let requests = test.requests();
    assert!(
        requests.len() >= 2,
        "expected at least 2 requests (initial + tool result), got {}",
        requests.len()
    );

    // The second request should contain a successful tool result
    let has_tool_result = requests[1].has_tool_result(|result| {
        result
            .content
            .iter()
            .any(|c| matches!(c, ToolResultContentBlock::Text(t) if t.contains("hello world")))
    });
    assert!(
        has_tool_result,
        "expected successful tool result containing file content 'hello world'"
    );
}

/// Enterprise MCP governance: when `AgentSettings.mcp_enabled == false`, the Agent must
/// construct with an empty `cached_mcp_configs` — MCP servers configured on the agent
/// are suppressed before any subprocess is spawned. This is the defense-in-depth guarantee
/// backing the Kiro CLI V2 TUI fix for P423069569.
#[tokio::test]
async fn test_agent_new_suppresses_mcp_when_governance_disabled() {
    use agent::agent_config::definitions::{
        LocalMcpServerConfig,
        McpServerConfig,
    };

    let _ = tracing_subscriber::fmt::try_init();

    let mcp_config = McpServerConfig::Local(LocalMcpServerConfig {
        command: "/bin/echo".to_string(),
        args: vec!["unused".to_string()],
        env: None,
        timeout_ms: 30_000,
        disabled: false,
        disabled_tools: vec![],
    });

    let settings = agent::types::AgentSettings {
        mcp_enabled: false,
        ..Default::default()
    };

    let test = TestCase::builder()
        .test_name("mcp_governance_suppresses_servers")
        .with_default_agent_config()
        .with_settings(settings)
        .with_mcp_server("should-not-launch", mcp_config)
        .build()
        .await
        .unwrap();

    // Create a snapshot of the live agent to inspect its effective config.
    let snapshot = test.create_snapshot().await;

    assert!(
        snapshot.agent_config.config().mcp_servers().is_empty(),
        "Agent::new must clear MCP servers when settings.mcp_enabled == false; got: {:?}",
        snapshot.agent_config.config().mcp_servers()
    );
    assert!(
        !snapshot.agent_config.config().use_legacy_mcp_json(),
        "Agent::new must reset use_legacy_mcp_json when settings.mcp_enabled == false"
    );
    assert!(
        !snapshot
            .agent_config
            .tools()
            .iter()
            .any(|t| t.starts_with('@') && !t.starts_with("@builtin")),
        "Agent::new must strip MCP tool refs from agent config tools list"
    );
    assert!(
        !snapshot.settings.mcp_enabled,
        "settings.mcp_enabled must be persisted on the snapshot"
    );
}

/// Baseline: when governance allows MCP, the configured server is retained on the agent
/// (the actual spawn/launch is mocked elsewhere; here we only assert that governance is
/// NOT stripping the config).
#[tokio::test]
async fn test_agent_new_keeps_mcp_when_governance_enabled() {
    use agent::agent_config::definitions::{
        LocalMcpServerConfig,
        McpServerConfig,
    };

    let _ = tracing_subscriber::fmt::try_init();

    let mcp_config = McpServerConfig::Local(LocalMcpServerConfig {
        command: "/bin/echo".to_string(),
        args: vec!["unused".to_string()],
        env: None,
        timeout_ms: 30_000,
        disabled: false,
        disabled_tools: vec![],
    });

    let settings = agent::types::AgentSettings {
        mcp_enabled: true,
        ..Default::default()
    };

    let test = TestCase::builder()
        .test_name("mcp_governance_allows_servers")
        .with_default_agent_config()
        .with_settings(settings)
        .with_mcp_server("should-stay", mcp_config)
        .build()
        .await
        .unwrap();

    let snapshot = test.create_snapshot().await;

    assert!(
        snapshot.agent_config.config().mcp_servers().contains_key("should-stay"),
        "MCP server should remain configured when governance is enabled"
    );
}

/// Defense-in-depth for `swap_agent`: even if the SessionManager / TUI somehow provides
/// a swap target with MCP servers, the Agent must re-strip them when `mcp_enabled == false`.
/// This matters because `/agent <name>` re-evaluates the underlying agent config, and any
/// future regressions in the SessionManager pre-clearing logic must still be caught here.
#[tokio::test]
async fn test_agent_swap_agent_suppresses_mcp_when_governance_disabled() {
    use agent::agent_config::definitions::{
        AgentConfig,
        AgentConfigV2025_08_22,
        LocalMcpServerConfig,
        McpServerConfig,
    };
    use agent::agent_config::{
        ConfigSource,
        LoadedAgentConfig,
        ResolvedGlobalPrompt,
    };
    use agent::protocol::SwapAgentArgs;

    let _ = tracing_subscriber::fmt::try_init();

    // Agent starts with MCP-off governance, no MCP servers.
    let settings = agent::types::AgentSettings {
        mcp_enabled: false,
        ..Default::default()
    };
    let test = TestCase::builder()
        .test_name("mcp_governance_swap_defense_in_depth")
        .with_default_agent_config()
        .with_settings(settings)
        .build()
        .await
        .unwrap();

    // Build a swap target agent that DOES have MCP servers — simulating a caller that
    // failed to pre-strip (e.g. a future SessionManager bug).
    let mut target_inner = AgentConfigV2025_08_22 {
        name: "rogue_agent".to_string(),
        tools: vec!["*".to_string(), "@contraband/run".to_string()],
        use_legacy_mcp_json: true,
        ..Default::default()
    };
    target_inner.mcp_servers.insert(
        "contraband".to_string(),
        McpServerConfig::Local(LocalMcpServerConfig {
            command: "/bin/echo".to_string(),
            args: vec!["should-be-suppressed".to_string()],
            env: None,
            timeout_ms: 30_000,
            disabled: false,
            disabled_tools: vec![],
        }),
    );
    let target = LoadedAgentConfig::new(
        AgentConfig::V2025_08_22(target_inner),
        ConfigSource::Ephemeral,
        ResolvedGlobalPrompt::None,
    );

    test.swap_agent(SwapAgentArgs {
        agent_config: target,
        force: false,
        knowledge_provider: None,
    })
    .await
    .expect("swap_agent failed");

    let snapshot = test.create_snapshot().await;

    assert!(
        snapshot.agent_config.config().mcp_servers().is_empty(),
        "swap_agent must clear MCP servers when settings.mcp_enabled == false; got: {:?}",
        snapshot.agent_config.config().mcp_servers()
    );
    assert!(
        !snapshot.agent_config.config().use_legacy_mcp_json(),
        "swap_agent must reset use_legacy_mcp_json when settings.mcp_enabled == false"
    );
    assert!(
        !snapshot
            .agent_config
            .tools()
            .iter()
            .any(|t| t.starts_with('@') && !t.starts_with("@builtin")),
        "swap_agent must strip MCP tool refs from tools list; got: {:?}",
        snapshot.agent_config.tools()
    );
}

#[tokio::test]
async fn test_swap_agent_updates_knowledge_provider() {
    use std::sync::Arc;

    use agent::agent_config::definitions::{
        AgentConfig,
        AgentConfigV2025_08_22,
    };
    use agent::agent_config::{
        ConfigSource,
        LoadedAgentConfig,
        ResolvedGlobalPrompt,
    };
    use agent::protocol::SwapAgentArgs;
    use agent::tools::{
        Knowledge,
        KnowledgeProvider,
        ToolExecutionOutput,
        ToolExecutionOutputItem,
        ToolExecutionResult,
    };
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockKnowledgeProvider {
        name: String,
    }

    #[async_trait]
    impl KnowledgeProvider for MockKnowledgeProvider {
        async fn execute(&self, _command: Knowledge) -> ToolExecutionResult {
            Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Text(format!(
                "mock-kb: {}",
                self.name
            ))]))
        }

        async fn list_available(&self) -> Option<String> {
            Some(format!("KB: {}", self.name))
        }
    }

    let _ = tracing_subscriber::fmt::try_init();

    let test = TestCase::builder()
        .test_name("swap_agent_updates_knowledge_provider")
        .with_default_agent_config()
        .build()
        .await
        .unwrap();

    // Initially no knowledge provider
    let snapshot = test.create_snapshot().await;
    assert!(
        !snapshot.has_knowledge_provider,
        "agent should start without a knowledge provider"
    );

    // Swap with a knowledge provider
    let target = LoadedAgentConfig::new(
        AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: "agent-alpha".to_string(),
            ..Default::default()
        }),
        ConfigSource::Ephemeral,
        ResolvedGlobalPrompt::None,
    );

    let provider_alpha: Arc<dyn KnowledgeProvider> = Arc::new(MockKnowledgeProvider {
        name: "alpha".to_string(),
    });

    test.swap_agent(SwapAgentArgs {
        agent_config: target,
        force: false,
        knowledge_provider: Some(provider_alpha),
    })
    .await
    .expect("swap to alpha failed");

    let snapshot = test.create_snapshot().await;
    assert!(
        snapshot.has_knowledge_provider,
        "knowledge provider should be set after swap with provider"
    );

    // Swap to another agent with a different provider
    let target_beta = LoadedAgentConfig::new(
        AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: "agent-beta".to_string(),
            ..Default::default()
        }),
        ConfigSource::Ephemeral,
        ResolvedGlobalPrompt::None,
    );

    let provider_beta: Arc<dyn KnowledgeProvider> = Arc::new(MockKnowledgeProvider {
        name: "beta".to_string(),
    });

    test.swap_agent(SwapAgentArgs {
        agent_config: target_beta,
        force: false,
        knowledge_provider: Some(provider_beta),
    })
    .await
    .expect("swap to beta failed");

    let snapshot = test.create_snapshot().await;
    assert!(
        snapshot.has_knowledge_provider,
        "knowledge provider should still be set after second swap"
    );

    // Swap without a provider (e.g. MCP reload) should NOT clear it
    let target_reload = LoadedAgentConfig::new(
        AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: "agent-beta".to_string(),
            ..Default::default()
        }),
        ConfigSource::Ephemeral,
        ResolvedGlobalPrompt::None,
    );

    test.swap_agent(SwapAgentArgs {
        agent_config: target_reload,
        force: true,
        knowledge_provider: None,
    })
    .await
    .expect("reload swap failed");

    let snapshot = test.create_snapshot().await;
    assert!(
        snapshot.has_knowledge_provider,
        "knowledge provider should be preserved when swap passes None (reload case)"
    );
}

/// Part A: a model call to the unavailable `dummy` placeholder tool must
/// resolve to a benign, instructional tool_result (Success) rather than a hard
/// `NameDoesNotExist` parse error. This is what lets the model self-correct
/// (e.g. switch to an agent that provides the tool) instead of looping on an
/// unavailable tool after a cross-agent handoff.
#[tokio::test]
async fn test_dummy_tool_call_returns_benign_result() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("dummy tool call returns benign result")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/dummy_tool_call_benign.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("do the thing".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    let requests = test.requests();
    assert_eq!(
        requests.len(),
        2,
        "expected initial request + one resend carrying the dummy tool_result, got {}",
        requests.len()
    );

    // The dummy call must resolve to a SUCCESS tool_result (benign no-op), not a
    // NameDoesNotExist error.
    let resend = &requests[1];
    assert!(
        resend.has_tool_result(
            |tr| tr.tool_use_id == "tooluse_dummy_1" && matches!(tr.status, ToolResultStatus::Success)
        ),
        "dummy tool call should yield a successful (benign) tool_result, not an error"
    );

    // The result text must carry the instructional guidance about the
    // unavailable tool belonging to a different agent -- NOT a "does not
    // exist" / parse-error message.
    let text = resend
        .messages()
        .last()
        .expect("resend should have a last message")
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::ToolResult(tr) if tr.tool_use_id == "tooluse_dummy_1" => Some(
                tr.content
                    .iter()
                    .filter_map(|b| match b {
                        ToolResultContentBlock::Text(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .expect("dummy tool_use must have a tool_result");
    assert!(
        text.contains("not available") && text.contains("different agent"),
        "dummy guidance should describe an unavailable tool belonging to a different agent; got: {text}"
    );
    assert!(
        !text.contains("does not exist") && !text.contains("Failed to parse the tool use"),
        "dummy must not be reported as a nonexistent / unparseable tool; got: {text}"
    );
}

/// Part B: when the model repeatedly calls the unavailable `dummy` tool and
/// never ends the turn, the consecutive-unexecutable breaker must stop
/// auto-resending after the cap (3) and force-end the turn — so the agent can
/// never infinite-loop and the ACP bridge's pending prompt is released.
#[tokio::test]
async fn test_repeated_dummy_tool_calls_break_loop() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("repeated dummy tool calls break loop")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/repeated_dummy_tool_calls.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("keep going".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    // Exactly 3 requests are sent (initial + 2 resends); the 3rd consecutive
    // unexecutable turn trips the breaker, so no 4th request is made.
    let requests = test.requests();
    assert_eq!(
        requests.len(),
        3,
        "breaker should stop auto-resending after 3 unexecutable turns; got {} requests",
        requests.len()
    );

    // A clear assistant message explaining the stop must be surfaced to the user.
    let surfaced = test.agent_events().iter().any(|e| {
        matches!(
            e,
            AgentEvent::Update(agent::protocol::UpdateEvent::AgentContent(
                agent::protocol::ContentChunk::Text(t),
            )) if t.contains("Stopped after repeated attempts to call tools that aren't available")
        )
    });
    assert!(
        surfaced,
        "expected a 'Stopped after repeated attempts to call tools that aren't available' assistant message"
    );

    // The turn must terminate (EndTurn) so the ACP bridge releases the prompt.
    assert!(
        test.agent_events().iter().any(|e| matches!(e, AgentEvent::EndTurn(_))),
        "expected EndTurn to be emitted so the pending prompt resolves"
    );
}

/// Part B (reset): a successful tool dispatch in the middle of a run resets the
/// unavailable-tool breaker, so it only trips on consecutive unexecutable
/// turns. Flow: dummy, dummy, read(success → reset), dummy, dummy, dummy(trip).
/// A correct implementation sends exactly 6 requests; without the reset it
/// would trip at request 4.
#[tokio::test]
async fn test_unexecutable_tool_breaker_resets_after_success() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test = TestCase::builder()
        .test_name("unexecutable tool breaker resets after success")
        .with_default_agent_config()
        .with_trust_all_tools(true)
        .with_file(("a.txt", "alpha"))
        .with_responses(
            parse_response_streams(include_str!(
                "./mock_responses/dummy_breaker_resets_after_success.jsonl"
            ))
            .await
            .unwrap(),
        )
        .build()
        .await
        .unwrap();

    test.send_prompt("work on it".to_string()).await;
    test.wait_until_agent_stop(Duration::from_secs(5)).await.unwrap();

    // The successful read resets the counter, so the breaker only trips on the
    // 3rd CONSECUTIVE dummy after the reset: exactly 6 requests.
    let requests = test.requests();
    assert_eq!(
        requests.len(),
        6,
        "breaker should reset after the successful tool; expected 6 requests, got {}",
        requests.len()
    );
    assert!(
        test.agent_events().iter().any(|e| matches!(e, AgentEvent::EndTurn(_))),
        "expected EndTurn after the breaker eventually trips"
    );
}
