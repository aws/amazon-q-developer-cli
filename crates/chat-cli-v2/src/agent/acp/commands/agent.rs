//! /agent command execution

use agent::protocol::SwapAgentArgs;
use agent::tui_commands::{
    AgentArgs,
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
};

use super::CommandContext;

pub async fn execute(args: &AgentArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match &args.agent_name {
        None => list_agents(ctx),
        Some(name) => switch_agent(name, ctx).await,
    }
}

pub fn get_options(partial: &str, ctx: &CommandContext<'_>) -> CommandOptionsResponse {
    let partial_lower = partial.to_lowercase();
    let mut options: Vec<CommandOption> = ctx
        .available_agents
        .iter()
        .filter(|a| {
            partial.is_empty()
                || a.name.to_lowercase().contains(&partial_lower)
                || a.description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&partial_lower))
        })
        .map(|a| CommandOption {
            value: a.name.clone(),
            label: a.name.clone(),
            description: a.description.clone(),
            group: None,
        })
        .collect();
    options.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    CommandOptionsResponse {
        options,
        has_more: false,
    }
}

fn list_agents(ctx: &CommandContext<'_>) -> CommandResult {
    let current = ctx.current_agent_name;
    let message = ctx
        .available_agents
        .iter()
        .map(|a| {
            let marker = if a.name == current { "→ " } else { "  " };
            match &a.description {
                Some(desc) => format!("{}{} - {}", marker, a.name, desc),
                None => format!("{}{}", marker, a.name),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    CommandResult::success_with_data(
        message,
        serde_json::json!({ "agents": ctx.available_agents, "current": current }),
    )
}

async fn switch_agent(name: &str, ctx: &CommandContext<'_>) -> CommandResult {
    let (index, agent_info) = match ctx.available_agents.iter().enumerate().find(|(_, a)| a.name == name) {
        Some((i, a)) => (i, a),
        None => return CommandResult::error(format!("Unknown agent: {}", name)),
    };

    let agent_config = match ctx.agent_configs.iter().find(|c| c.name() == name) {
        Some(c) => c,
        None => return CommandResult::error(format!("Agent config not found: {}", name)),
    };

    if let Err(e) = ctx
        .agent
        .swap_agent(SwapAgentArgs {
            agent_config: agent_config.config().clone(),
            local_mcp_path: ctx.local_mcp_path.cloned(),
            global_mcp_path: ctx.global_mcp_path.cloned(),
        })
        .await
    {
        return CommandResult::error(format!("Failed to switch agent: {}", e));
    }

    CommandResult::success_with_data(
        format!("Agent changed to {}", name),
        serde_json::json!({ "agent": { "name": agent_info.name.clone(), "index": index } }),
    )
}
