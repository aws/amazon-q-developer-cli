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

    let mut test = TestCase::builder()
        .test_name("agent default config behavior")
        .with_agent_config(AgentConfig::default())
        .with_file(("AmazonQ.md", AMAZON_Q_MD_CONTENT))
        .with_file(("AGENTS.md", AGENTS_MD_CONTENT))
        .with_file(("README.md", README_MD_CONTENT))
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

    test.wait_until_agent_stop(Duration::from_secs(3)).await;
}
