//! /mcp command execution — lists configured MCP servers with status and tool count

use agent::tui_commands::CommandResult;
use serde_json::json;
use tracing::warn;

use super::CommandContext;

pub async fn execute(ctx: &CommandContext<'_>) -> CommandResult {
    let servers = match ctx.agent.get_mcp_server_info().await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "/mcp: failed to get server info");
            return CommandResult::error(format!("Failed to get MCP server info: {}", e));
        },
    };

    let message = if servers.is_empty() {
        "No MCP servers configured".to_string()
    } else {
        format!(
            "{} MCP server{} configured",
            servers.len(),
            if servers.len() == 1 { "" } else { "s" }
        )
    };

    let servers_json: Vec<serde_json::Value> = servers
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "status": s.status,
                "toolCount": s.tool_count,
            })
        })
        .collect();

    let data = json!({ "servers": servers_json, "message": message });
    CommandResult::success_with_data(&message, data)
}
