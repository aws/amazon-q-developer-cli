//! /tools command execution — lists available tools with source and description

use agent::tui_commands::CommandResult;
use serde_json::json;
use tracing::warn;

use super::CommandContext;

pub async fn execute(ctx: &CommandContext<'_>) -> CommandResult {
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
