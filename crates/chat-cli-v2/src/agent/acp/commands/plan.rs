use agent::tui_commands::{
    AgentArgs,
    CommandResult,
};

use super::{
    CommandContext,
    agent as agent_cmd,
};
use crate::constants::PLANNER_AGENT_NAME;

pub async fn execute(prompt: Option<&str>, ctx: &CommandContext<'_>) -> CommandResult {
    let result = agent_cmd::execute(
        &AgentArgs {
            agent_name: Some(PLANNER_AGENT_NAME.to_string()),
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
