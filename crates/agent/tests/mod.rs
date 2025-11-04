mod common;

use std::time::Duration;

use agent::agent_config::definitions::AgentConfig;
use agent::protocol::{
    ApprovalResult,
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
    
    test.wait_until_agent_initializes(Duration::from_millis(100)).await;

    test.send_prompt("start turn".to_string()).await;
    
    test.wait_until_agent_stop(Duration::from_secs(2)).await;

    for req in test.requests() {
        let first_msg = req.messages().first().expect("first message should exist").text();
        assert_contains(&first_msg, AMAZON_Q_MD_CONTENT);
        assert_contains(&first_msg,AGENTS_MD_CONTENT);
        assert_contains(&first_msg,README_MD_CONTENT);
        assert_contains(&first_msg,LOCAL_RULE_MD_CONTENT);
        assert_contains(&first_msg,SUB_LOCAL_RULE_MD_CONTENT);
    }
}
