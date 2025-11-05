mod common;

use agent::agent_config::definitions::*;
use common::*;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

fn create_spawn_hook_single_shell_config() -> AgentConfig {
    let hooks = HashMap::from([
        (
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
        ),
    ]);

    AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        hooks,
        ..Default::default()
    })
}


#[tokio::test]
async fn test_agent_spawn_hook_single() {
    let _ = tracing_subscriber::fmt::try_init();
   
    let mut test: TestCase = TestCase::builder()
        .test_name("agent spawn hook behavior")
        .with_agent_config(create_spawn_hook_single_shell_config())
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/simple_two_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();
    
    test.wait_until_agent_initializes(Duration::from_millis(100)).await;
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_millis(100)).await;
    let req = test.requests().first().expect("should have one request");
    let first_msg = req.messages().first().expect("first message should exist").text(); 
    assert_contains(&first_msg, "Agent initialized");
}

#[tokio::test]
async fn test_agent_spawn_hook_persistence() {
    let _ = tracing_subscriber::fmt::try_init();
   
    let mut test: TestCase = TestCase::builder()
        .test_name("agent spawn hook behavior")
        .with_agent_config(create_spawn_hook_single_shell_config())
        .with_responses(
            parse_response_streams(include_str!("./mock_responses/simple_two_turn.jsonl"))
                .await
                .unwrap(),
        )
        .build()
        .await
        .unwrap();
    
    test.wait_until_agent_initializes(Duration::from_millis(100)).await;
    test.send_prompt("hello".to_string()).await;
    test.wait_until_agent_stop(Duration::from_millis(100)).await;
    test.send_prompt("bye".to_string()).await;
    test.wait_until_agent_stop(Duration::from_millis(100)).await;
    for req in test.requests() {
        let first_msg = req.messages().first().expect("first message should exist").text(); 
        assert_contains(&first_msg, "Agent initialized");
    }
}
