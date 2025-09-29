use std::path::PathBuf;

use eyre::Result;
use serde_json;

use crate::cli::chat::tools::delegate::types::{
    AgentConfig,
    AgentExecution,
};
use crate::cli::chat::tools::delegate::ui::{
    display_agent_info,
    get_user_confirmation,
};
use crate::os::Os;
use crate::util::directories::{
    chat_global_agent_path,
    home_dir,
};

pub async fn validate_agent_availability(_os: &Os, _agent: &str) -> Result<()> {
    // For now, accept any agent name (no need to print here, will show in approval)
    Ok(())
}

pub async fn request_user_approval(os: &Os, agent: &str, task: &str) -> Result<()> {
    let config = load_agent_config(os, agent).await;
    display_agent_info(agent, task, &config)?;
    get_user_confirmation()?;
    Ok(())
}

async fn load_agent_config(os: &Os, agent: &str) -> AgentConfig {
    match load_real_agent_config(os, agent).await {
        Ok(config) => config,
        Err(_) => AgentConfig {
            description: Some(format!("Agent '{}' (no config found)", agent)),
            allowed_tools: vec!["No tools specified".to_string()],
        },
    }
}

async fn load_real_agent_config(os: &Os, agent: &str) -> Result<AgentConfig> {

    let config_path = cli_agents_dir.join(format!("{}.json", agent));
    if config_path.exists() {
        let content = os.fs.read_to_string(&config_path).await?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        return Ok(AgentConfig {
            description: config
                .get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string()),
            allowed_tools: config
                .get("allowedTools")
                .and_then(|t| t.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
                .or_else(|| {
                    // Fallback to "tools" if "allowedTools" not found
                    config
                        .get("tools")
                        .and_then(|t| t.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
                })
                .unwrap_or_else(|| vec!["No tools specified".to_string()]),
        });
    }

    Err(eyre::eyre!("Agent config not found"))
}

pub async fn list_available_agents(os: &Os) -> Result<Vec<String>> {
    let cli_agents_dir = home_dir(os)?.join(".aws").join("amazonq").join("cli-agents");

    if !cli_agents_dir.exists() {
        return Ok(vec![]);
    }

    let mut agents = vec![];
    let mut entries = os.fs.read_dir(&cli_agents_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(extension) = path.extension() {
            if extension == "json" {
                if let Some(stem) = path.file_stem() {
                    if let Some(agent_name) = stem.to_str() {
                        agents.push(agent_name.to_string());
                    }
                }
            }
        }
    }

    agents.sort();
    Ok(agents)
}

pub async fn load_agent_execution(os: &Os, agent: &str) -> Result<Option<AgentExecution>> {
    let file_path = agent_file_path(os, agent).await?;

    if file_path.exists() {
        let content = os.fs.read_to_string(&file_path).await?;
        let execution: AgentExecution = serde_json::from_str(&content)?;
        Ok(Some(execution))
    } else {
        Ok(None)
    }
}

pub async fn save_agent_execution(os: &Os, execution: &AgentExecution) -> Result<()> {
    let file_path = agent_file_path(os, &execution.agent).await?;
    let content = serde_json::to_string_pretty(execution)?;
    os.fs.write(&file_path, content).await?;
    Ok(())
}

async fn agent_file_path(os: &Os, agent: &str) -> Result<PathBuf> {
    let subagents_dir = subagents_dir(os).await?;
    Ok(subagents_dir.join(format!("{}.json", agent)))
}

async fn subagents_dir(os: &Os) -> Result<PathBuf> {
    let subagents_dir = home_dir(os)?.join(".aws").join("amazonq").join(".subagents");
    if !subagents_dir.exists() {
        os.fs.create_dir_all(&subagents_dir).await?;
    }
    Ok(subagents_dir)
}
