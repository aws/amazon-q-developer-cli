//! /agent command execution

pub mod create;
pub mod edit;

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

/// Parsed agent subcommand
enum AgentSubcommand<'a> {
    List,
    Create(&'a str),
    Edit(&'a str),
    Switch(&'a str),
}

/// Checks if the input starts with a subcommand keyword as a whole word
/// (exact match or followed by whitespace). This avoids matching agent names
/// that happen to start with "create" or "edit" (e.g. "create-helper", "editor").
fn strip_subcommand<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    if input == keyword {
        return Some("");
    }
    if let Some(rest) = input.strip_prefix(keyword)
        && rest.starts_with(char::is_whitespace)
    {
        return Some(rest.trim());
    }
    None
}

fn parse_subcommand(args: &AgentArgs) -> AgentSubcommand<'_> {
    let Some(input) = args.agent_name.as_deref() else {
        return AgentSubcommand::List;
    };
    let trimmed = input.trim();
    // "swap" is an explicit switch — bypasses subcommand parsing
    if let Some(rest) = strip_subcommand(trimmed, "swap")
        && !rest.is_empty()
    {
        return AgentSubcommand::Switch(rest);
    }
    if let Some(rest) = strip_subcommand(trimmed, "create") {
        return AgentSubcommand::Create(rest);
    }
    if let Some(rest) = strip_subcommand(trimmed, "edit") {
        return AgentSubcommand::Edit(rest);
    }
    AgentSubcommand::Switch(trimmed)
}

pub async fn execute(args: &AgentArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match parse_subcommand(args) {
        AgentSubcommand::List => list_agents(ctx),
        AgentSubcommand::Create(rest) => {
            let create_args = create::parse_args(rest);
            create::execute(&create_args, ctx).await
        },
        AgentSubcommand::Edit(rest) => {
            let edit_args = edit::AgentEditArgs {
                name: (!rest.is_empty()).then(|| rest.to_string()),
            };
            edit::execute(&edit_args, ctx).await
        },
        AgentSubcommand::Switch(name) => switch_agent(name, ctx).await,
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
        hint: None,
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
    if let Some((index, agent_info)) = ctx.available_agents.iter().enumerate().find(|(_, a)| a.name == name) {
        return do_switch_agent(index, agent_info, ctx).await;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(name: Option<&str>) -> AgentArgs {
        AgentArgs {
            agent_name: name.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_parse_subcommand_list() {
        assert!(matches!(parse_subcommand(&args(None)), AgentSubcommand::List));
    }

    #[test]
    fn test_parse_subcommand_switch() {
        assert!(matches!(
            parse_subcommand(&args(Some("my-agent"))),
            AgentSubcommand::Switch("my-agent")
        ));
    }

    #[test]
    fn test_parse_subcommand_create() {
        assert!(matches!(
            parse_subcommand(&args(Some("create myagent"))),
            AgentSubcommand::Create("myagent")
        ));
    }

    #[test]
    fn test_parse_subcommand_create_no_args() {
        assert!(matches!(
            parse_subcommand(&args(Some("create"))),
            AgentSubcommand::Create("")
        ));
    }

    #[test]
    fn test_parse_subcommand_edit() {
        assert!(matches!(
            parse_subcommand(&args(Some("edit myagent"))),
            AgentSubcommand::Edit("myagent")
        ));
    }

    #[test]
    fn test_parse_subcommand_edit_no_args() {
        assert!(matches!(
            parse_subcommand(&args(Some("edit"))),
            AgentSubcommand::Edit("")
        ));
    }

    #[test]
    fn test_agent_named_create_prefix_switches_not_creates() {
        assert!(matches!(
            parse_subcommand(&args(Some("create-helper"))),
            AgentSubcommand::Switch("create-helper")
        ));
    }

    #[test]
    fn test_agent_named_editor_switches_not_edits() {
        assert!(matches!(
            parse_subcommand(&args(Some("editor"))),
            AgentSubcommand::Switch("editor")
        ));
    }

    #[test]
    fn test_agent_named_creative_switches() {
        assert!(matches!(
            parse_subcommand(&args(Some("creative"))),
            AgentSubcommand::Switch("creative")
        ));
    }

    #[test]
    fn test_swap_subcommand() {
        // "/agent swap create" should switch to agent named "create", not run create subcommand
        assert!(matches!(
            parse_subcommand(&args(Some("swap create"))),
            AgentSubcommand::Switch("create")
        ));
    }

    #[test]
    fn test_swap_subcommand_edit_agent() {
        assert!(matches!(
            parse_subcommand(&args(Some("swap edit"))),
            AgentSubcommand::Switch("edit")
        ));
    }

    #[test]
    fn test_swap_no_name_falls_through() {
        // "/agent swap" with no name — falls through to Switch("swap")
        // which will fail with "agent not found" (expected behavior)
        assert!(matches!(
            parse_subcommand(&args(Some("swap"))),
            AgentSubcommand::Switch("swap")
        ));
    }
}
