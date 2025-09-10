use std::collections::HashMap;

use eyre::{
    Result,
    eyre,
};

use crate::cli::chat::tools::delegate::agent_paths::AgentPaths;
use crate::cli::chat::tools::delegate::errors::AgentError;
use crate::cli::chat::tools::delegate::file_ops::{
    load_json,
    save_json,
};
use crate::cli::chat::tools::delegate::types::{
    AgentConfig,
    AgentExecution,
};
use crate::cli::chat::tools::delegate::ui::{
    display_agent_info,
    display_default_agent_warning,
    get_user_confirmation,
};
use crate::os::Os;

const DEFAULT_AGENT: &str = "default";

pub async fn validate_agent_availability(os: &Os, agent: &str) -> Result<()> {
    if let Some(existing) = load_agent_execution(os, agent).await? {
        if existing.is_active() {
            return Err(eyre!("{}", AgentError::already_running(agent)));
        }
    }

    if agent != DEFAULT_AGENT {
        let agents_config = load_available_agents(os).await?;
        if !agents_config.contains_key(agent) {
            let available: Vec<String> = agents_config.keys().cloned().collect();
            return Err(eyre!("{}", AgentError::not_found(agent, &available)));
        }
    }

    Ok(())
}

pub async fn request_user_approval(os: &Os, agent: &str, task: &str) -> Result<()> {
    if agent != DEFAULT_AGENT {
        let agents_config = load_available_agents(os).await?;
        if let Some(agent_config) = agents_config.get(agent) {
            display_agent_info(agent, task, agent_config)?;
            if !get_user_confirmation()? {
                return Err(eyre!("✗ Task delegation cancelled by user."));
            }
        }
    } else {
        display_default_agent_warning()?;
    }
    Ok(())
}

pub async fn load_agent_execution(os: &Os, agent: &str) -> Result<Option<AgentExecution>> {
    let file_path = AgentPaths::agent_file(os, agent).await?;
    load_json(os, &file_path).await
}

pub async fn save_agent_execution(os: &Os, execution: &AgentExecution) -> Result<()> {
    let file_path = AgentPaths::agent_file(os, &execution.agent).await?;
    save_json(os, &file_path, execution).await
}

pub async fn load_available_agents(os: &Os) -> Result<HashMap<String, AgentConfig>> {
    let agents_dir = AgentPaths::cli_agents_dir(os).await?;
    let mut agents = HashMap::new();

    if agents_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.ends_with(".json") {
                        let agent_name = file_name.trim_end_matches(".json");
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if let Ok(config) = serde_json::from_str::<AgentConfig>(&content) {
                                agents.insert(agent_name.to_string(), config);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(agents)
}
