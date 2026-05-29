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
        return execute_list(ctx).await;
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

/// `/mcp list` — show both configured servers and registry servers
async fn execute_list(ctx: &CommandContext<'_>) -> CommandResult {
    let configured = ctx.agent.get_mcp_server_info().await.unwrap_or_default();
    let configured_json: Vec<serde_json::Value> = configured
        .iter()
        .map(|s| {
            json!({
                "name": s.name,
                "status": s.status,
                "toolCount": s.tool_count,
            })
        })
        .collect();

    let registry = ctx.session_tx.get_registry_data().await;
    let registry_json: Vec<serde_json::Value> = registry
        .as_ref()
        .map(|r| {
            let configured_names: std::collections::HashSet<_> = configured.iter().map(|s| s.name.as_str()).collect();
            r.servers
                .iter()
                .map(|entry| {
                    let s = &entry.server;
                    json!({
                        "name": s.name,
                        "version": s.version,
                        "description": s.description,
                        "enabled": configured_names.contains(s.name.as_str()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let message = format!(
        "{} configured server{}, {} registry server{}",
        configured_json.len(),
        if configured_json.len() == 1 { "" } else { "s" },
        registry_json.len(),
        if registry_json.len() == 1 { "" } else { "s" },
    );

    CommandResult::success_with_data(
        &message,
        json!({
            "servers": configured_json,
            "registryServers": registry_json,
            "message": message,
            "mode": "list",
        }),
    )
}

/// `/mcp add` (no name), `/mcp remove` (no name) — show registry servers
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

/// Persist MCP server changes to the agent's source file on disk.
///
/// Persist MCP server changes to the agent's source file on disk.
///
/// Re-reads the original file into a typed `AgentConfig`, applies the changes,
/// and serializes back. This preserves `"type": "registry"` entries since
/// `RegistryMcpServerConfig` is part of the schema.
async fn persist_mcp_changes(
    ctx: &CommandContext<'_>,
    source: &agent::agent_config::ConfigSource,
    added_tools: &[String],
    removed_servers: &[&str],
) {
    let path = match source {
        agent::agent_config::ConfigSource::Workspace { path } | agent::agent_config::ConfigSource::Global { path } => {
            path
        },
        _ => {
            tracing::debug!("/mcp: agent has no file path, skipping disk persistence");
            return;
        },
    };

    // Read and parse into the typed schema
    let contents = match ctx.os.fs.read_to_string(path).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("/mcp: failed to read agent config from {}: {e}", path.display());
            return;
        },
    };

    let mut config: agent::agent_config::definitions::AgentConfig = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("/mcp: failed to parse agent config: {e}");
            return;
        },
    };

    // Add new tool patterns
    let mut tools = config.tools();
    for pattern in added_tools {
        if !tools.contains(pattern) {
            tools.push(pattern.clone());
        }
    }

    // Remove servers
    if !removed_servers.is_empty() {
        let remove_set: std::collections::HashSet<&str> = removed_servers.iter().copied().collect();
        tools.retain(|t| {
            !remove_set
                .iter()
                .any(|name| t.starts_with(&format!("@{name}/")) || t == &format!("@{name}"))
        });
        config.retain_mcp_servers(|name| !remove_set.contains(name));
    }

    config.set_tools(tools);

    let output = match serde_json::to_string_pretty(&config) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("/mcp: failed to serialize agent config: {e}");
            return;
        },
    };

    if let Err(e) = ctx.os.fs.write(path, output).await {
        tracing::warn!("/mcp: failed to write agent config to {}: {e}", path.display());
    } else {
        tracing::debug!("/mcp: persisted MCP changes to {}", path.display());
    }
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

    let added_tools: Vec<String> = server_names.iter().map(|name| format!("@{name}/*")).collect();

    let mut tools = config.tools();
    for pattern in &added_tools {
        if !tools.contains(pattern) {
            tools.push(pattern.clone());
        }
    }
    config.config_mut().set_tools(tools);

    // Persist to disk (patches original JSON to preserve registry entries)
    persist_mcp_changes(ctx, config.source(), &added_tools, &[]).await;

    // The agent re-applies its stored MCP registry inside `handle_swap_agent`,
    // which resolves the newly referenced registry servers and (re)filters tools.
    // The host no longer pre-rewrites here.
    if let Err(e) = ctx
        .agent
        .swap_agent(agent::protocol::SwapAgentArgs {
            agent_config: config,
            force: true,
            knowledge_provider: None,
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
    if ctx.session_tx.get_registry_data().await.is_none() {
        return CommandResult::error("No MCP registry configured");
    }

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

    // Persist to disk (patches original JSON to preserve registry entries)
    persist_mcp_changes(ctx, config.source(), &[], server_names).await;

    // The agent re-applies its stored MCP registry inside `handle_swap_agent`,
    // re-filtering tools and `use_legacy_mcp_json` against the registry; the
    // host no longer pre-rewrites here.
    if let Err(e) = ctx
        .agent
        .swap_agent(agent::protocol::SwapAgentArgs {
            agent_config: config,
            force: true,
            knowledge_provider: None,
        })
        .await
    {
        return CommandResult::error(format!("Failed to remove servers: {e}"));
    }

    let label = server_names.join(", ");
    CommandResult::success(format!("✓ Removed {label}"))
}

#[cfg(test)]
mod tests {
    use agent::agent_config::definitions::{
        AgentConfig,
        AgentConfigV2025_08_22,
        McpServerConfig,
        RegistryMcpServerConfig,
    };

    /// Helper: round-trip an AgentConfig through JSON (simulates persist + reload)
    fn roundtrip(config: &AgentConfig) -> AgentConfig {
        let json = serde_json::to_string_pretty(config).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn test_persist_add_tools() {
        let mut config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["fs_read".to_string(), "@existing-server/*".to_string()],
            ..Default::default()
        });

        // Simulate /mcp add
        let mut tools = config.tools().to_vec();
        tools.push("@new-server/*".to_string());
        config.set_tools(tools);

        let restored = roundtrip(&config);
        let tools = restored.tools();
        assert_eq!(tools.len(), 3);
        assert!(tools.contains(&"@new-server/*".to_string()));
        assert!(tools.contains(&"fs_read".to_string()));
        assert!(tools.contains(&"@existing-server/*".to_string()));
    }

    #[test]
    fn test_persist_remove_servers() {
        let mut mcp_servers = std::collections::HashMap::new();
        mcp_servers.insert(
            "server-a".to_string(),
            McpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: Some([("FOO".to_string(), "bar".to_string())].into()),
                headers: None,
                timeout: None,
                oauth_scopes: Vec::new(),
                oauth: None,
            }),
        );
        mcp_servers.insert(
            "server-b".to_string(),
            McpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: None,
                timeout: None,
                oauth_scopes: Vec::new(),
                oauth: None,
            }),
        );

        let mut config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec![
                "fs_read".to_string(),
                "@server-a/*".to_string(),
                "@server-b/*".to_string(),
            ],
            mcp_servers,
            ..Default::default()
        });

        // Simulate /mcp remove server-a
        let tools: Vec<String> = config
            .tools()
            .iter()
            .filter(|t| !t.starts_with("@server-a/") && *t != "@server-a")
            .cloned()
            .collect();
        config.set_tools(tools);
        config.retain_mcp_servers(|name| name != "server-a");

        let restored = roundtrip(&config);
        let tools = restored.tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"fs_read".to_string()));
        assert!(tools.contains(&"@server-b/*".to_string()));
        assert!(!tools.iter().any(|t| t.contains("server-a")));

        // server-b preserved, server-a gone
        assert!(restored.mcp_servers().contains_key("server-b"));
        assert!(!restored.mcp_servers().contains_key("server-a"));
    }

    #[test]
    fn test_persist_preserves_registry_env() {
        let mut mcp_servers = std::collections::HashMap::new();
        mcp_servers.insert(
            "existing".to_string(),
            McpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: Some([("SECRET".to_string(), "keep-me".to_string())].into()),
                headers: Some([("Auth".to_string(), "Bearer tok".to_string())].into()),
                timeout: Some(45000),
                oauth_scopes: Vec::new(),
                oauth: None,
            }),
        );

        let mut config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@existing/*".to_string()],
            mcp_servers,
            ..Default::default()
        });

        // Simulate /mcp add new-server (should not touch existing)
        let mut tools = config.tools().to_vec();
        tools.push("@new-server/*".to_string());
        config.set_tools(tools);

        let restored = roundtrip(&config);

        // Existing registry entry with env/headers/timeout preserved
        let existing = restored.mcp_servers().get("existing").unwrap();
        let reg = existing.registry_overrides().unwrap();
        assert_eq!(reg.env.as_ref().unwrap().get("SECRET").unwrap(), "keep-me");
        assert_eq!(reg.headers.as_ref().unwrap().get("Auth").unwrap(), "Bearer tok");
        assert_eq!(reg.timeout, Some(45000));
    }
}
