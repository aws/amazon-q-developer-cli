use eyre::Result;

use crate::cli::chat::tools::delegate::agent_manager::load_agent_execution;
use crate::cli::chat::tools::delegate::agent_paths::AgentPaths;
use crate::cli::chat::tools::delegate::errors::AgentError;
use crate::os::Os;

pub async fn status_all_agents(os: &Os) -> Result<String> {
    let agents_dir = AgentPaths::subagents_dir(os).await?;
    let mut results = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".json") && file_name != "status.json" {
                    let agent_name = file_name.trim_end_matches(".json");
                    if let Some(execution) = load_agent_execution(os, agent_name).await? {
                        results.push(execution.format_status_line());
                    }
                }
            }
        }
    }

    if results.is_empty() {
        Ok("No agent executions found.".to_string())
    } else {
        Ok(format!("Agent Status Summary:\n\n{}", results.join("\n")))
    }
}

pub async fn status_agent(os: &Os, agent: &str) -> Result<String> {
    match load_agent_execution(os, agent).await? {
        Some(execution) => Ok(execution.format_detailed_status()),
        None => Ok(AgentError::no_execution_found(agent)),
    }
}
