//! /agent command execution

use agent::protocol::SwapAgentArgs;
use agent::tui_commands::{
    AgentArgs,
    CommandOption,
    CommandOptionsResponse,
    CommandResult,
};
use strsim::jaro_winkler;

use super::CommandContext;
use crate::agent::acp::session_manager::AgentInfo;
/// Minimum similarity score (0.0-1.0) for suggesting an agent name
const AGENT_SIMILARITY_THRESHOLD: f64 = 0.6;

pub async fn execute(args: &AgentArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match &args.agent_name {
        None => list_agents(ctx),
        Some(name) => switch_agent(name, ctx).await,
    }
}

pub fn get_options(_partial: &str, ctx: &CommandContext<'_>) -> CommandOptionsResponse {
    let mut options: Vec<CommandOption> = ctx.available_agents.iter().map(to_agent_option).collect();
    options.sort_by_key(|a| a.label.to_lowercase());
    CommandOptionsResponse {
        options,
        has_more: false,
    }
}

fn to_agent_option(a: &AgentInfo) -> CommandOption {
    CommandOption {
        value: a.name.clone(),
        label: a.name.clone(),
        description: a.description.clone(),
        group: if a.source.is_empty() {
            None
        } else {
            Some(a.source.clone())
        },
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
    // Exact match — switch immediately
    if let Some((index, agent_info)) = ctx.available_agents.iter().enumerate().find(|(_, a)| a.name == name) {
        return do_switch_agent(index, agent_info, ctx).await;
    }

    // Fuzzy match — suggest, don't switch
    if let Some((_, agent_info)) = find_similar_agent(ctx, name) {
        return CommandResult::error(format!(
            "Agent '{}' not found. Did you mean {}? Run /agent to browse available agents.",
            name, agent_info.name
        ));
    }

    CommandResult::error(format!(
        "Unknown agent: {}. Run /agent to browse available agents.",
        name
    ))
}

async fn do_switch_agent(index: usize, agent_info: &AgentInfo, ctx: &CommandContext<'_>) -> CommandResult {
    let agent_config = match ctx.agent_configs.iter().find(|c| c.name() == agent_info.name) {
        Some(c) => c,
        None => return CommandResult::error(format!("Agent config not found: {}", agent_info.name)),
    };

    if let Err(e) = ctx
        .agent
        .swap_agent(SwapAgentArgs {
            agent_config: agent_config.clone(),
            local_mcp_path: ctx.local_mcp_path.cloned(),
            global_mcp_path: ctx.global_mcp_path.cloned(),
        })
        .await
    {
        return CommandResult::error(format!("Failed to switch agent: {}", e));
    }

    CommandResult::success_with_data(
        format!("Agent changed to {}", agent_info.name),
        serde_json::json!({ "agent": { "name": agent_info.name.clone(), "index": index } }),
    )
}

/// Find the closest matching agent using fuzzy string matching.
fn find_similar_agent<'a>(ctx: &'a CommandContext<'_>, query: &str) -> Option<(usize, &'a AgentInfo)> {
    let query_lower = query.to_lowercase();
    ctx.available_agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name_score = jaro_winkler(&query_lower, &a.name.to_lowercase());
            let desc_score = a
                .description
                .as_ref()
                .map_or(0.0, |d| jaro_winkler(&query_lower, &d.to_lowercase()));
            (name_score.max(desc_score), i, a)
        })
        .filter(|(score, _, _)| *score >= AGENT_SIMILARITY_THRESHOLD)
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, i, a)| (i, a))
}
