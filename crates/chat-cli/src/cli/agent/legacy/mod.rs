pub mod context;
pub mod hooks;

use std::collections::HashMap;

use dialoguer::Select;
use eyre::bail;
use tracing::{
    error,
    info,
    warn,
};

use super::{
    Agent,
    McpServerConfig,
};
use crate::cli::agent::hook::Hook;
use crate::cli::agent::legacy::context::LegacyContextConfig;
use crate::os::Os;
use crate::util::directories;

/// Performs the migration from legacy profile configuration to agent configuration if it hasn't
/// already been done.
///
/// Returns [Some] with the newly migrated agents if the migration was performed, [None] if the
/// migration was already done previously.
pub async fn migrate(os: &mut Os, force: bool) -> eyre::Result<Option<Vec<Agent>>> {
    let has_migrated = os.database.get_has_migrated()?;
    if !force && has_migrated.is_some_and(|has_migrated| has_migrated) {
        return Ok(None);
    }

    let legacy_global_context_path = directories::chat_global_context_path(os)?;
    let legacy_global_context: Option<LegacyContextConfig> = 'global: {
        let Ok(content) = os.fs.read(&legacy_global_context_path).await else {
            break 'global None;
        };
        serde_json::from_slice::<LegacyContextConfig>(&content).ok()
    };

    let legacy_profile_path = directories::chat_profiles_dir(os)?;
    let mut legacy_profiles: HashMap<String, LegacyContextConfig> = 'profiles: {
        let mut profiles = HashMap::<String, LegacyContextConfig>::new();
        let Ok(mut read_dir) = os.fs.read_dir(&legacy_profile_path).await else {
            break 'profiles profiles;
        };

        // Here we assume every profile is stored under their own folders
        // And that the profile config is in profile_name/context.json
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let config_file_path = entry.path().join("context.json");
            if !os.fs.exists(&config_file_path) {
                continue;
            }
            let Some(profile_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let Ok(content) = tokio::fs::read_to_string(&config_file_path).await else {
                continue;
            };
            let Ok(mut context_config) = serde_json::from_str::<LegacyContextConfig>(content.as_str()) else {
                continue;
            };

            // Combine with global context since you can now only choose one agent at a time
            // So this is how we make what is previously global available to every new agent migrated
            if let Some(context) = legacy_global_context.as_ref() {
                context_config.paths.extend(context.paths.clone());
                context_config.hooks.extend(context.hooks.clone());
            }

            profiles.insert(profile_name.clone(), context_config);
        }

        profiles
    };

    let mcp_servers = {
        let config_path = directories::chat_legacy_mcp_config(os)?;
        if os.fs.exists(&config_path) {
            match McpServerConfig::load_from_file(os, config_path).await {
                Ok(mut config) => {
                    config.mcp_servers.iter_mut().for_each(|(_name, config)| {
                        config.is_from_legacy_mcp_json = true;
                    });
                    Some(config)
                },
                Err(e) => {
                    error!("Malformed legacy global mcp config detected: {e}. Skipping mcp migration.");
                    None
                },
            }
        } else {
            None
        }
    };

    if legacy_global_context.is_none() && legacy_profiles.is_empty() {
        os.database.set_has_migrated()?;
        bail!("Nothing to migrate");
    }

    let labels = vec!["Yes", "No"];
    let selection: Option<_> = if !force {
        match Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt("Legacy profiles detected. Would you like to migrate them?")
            .items(&labels)
            .default(1)
            .interact_on_opt(&dialoguer::console::Term::stdout())
        {
            Ok(sel) => {
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::style::SetForegroundColor(crossterm::style::Color::Magenta)
                );
                sel
            },
            // Ctrl‑C -> Err(Interrupted)
            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => None,
            Err(e) => bail!("Failed to choose an option: {e}"),
        }
    } else {
        // Yes
        Some(0)
    };

    if selection.is_none() || selection == Some(1) {
        bail!("Aborting migration")
    }

    // Migrate
    const LEGACY_GLOBAL_AGENT_NAME: &str = "migrated_agent_from_global_context";
    const DEFAULT_DESC: &str = "This is an agent migrated from global context";
    const PROFILE_DESC: &str = "This is an agent migrated from profile context";

    // Migration of global context
    let mut new_agents = vec![];
    if let Some(context) = legacy_global_context {
        let (create_hooks, prompt_hooks) = context
            .hooks
            .into_iter()
            .partition::<HashMap<String, hooks::LegacyHook>, _>(|(_, hook)| {
                matches!(hook.trigger, hooks::LegacyHookTrigger::ConversationStart)
            });

        new_agents.push(Agent {
            name: LEGACY_GLOBAL_AGENT_NAME.to_string(),
            description: Some(DEFAULT_DESC.to_string()),
            path: Some(directories::chat_global_agent_path(os)?.join(format!("{LEGACY_GLOBAL_AGENT_NAME}.json"))),
            resources: context.paths.iter().map(|p| format!("file://{p}").into()).collect(),
            hooks: HashMap::from([
                (
                    super::HookTrigger::AgentSpawn,
                    create_hooks
                        .into_iter()
                        .filter_map(|(_, hook)| Option::<Hook>::from(hook))
                        .collect(),
                ),
                (
                    super::HookTrigger::UserPromptSubmit,
                    prompt_hooks
                        .into_iter()
                        .filter_map(|(_, hook)| Option::<Hook>::from(hook))
                        .collect(),
                ),
            ]),
            mcp_servers: mcp_servers.clone().unwrap_or_default(),
            ..Default::default()
        });
    }

    let global_agent_path = directories::chat_global_agent_path(os)?;

    // Migration of profile context
    for (profile_name, context) in legacy_profiles.drain() {
        let (create_hooks, prompt_hooks) = context
            .hooks
            .into_iter()
            .partition::<HashMap<String, hooks::LegacyHook>, _>(|(_, hook)| {
                matches!(hook.trigger, hooks::LegacyHookTrigger::ConversationStart)
            });

        new_agents.push(Agent {
            path: Some(global_agent_path.join(format!("{profile_name}.json"))),
            name: profile_name,
            description: Some(PROFILE_DESC.to_string()),
            resources: context.paths.iter().map(|p| format!("file://{p}").into()).collect(),
            hooks: HashMap::from([
                (
                    super::HookTrigger::AgentSpawn,
                    create_hooks
                        .into_iter()
                        .filter_map(|(_, hook)| Option::<Hook>::from(hook))
                        .collect(),
                ),
                (
                    super::HookTrigger::UserPromptSubmit,
                    prompt_hooks
                        .into_iter()
                        .filter_map(|(_, hook)| Option::<Hook>::from(hook))
                        .collect(),
                ),
            ]),
            mcp_servers: mcp_servers.clone().unwrap_or_default(),
            ..Default::default()
        });
    }

    if !os.fs.exists(&global_agent_path) {
        os.fs.create_dir_all(&global_agent_path).await?;
    }

    for agent in &mut new_agents {
        let content = agent.to_str_pretty()?;
        if let Some(path) = agent.path.as_ref() {
            info!("Agent {} peristed in path {}", agent.name, path.to_string_lossy());
            os.fs.write(path, content).await?;
        } else {
            warn!(
                "Agent with name {} does not have path associated and is thus not migrated.",
                agent.name
            );
        }
    }

    os.database.set_has_migrated()?;

    Ok(Some(new_agents))
}
