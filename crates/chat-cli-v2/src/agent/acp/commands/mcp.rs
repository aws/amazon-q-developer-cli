//! /mcp command execution — lists configured MCP servers with status and tool count

use agent::tui_commands::{
    CommandResult,
    McpArgs,
};
use serde_json::json;
use tracing::warn;

use super::CommandContext;

pub async fn execute(ctx: &CommandContext<'_>, args: &McpArgs) -> CommandResult {
    let sub = args.subcommand.as_deref().unwrap_or("").trim();
    if sub.is_empty() {
        return execute_status(ctx).await;
    }
    // Parse "list", "add", "add <name>", "add <n1>,<n2>,...", "remove", "remove <name>"
    if let Some(rest) = sub.strip_prefix("add") {
        let name = rest.trim();
        if name.is_empty() {
            return execute_registry_list(ctx, "add").await;
        }
        let names: Vec<&str> = name.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        return execute_add(ctx, &names).await;
    }
    if let Some(rest) = sub.strip_prefix("remove") {
        let name = rest.trim();
        if name.is_empty() {
            return execute_registry_list(ctx, "remove").await;
        }
        let names: Vec<&str> = name.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        return execute_remove(ctx, &names).await;
    }
    if sub == "list" {
        return execute_registry_list(ctx, "list").await;
    }
    CommandResult::error(format!(
        "Unknown subcommand: {sub}. Try /mcp, /mcp list, /mcp add, or /mcp remove"
    ))
}

/// `/mcp` — show configured servers with status
async fn execute_status(ctx: &CommandContext<'_>) -> CommandResult {
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

    CommandResult::success_with_data(&message, json!({ "servers": servers_json, "message": message }))
}

/// `/mcp list`, `/mcp add` (no name), `/mcp remove` (no name) — show registry servers
async fn execute_registry_list(ctx: &CommandContext<'_>, mode: &str) -> CommandResult {
    let registry = ctx.session_tx.get_registry_data().await;

    let Some(registry) = registry else {
        return CommandResult::success_with_data(
            "No MCP registry configured",
            json!({ "servers": [], "message": "No MCP registry configured", "mode": mode }),
        );
    };

    if registry.servers.is_empty() {
        return CommandResult::success_with_data(
            "MCP registry is empty (fetch may have failed)",
            json!({ "servers": [], "message": "MCP registry is empty (fetch may have failed)", "mode": mode }),
        );
    }

    let configured = ctx
        .agent
        .get_mcp_server_info()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.name)
        .collect::<std::collections::HashSet<_>>();

    let servers_json: Vec<serde_json::Value> = registry
        .servers
        .iter()
        .map(|entry| {
            let s = &entry.server;
            json!({
                "name": s.name,
                "version": s.version,
                "description": s.description,
                "enabled": configured.contains(&s.name),
            })
        })
        .collect();

    let enabled_count = servers_json.iter().filter(|s| s["enabled"] == true).count();
    let message = format!(
        "{} registry server{} ({} enabled)",
        registry.servers.len(),
        if registry.servers.len() == 1 { "" } else { "s" },
        enabled_count,
    );

    CommandResult::success_with_data(
        &message,
        json!({ "servers": servers_json, "message": message, "mode": mode }),
    )
}

/// `/mcp add <name>[,<name>...]` — add registry servers to the agent config (single swap)
async fn execute_add(ctx: &CommandContext<'_>, server_names: &[&str]) -> CommandResult {
    let registry = ctx.session_tx.get_registry_data().await;
    let Some(registry) = registry else {
        return CommandResult::error("No MCP registry configured");
    };
    for name in server_names {
        if !registry.servers.iter().any(|e| e.server.name == *name) {
            return CommandResult::error(format!("Server '{name}' not found in registry"));
        }
    }

    let snapshot = match ctx.agent.create_snapshot().await {
        Ok(s) => s,
        Err(e) => return CommandResult::error(format!("Failed to get agent state: {e}")),
    };
    let mut config = snapshot.agent_config;

    let mut tools = config.tools();
    for name in server_names {
        let pattern = format!("@{name}/*");
        if !tools.contains(&pattern) {
            tools.push(pattern);
        }
    }
    config.config_mut().set_tools(tools);

    crate::mcp_registry::resolve_registry_servers_for_agent_config(&mut config, &registry);
    crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, &registry);

    if let Err(e) = ctx
        .agent
        .swap_agent(agent::protocol::SwapAgentArgs {
            agent_config: config,
            local_mcp_path: ctx.local_mcp_path.cloned(),
            global_mcp_path: ctx.global_mcp_path.cloned(),
            force: true,
        })
        .await
    {
        return CommandResult::error(format!("Failed to add servers: {e}"));
    }

    let label = server_names.join(", ");
    CommandResult::success(format!("✓ Added {label}"))
}

/// `/mcp remove <name>[,<name>...]` — remove servers from the agent config (single swap)
async fn execute_remove(ctx: &CommandContext<'_>, server_names: &[&str]) -> CommandResult {
    let registry = ctx.session_tx.get_registry_data().await;
    let Some(registry) = registry else {
        return CommandResult::error("No MCP registry configured");
    };

    let snapshot = match ctx.agent.create_snapshot().await {
        Ok(s) => s,
        Err(e) => return CommandResult::error(format!("Failed to get agent state: {e}")),
    };
    let mut config = snapshot.agent_config;

    let remove_set: std::collections::HashSet<&str> = server_names.iter().copied().collect();
    let tools: Vec<String> = config
        .tools()
        .into_iter()
        .filter(|t| {
            !remove_set
                .iter()
                .any(|name| t.starts_with(&format!("@{name}/")) || t == &format!("@{name}"))
        })
        .collect();
    config.config_mut().set_tools(tools);
    config
        .config_mut()
        .retain_mcp_servers(|name| !remove_set.contains(name));

    crate::mcp_registry::filter_agent_config_tools_by_registry(&mut config, &registry);

    if let Err(e) = ctx
        .agent
        .swap_agent(agent::protocol::SwapAgentArgs {
            agent_config: config,
            local_mcp_path: ctx.local_mcp_path.cloned(),
            global_mcp_path: ctx.global_mcp_path.cloned(),
            force: true,
        })
        .await
    {
        return CommandResult::error(format!("Failed to remove servers: {e}"));
    }

    let label = server_names.join(", ");
    CommandResult::success(format!("✓ Removed {label}"))
}
