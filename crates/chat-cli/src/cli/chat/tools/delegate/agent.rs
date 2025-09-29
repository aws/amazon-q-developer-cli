use std::path::PathBuf;

use eyre::Result;
use serde_json;

use crate::cli::agent::Agents;
use crate::cli::chat::tools::delegate::types::{
    AgentConfig,
    AgentExecution,
    AgentExecution,
};
use crate::cli::chat::tools::delegate::ui::{
    display_agent_info,
    display_agent_info,
    get_user_confirmation,
    get_user_confirmation,
};
use crate::os::Os;

pub async fn validate_agent_availability(_os: &Os, _agent: &str) -> Result<()> {
    // For now, accept any agent name (no need to print here, will show in approval)
    Ok(())
}

pub async fn request_user_approval(agent: &str, agents: &Agents, task: &str) -> Result<()> {
    let config = agents
        .agents
        .get(agent)
        .ok_or(eyre::eyre!("No agent by the name {agent} found"))?
        .into();
    display_agent_info(agent, task, &config)?;
    get_user_confirmation()?;

    Ok(())
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
