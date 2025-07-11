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
use crate::cli::agent::{
    CreateHooks,
    PromptHooks,
};
use crate::cli::chat::cli::hooks::{
    Hook,
    HookTrigger,
};
use crate::cli::chat::context::ContextConfig;
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories;

pub(in crate::cli::agent) struct ContextMigrate<const S: char> {
    legacy_global_context: Option<ContextConfig>,
    legacy_profiles: HashMap<String, ContextConfig>,
    mcp_servers: Option<McpServerConfig>,
    new_agents: Vec<Agent>,
}

impl ContextMigrate<'a'> {
    pub async fn scan(os: &Os) -> eyre::Result<ContextMigrate<'b'>> {
        let legacy_global_context_path = directories::chat_global_context_path(os)?;
        let legacy_global_context: Option<ContextConfig> = 'global: {
            let Ok(content) = os.fs.read(&legacy_global_context_path).await else {
                break 'global None;
            };
            serde_json::from_slice::<ContextConfig>(&content).ok()
        };

        let legacy_profile_path = directories::chat_profiles_dir(os)?;
        let legacy_profiles: HashMap<String, ContextConfig> = 'profiles: {
            let mut profiles = HashMap::<String, ContextConfig>::new();
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
                let Ok(mut context_config) = serde_json::from_str::<ContextConfig>(content.as_str()) else {
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
                    Ok(config) => Some(config),
                    Err(e) => {
                        error!("Malformed legacy global mcp config detected: {e}. Skipping mcp migration.");
                        None
                    },
                }
            } else {
                None
            }
        };

        if legacy_global_context.is_some() || !legacy_profiles.is_empty() {
            Ok(ContextMigrate {
                legacy_global_context,
                legacy_profiles,
                mcp_servers,
                new_agents: vec![],
            })
        } else {
            bail!("Nothing to migrate");
        }
    }
}

impl ContextMigrate<'b'> {
    pub async fn prompt_migrate(self) -> eyre::Result<ContextMigrate<'c'>> {
        let ContextMigrate {
            legacy_global_context,
            legacy_profiles,
            mcp_servers,
            new_agents,
        } = self;

        let labels = vec!["Yes", "No"];
        let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
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
        };

        if let Some(0) = selection {
            Ok(ContextMigrate {
                legacy_global_context,
                legacy_profiles,
                mcp_servers,
                new_agents,
            })
        } else {
            bail!("Aborting migration")
        }
    }
}

impl ContextMigrate<'c'> {
    pub async fn migrate(self, os: &Os) -> eyre::Result<ContextMigrate<'d'>> {
        const LEGACY_GLOBAL_AGENT_NAME: &str = "migrated_agent_from_global_context";
        const DEFAULT_DESC: &str = "This is an agent migrated from global context";
        const PROFILE_DESC: &str = "This is an agent migrated from profile context";

        let ContextMigrate {
            legacy_global_context,
            mut legacy_profiles,
            mcp_servers,
            mut new_agents,
        } = self;

        let has_global_context = legacy_global_context.is_some();

        // Migration of global context
        if let Some(context) = legacy_global_context {
            let (create_hooks, prompt_hooks) =
                context
                    .hooks
                    .into_iter()
                    .partition::<HashMap<String, Hook>, _>(|(_, hook)| {
                        matches!(hook.trigger, HookTrigger::ConversationStart)
                    });

            new_agents.push(Agent {
                name: LEGACY_GLOBAL_AGENT_NAME.to_string(),
                description: Some(DEFAULT_DESC.to_string()),
                path: Some(directories::chat_global_agent_path(os)?.join(format!("{LEGACY_GLOBAL_AGENT_NAME}.json"))),
                included_files: context.paths,
                create_hooks: CreateHooks::Map(create_hooks),
                prompt_hooks: PromptHooks::Map(prompt_hooks),
                mcp_servers: mcp_servers.clone().unwrap_or_default(),
                ..Default::default()
            });
        }

        let global_agent_path = directories::chat_global_agent_path(os)?;

        // Migration of profile context
        for (profile_name, context) in legacy_profiles.drain() {
            let (create_hooks, prompt_hooks) =
                context
                    .hooks
                    .into_iter()
                    .partition::<HashMap<String, Hook>, _>(|(_, hook)| {
                        matches!(hook.trigger, HookTrigger::ConversationStart)
                    });

            new_agents.push(Agent {
                path: Some(global_agent_path.join(format!("{profile_name}.json"))),
                name: profile_name,
                description: Some(PROFILE_DESC.to_string()),
                included_files: context.paths,
                create_hooks: CreateHooks::Map(create_hooks),
                prompt_hooks: PromptHooks::Map(prompt_hooks),
                mcp_servers: mcp_servers.clone().unwrap_or_default(),
                ..Default::default()
            });
        }

        if !os.fs.exists(&global_agent_path) {
            os.fs.create_dir_all(&global_agent_path).await?;
        }

        let formatted_server_list = mcp_servers
            .map(|config| {
                config
                    .mcp_servers
                    .keys()
                    .map(|server_name| format!("@{server_name}"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        for agent in &mut new_agents {
            agent.tools.extend(formatted_server_list.clone());

            let content = serde_json::to_string_pretty(agent)?;
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

        let legacy_profile_config_path = directories::chat_profiles_dir(os)?;
        let profile_backup_path = legacy_profile_config_path
            .parent()
            .ok_or(eyre::eyre!("Failed to obtain profile config parent path"))?
            .join("profiles.bak");
        os.fs.rename(legacy_profile_config_path, profile_backup_path).await?;

        if has_global_context {
            let legacy_global_config_path = directories::chat_global_context_path(os)?;
            let legacy_global_config_file_name = legacy_global_config_path
                .file_name()
                .ok_or(eyre::eyre!("Failed to obtain legacy global config name"))?
                .to_string_lossy();
            let global_context_backup_path = legacy_global_config_path
                .parent()
                .ok_or(eyre::eyre!("Failed to obtain parent path for global context"))?
                .join(format!("{}.bak", legacy_global_config_file_name));
            os.fs
                .rename(legacy_global_config_path, global_context_backup_path)
                .await?;
        }

        Ok(ContextMigrate {
            legacy_global_context: None,
            legacy_profiles,
            mcp_servers: None,
            new_agents,
        })
    }
}

impl ContextMigrate<'d'> {
    pub async fn prompt_set_default(self, os: &mut Os) -> eyre::Result<(Option<String>, Vec<Agent>)> {
        let ContextMigrate { new_agents, .. } = self;

        let labels = new_agents
            .iter()
            .map(|a| a.name.as_str())
            .chain(vec!["Let me do this on my own later"])
            .collect::<Vec<_>>();
        // This yields 0 if it's negative, which is acceptable.
        let later_idx = labels.len().saturating_sub(1);
        let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt(
                "Set an agent as default. This is the agent that q chat will launch with unless specified otherwise.",
            )
            .default(0)
            .items(&labels)
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
        };

        let mut agent_to_load = None::<String>;
        if let Some(i) = selection {
            if later_idx != i {
                if let Some(name) = labels.get(i) {
                    if let Ok(value) = serde_json::to_value(name) {
                        if os.database.settings.set(Setting::ChatDefaultAgent, value).await.is_ok() {
                            let chosen_name = (*name).to_string();
                            agent_to_load.replace(chosen_name);
                        }
                    }
                }
            }
        }

        Ok((agent_to_load, new_agents))
    }
}
