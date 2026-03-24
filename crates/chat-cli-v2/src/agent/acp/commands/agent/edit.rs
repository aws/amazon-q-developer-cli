//! /agent edit subcommand execution

use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
};
use agent::tui_commands::CommandResult;

use super::super::CommandContext;

/// Arguments for /agent edit subcommand
#[derive(Debug, Clone, Default)]
pub struct AgentEditArgs {
    pub name: Option<String>,
}

/// Core edit logic, testable without CommandContext.
pub fn resolve_edit(
    args: &AgentEditArgs,
    agent_configs: &[LoadedAgentConfig],
    current_agent_name: &str,
) -> CommandResult {
    let name = match &args.name {
        Some(n) => n.clone(),
        None => current_agent_name.to_string(),
    };

    let config = match agent_configs.iter().find(|c| c.name() == name) {
        Some(c) => c,
        None => return CommandResult::error(format!("Agent '{}' not found", name)),
    };

    let path = match config.source() {
        ConfigSource::Workspace { path } | ConfigSource::Global { path } => path.clone(),
        ConfigSource::BuiltIn => {
            return CommandResult::error(format!(
                "Cannot edit built-in agent '{}'. Create a new agent with '/agent create'",
                name
            ));
        },
        ConfigSource::Ephemeral => {
            return CommandResult::error(format!("Agent '{}' has no config file on disk", name));
        },
    };

    CommandResult::success_with_data(
        format!("Edited agent '{}' at {}", name, path.display()),
        serde_json::json!({
            "name": name,
            "path": path.to_string_lossy(),
        }),
    )
}

/// Entry point called from the agent command handler.
pub async fn execute(args: &AgentEditArgs, ctx: &CommandContext<'_>) -> CommandResult {
    resolve_edit(args, ctx.agent_configs, ctx.current_agent_name)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use agent::agent_config::ResolvedGlobalPrompt;
    use agent::agent_config::definitions::AgentConfig;

    use super::*;

    fn make_config(name: &str, source: ConfigSource) -> LoadedAgentConfig {
        let mut config = AgentConfig::default();
        match &mut config {
            AgentConfig::V2025_08_22(c) => c.name = name.to_string(),
        }
        LoadedAgentConfig::new(config, source, ResolvedGlobalPrompt::None)
    }

    #[test]
    fn test_edit_by_name() {
        let configs = vec![make_config("my-agent", ConfigSource::Global {
            path: PathBuf::from("/home/user/.kiro/agents/my-agent.json"),
        })];
        let args = AgentEditArgs {
            name: Some("my-agent".to_string()),
        };
        let result = resolve_edit(&args, &configs, "kiro_default");
        assert!(result.success);
        assert!(result.message.contains("my-agent"));
        let data = result.data.unwrap();
        assert_eq!(data["path"], "/home/user/.kiro/agents/my-agent.json");
    }

    #[test]
    fn test_edit_defaults_to_current_agent() {
        let configs = vec![make_config("current", ConfigSource::Workspace {
            path: PathBuf::from("/ws/.kiro/agents/current.json"),
        })];
        let args = AgentEditArgs { name: None };
        let result = resolve_edit(&args, &configs, "current");
        assert!(result.success);
        assert!(result.message.contains("current"));
    }

    #[test]
    fn test_edit_not_found() {
        let result = resolve_edit(
            &AgentEditArgs {
                name: Some("nonexistent".to_string()),
            },
            &[],
            "kiro_default",
        );
        assert!(!result.success);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_edit_builtin_rejected() {
        let configs = vec![make_config("kiro_default", ConfigSource::BuiltIn)];
        let args = AgentEditArgs {
            name: Some("kiro_default".to_string()),
        };
        let result = resolve_edit(&args, &configs, "kiro_default");
        assert!(!result.success);
        assert!(result.message.contains("Cannot edit built-in"));
    }

    #[test]
    fn test_edit_ephemeral_rejected() {
        let configs = vec![make_config("temp", ConfigSource::Ephemeral)];
        let args = AgentEditArgs {
            name: Some("temp".to_string()),
        };
        let result = resolve_edit(&args, &configs, "kiro_default");
        assert!(!result.success);
        assert!(result.message.contains("no config file"));
    }
}
