//! /tools command execution — lists available tools or manages trust settings

use agent::tui_commands::{
    CommandResult,
    ToolsArgs,
};
use serde_json::json;
use tracing::warn;

use super::CommandContext;

pub async fn execute(args: &ToolsArgs, ctx: &CommandContext<'_>) -> CommandResult {
    match args.subcommand.as_deref() {
        Some("trust-all") => trust_all(ctx).await,
        Some("reset") => reset(ctx).await,
        Some(sub) if sub.starts_with("trust ") => {
            let names = parse_tool_names(&sub["trust ".len()..]);
            trust(ctx, names).await
        },
        Some(sub) if sub.starts_with("untrust ") => {
            let names = parse_tool_names(&sub["untrust ".len()..]);
            untrust(ctx, names).await
        },
        Some("trust") => CommandResult::error("Usage: /tools trust <tool_name> [tool_name...]"),
        Some("untrust") => CommandResult::error("Usage: /tools untrust <tool_name> [tool_name...]"),
        Some(other) => CommandResult::error(format!(
            "Unknown /tools subcommand: {other}. Use trust-all, reset, trust, or untrust."
        )),
        None => list_tools(ctx).await,
    }
}

fn parse_tool_names(args: &str) -> Vec<String> {
    args.split_whitespace().map(String::from).collect()
}

async fn trust(ctx: &CommandContext<'_>, names: Vec<String>) -> CommandResult {
    match ctx.agent.trust_tools(names).await {
        Ok((changed, invalid)) => format_trust_result(&changed, &invalid, true),
        Err(e) => {
            warn!(error = %e, "/tools trust: failed");
            CommandResult::error(format!("Failed to trust tools: {}", e))
        },
    }
}

async fn untrust(ctx: &CommandContext<'_>, names: Vec<String>) -> CommandResult {
    match ctx.agent.untrust_tools(names).await {
        Ok((changed, invalid)) => format_trust_result(&changed, &invalid, false),
        Err(e) => {
            warn!(error = %e, "/tools untrust: failed");
            CommandResult::error(format!("Failed to untrust tools: {}", e))
        },
    }
}

fn format_trust_result(changed: &[String], invalid: &[String], trusted: bool) -> CommandResult {
    let mut parts = Vec::new();
    if !changed.is_empty() {
        let verb = if trusted {
            "now trusted"
        } else {
            "set to per-request confirmation"
        };
        parts.push(format!("{} {}", changed.join(", "), verb));
    }
    if !invalid.is_empty() {
        parts.push(format!("not found: {}", invalid.join(", ")));
    }
    if parts.is_empty() {
        return CommandResult::error("No tool names provided");
    }
    let message = parts.join(". ");
    if changed.is_empty() {
        CommandResult::error(message)
    } else {
        CommandResult::success(message)
    }
}

async fn trust_all(ctx: &CommandContext<'_>) -> CommandResult {
    if let Err(e) = ctx.agent.trust_all_tools().await {
        warn!(error = %e, "/tools trust-all: failed to set trust-all");
        return CommandResult::error(format!("Failed to enable trust-all: {}", e));
    }
    CommandResult::success("All tools are now trusted for this session. Tools will run without approval prompts.")
}

async fn reset(ctx: &CommandContext<'_>) -> CommandResult {
    if let Err(e) = ctx.agent.reset_tool_permissions().await {
        warn!(error = %e, "/tools reset: failed to reset trust");
        return CommandResult::error(format!("Failed to reset tool trust: {}", e));
    }
    CommandResult::success("Tool trust has been reset to default permission levels.")
}

async fn list_tools(ctx: &CommandContext<'_>) -> CommandResult {
    let tools = match ctx.agent.get_tool_info().await {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "/tools: failed to get tool info");
            return CommandResult::error(format!("Failed to get tool info: {}", e));
        },
    };

    let message = if tools.is_empty() {
        "No tools available".to_string()
    } else {
        format!(
            "{} tool{} available",
            tools.len(),
            if tools.len() == 1 { "" } else { "s" }
        )
    };

    let tools_json: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "source": t.source,
                "description": t.description,
                "status": match &t.status {
                    agent::tui_commands::ToolStatus::Allowed => "allowed",
                    agent::tui_commands::ToolStatus::RequiresApproval => "requires-approval",
                    agent::tui_commands::ToolStatus::Denied => "denied",
                },
            })
        })
        .collect();

    let data = json!({ "tools": tools_json, "message": message });
    CommandResult::success_with_data(&message, data)
}
