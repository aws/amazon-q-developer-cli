mod common;

use std::collections::HashMap;
use std::time::Duration;

use agent::agent_config::definitions::*;
use common::*;
use tracing::info;

fn create_spawn_hook_single_shell_config() -> AgentConfig {
    let hooks = HashMap::from([(HookTrigger::AgentSpawn, vec![HookConfig::ShellCommand(CommandHook {
        command: "echo 'Agent initialized'".to_string(),
        opts: BaseHookConfig {
            timeout_ms: 5000,
            max_output_size: 1024,
            cache_ttl_seconds: 0,
            matcher: None,
        },
    })])]);

    AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        hooks,
        ..Default::default()
    })
}

#[tokio::test]
async fn test_agent_spawn_hook_single() {
    let _ = tracing_subscriber::fmt::try_init();

    let mut test: TestCase = TestCase::builder()
        .test_name("agent spawn hook behavior for single shell command and one turn")
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
        .test_name("agent spawn hook behavior for single shell command and two turns")
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

#[tokio::test]
async fn test_user_prompt_hook_single() {
    let _ = tracing_subscriber::fmt::try_init();
    let hooks = HashMap::from([(HookTrigger::UserPromptSubmit, vec![HookConfig::ShellCommand(
        CommandHook {
            command: "echo 'submitted!'".to_string(),
            opts: BaseHookConfig {
                timeout_ms: 5000,
                max_output_size: 1024,
                cache_ttl_seconds: 0,
                matcher: None,
            },
        },
    )])]);

    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        hooks,
        ..Default::default()
    });
    let mut test: TestCase = TestCase::builder()
        .test_name("user prompt submit hook behavior for single shell command and one turn")
        .with_agent_config(agent_config)
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
    assert_contains(&first_msg, "submitted!");
}

#[tokio::test]
async fn test_user_prompt_hook_persistence() {
    let _ = tracing_subscriber::fmt::try_init();

    let hooks = HashMap::from([(HookTrigger::UserPromptSubmit, vec![HookConfig::ShellCommand(
        CommandHook {
            command: "printf 'a' >> turns.txt; printf \"char count: $(wc -c < turns.txt | tr -d ' ')\"".to_string(),
            opts: BaseHookConfig {
                timeout_ms: 5000,
                max_output_size: 1024,
                cache_ttl_seconds: 0,
                matcher: None,
            },
        },
    )])]);

    let agent_config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
        hooks,
        ..Default::default()
    });
    let mut test: TestCase = TestCase::builder()
        .test_name("user prompt submit hook behavior for single shell command and one turn")
        .with_agent_config(agent_config)
        .with_test_perprompt_hook()
        .with_file(("turns.txt", ""))
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
    let req = test.requests().first().expect("first request should exist");
    let first_msg = req.messages().first().expect("first message should exist").text();
    assert_contains(&first_msg, "char count: 1");
    let req = test.requests().get(1).expect("second request should exist");
    let first_msg = req.messages().first().expect("first message should exist").text();
    assert_contains(&first_msg, "char count: 2");
}
