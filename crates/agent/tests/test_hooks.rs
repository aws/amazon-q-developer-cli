mod common;

use agent::agent_config::definitions::*;
use common::*;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

#[tokio::test]
async fn test_agent_spawn_hook() {
    let _ = tracing_subscriber::fmt::try_init();
    // creates the agent config with the agent spawn hook
    let mut hooks = HashMap::new();
    hooks.insert(
        HookTrigger::AgentSpawn,
        vec![HookConfig::ShellCommand(CommandHook {
            command: "echo 'Agent initialized'".to_string(),
            opts: BaseHookConfig {
                timeout_ms: 5000,
                max_output_size: 1024,
                cache_ttl_seconds: 0,
                matcher: None,
            },
        })],
    );

    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        hooks,
        ..Default::default()
    });
    let mut test: TestCase = TestCase::builder()
        .test_name("agent spawn hook behavior")
        .with_agent_config(agent_config)
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/simple_two_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();
    
    test.wait_until_agent_initializes(Duration::from_millis(500)).await;
    test.send_prompt("hello".to_string()).await;
    info!("before stop");
    test.wait_until_agent_stop(Duration::from_secs(10)).await;
    info!("after stop");
    for req in test.requests() {
        let first_msg = req.messages().first().expect("first message should exist").text(); 
        assert_contains(&first_msg, "Agent initialized");
    }
}
