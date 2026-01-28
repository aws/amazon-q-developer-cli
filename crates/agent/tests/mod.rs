mod common;

use std::time::Duration;

use agent::ActiveState;
use agent::agent_config::definitions::{
    AgentConfig,
    CommandHook,
    HookConfig,
    HookTrigger,
};
use agent::protocol::{
    AgentEvent,
    ApprovalResult,
    CompactionEvent,
    SendApprovalResultArgs,
};
use common::*;

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
        .with_agent_config(AgentConfig::default())
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
                result: ApprovalResult::Approve,
            },
            SendApprovalResultArgs {
                id: "tooluse_second".into(),
                result: ApprovalResult::Approve,
            },
            SendApprovalResultArgs {
                id: "tooluse_third".into(),
                result: ApprovalResult::Approve,
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
        .with_agent_config(AgentConfig::default())
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/builtin_tools.jsonl"))
                .await
                .unwrap(),
        )
        .with_tool_use_approvals([
            SendApprovalResultArgs {
                id: "tooluse_first".into(),
                result: ApprovalResult::Approve,
            },
            SendApprovalResultArgs {
                id: "tooluse_second".into(),
                result: ApprovalResult::Approve,
            },
            SendApprovalResultArgs {
                id: "tooluse_third".into(),
                result: ApprovalResult::Approve,
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

    // Responses: tool use -> tool use -> context overflow -> compaction summary -> retry success
    let responses = parse_response_streams(include_str!("./mock_responses/context_window_overflow.jsonl"))
        .await
        .unwrap();

    let mut test = TestCase::builder()
        .test_name("auto compaction on context overflow")
        .with_agent_config(AgentConfig::default())
        .with_responses(responses)
        .with_trust_all_tools(true)
        .build()
        .await
        .unwrap();

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

    // Verify the request was retried after compaction
    // Request 2 has ls tool result (before overflow), request 4 has it again (after compaction retry)
    let requests = test.requests();
    let ls_tool_result_count = requests
        .iter()
        .filter(|r| r.has_tool_result(|tr| tr.tool_use_id.contains("kexAaD9RRkyTgeHlCu7bRA")))
        .count();
    assert!(
        ls_tool_result_count >= 2,
        "expected at least 2 requests with ls tool result (original + retry), found {}",
        ls_tool_result_count
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
        .with_agent_config(AgentConfig::default())
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
