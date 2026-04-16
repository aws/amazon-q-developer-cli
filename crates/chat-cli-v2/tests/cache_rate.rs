mod common;

use std::collections::HashMap;

use agent::agent_config::definitions::{
    AgentConfigV2025_08_22,
    BaseHookConfig,
    CommandHook,
    HookConfig,
    HookTrigger,
};
use aws_smithy_types::Document as AwsDocument;
use chat_cli_v2::api_client::model::{
    ChatMessage,
    ChatResponseStream,
    ConversationState,
    FigDocument,
};
use chat_cli_v2::api_client::send_message_output::MockStreamItem;
use common::AcpTestHarnessBuilder;
use ntest::timeout;
use serial_test::serial;

fn simple_response(text: &str) -> Vec<MockStreamItem> {
    vec![MockStreamItem::Event(ChatResponseStream::AssistantResponseEvent {
        content: text.to_string(),
    })]
}

/// Clears HashMap-backed fields from a ChatMessage that have non-deterministic
/// serialization order. Specifically, `ToolUse.input` is stored as
/// `FigDocument(Document::Object(HashMap))` which scrambles key order.
///
/// The backend explicitly sorts tool use/result JSON for prompt caching,
/// so these fields don't affect cache hit rate on the wire.
fn clear_nondeterministic_fields(msg: &mut ChatMessage) {
    let empty_input = FigDocument::from(AwsDocument::Object(HashMap::new()));
    match msg {
        ChatMessage::AssistantResponseMessage(m) => {
            if let Some(tool_uses) = &mut m.tool_uses {
                for tu in tool_uses {
                    tu.input = empty_input.clone();
                }
            }
        },
        ChatMessage::UserInputMessage(_) => {},
    }
}

fn tool_specs_json(conv: &ConversationState) -> Vec<serde_json::Value> {
    conv.user_input_message
        .user_input_message_context
        .as_ref()
        .and_then(|ctx| ctx.tools.as_ref())
        .map(|tools| tools.iter().map(|t| serde_json::to_value(t).unwrap()).collect())
        .unwrap_or_default()
}

fn make_hook(cmd: &str) -> Vec<HookConfig> {
    vec![HookConfig::ShellCommand(CommandHook {
        command: cmd.to_string(),
        opts: BaseHookConfig::default(),
    })]
}

/// Sends 150 prompts (with read/write tool calls every 10 turns) through the full
/// ACP pipeline using a custom agent config with resources, hooks, and a global prompt.
///
/// Asserts:
/// 1. Tool specs are sorted alphabetically and identical across all requests.
/// 2. Each request's history is an exact prefix of the next request's history (the "prefix match"
///    property required for prompt caching).
#[tokio::test]
#[timeout(120000)]
#[serial]
async fn prefix_match_across_turns() {
    const NUM_TURNS: usize = 150;

    let tool_response_files = [
        "tests/mock_responses/cache_rate_tool_read.jsonl",
        "tests/mock_responses/cache_rate_tool_write.jsonl",
    ];

    let hooks: HashMap<HookTrigger, Vec<HookConfig>> = [
        (HookTrigger::AgentSpawn, make_hook("echo agent-spawn")),
        (HookTrigger::UserPromptSubmit, make_hook("echo prompt-submit")),
        (HookTrigger::PreToolUse, make_hook("echo pre-tool")),
        (HookTrigger::PostToolUse, make_hook("echo post-tool")),
        (HookTrigger::Stop, make_hook("echo stop")),
    ]
    .into();

    let config = AgentConfigV2025_08_22 {
        name: "cache_test_agent".to_string(),
        description: Some("Agent for cache rate testing".to_string()),
        global_prompt: Some("You are a test agent. Follow the project guidelines.".to_string()),
        tools: vec!["*".to_string()],
        resources: vec!["file://GUIDELINES.md".parse().unwrap()],
        hooks,
        ..Default::default()
    };

    let (mut harness, client, session_id, _) = AcpTestHarnessBuilder::new("prefix_match_across_turns")
        .with_agent_config("cache_test_agent", &config)
        .with_setting("chat.defaultAgent", "cache_test_agent")
        .with_trust_all(true)
        .build_with_session()
        .await;

    std::fs::write(
        harness.paths.cwd.join("GUIDELINES.md"),
        "# Project Guidelines\nAlways write tests.\n",
    )
    .unwrap();
    std::fs::write(harness.paths.cwd.join("test_file.txt"), "line 1\nline 2\nline 3\n").unwrap();

    for i in 0..NUM_TURNS {
        if i > 0 && i % 10 == 0 {
            let file = tool_response_files[(i / 10 - 1) % tool_response_files.len()];
            harness.push_mock_responses_from_file(&session_id.0, file).await;
        } else {
            harness
                .push_mock_response(&session_id.0, Some(simple_response(&format!("Response {i}"))))
                .await;
            harness.push_mock_response(&session_id.0, None).await;
        }
    }

    for i in 0..NUM_TURNS {
        client
            .prompt_text(session_id.clone(), &format!("prompt {i}"))
            .await
            .expect(&format!("prompt {i} failed"));
    }

    let captured = harness.get_captured_requests(&session_id.0).await;
    assert!(
        captured.len() >= NUM_TURNS,
        "expected at least {NUM_TURNS} captured requests, got {}",
        captured.len()
    );

    // Tool specs must be identical across every request.
    let first_specs = tool_specs_json(&captured[0]);
    assert!(!first_specs.is_empty(), "should have tools");
    for (i, req) in captured.iter().enumerate().skip(1) {
        assert_eq!(first_specs, tool_specs_json(req), "tool specs differ at request {i}");
    }

    // History prefix match: request N's full history must be an exact prefix of
    // request N+1's history. ToolUse.input fields are cleared before comparison
    // because FigDocument(HashMap) has non-deterministic serialization order.
    for i in 1..captured.len() {
        let prev_history = captured[i - 1].history.as_deref().unwrap_or(&[]);
        let curr_history = captured[i].history.as_deref().unwrap_or(&[]);

        assert!(
            curr_history.len() >= prev_history.len(),
            "request {i} history ({} msgs) shorter than request {} ({} msgs)",
            curr_history.len(),
            i - 1,
            prev_history.len(),
        );

        // Clear unstable JSON HashMap-containing values
        let stabilize = |msgs: &[ChatMessage]| -> Vec<String> {
            msgs.iter()
                .map(|m| {
                    let mut m = m.clone();
                    clear_nondeterministic_fields(&mut m);
                    serde_json::to_string(&m).unwrap()
                })
                .collect()
        };
        let prev_json = stabilize(prev_history);
        let curr_prefix = stabilize(&curr_history[..prev_history.len()]);

        assert_eq!(
            prev_json,
            curr_prefix,
            "request {}'s history is NOT a prefix of request {}'s history \
             (diverges within first {} messages)",
            i - 1,
            i,
            prev_history.len(),
        );
    }
}
