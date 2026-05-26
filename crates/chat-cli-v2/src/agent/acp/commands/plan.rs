use agent::tui_commands::{
    AgentArgs,
    CommandResult,
};

use super::{
    CommandContext,
    agent as agent_cmd,
};

/// KAS's built-in quick-plan mode ID.
const QUICK_PLAN_MODE: &str = "quick-plan";

pub async fn execute(prompt: Option<&str>, ctx: &CommandContext<'_>) -> CommandResult {
    let result = agent_cmd::execute(
        &AgentArgs {
            agent_name: Some(QUICK_PLAN_MODE.to_string()),
        },
        ctx,
    )
    .await;

    if result.success
        && let Some(p) = prompt
        && !p.is_empty()
    {
        let _ = ctx
            .agent
            .send_prompt(agent::protocol::SendPromptArgs {
                content: vec![agent::protocol::ContentChunk::Text(p.to_string())],
                should_continue_turn: None,
            })
            .await;
    }
    result
}
