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
