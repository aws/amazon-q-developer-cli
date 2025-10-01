use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};

use crate::cli::DEFAULT_AGENT_NAME;
use crate::cli::chat::tools::delegate::agent::subagents_dir;
use crate::cli::chat::tools::delegate::{
    AgentExecution,
    AgentStatus,
    launch_agent,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::os::Os;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SubagentHeader {
    pub launched_at: String,
    pub agent: Option<String>,
    pub prompt: String,
    pub status: String, // "active", "completed", "failed"
    pub pid: u32,
    pub completed_at: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct SubagentContent {
    pub output: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct StatusFile {
    pub subagents: HashMap<String, SubagentHeader>,
    pub last_updated: String,
}

#[derive(Debug, PartialEq, clap::Subcommand)]
pub enum DelegateArgs {
    /// Show status of tasks
    Status {
        /// Specific task agent name (optional)
        agent_name: Option<String>,
    },
    /// Read output from a task
    Read {
        /// Task agent name
        agent_name: String,
    },
    /// Delete a task and its files
    Delete {
        /// Task agent name
        agent_name: String,
    },
    /// Launch a new task
    Launch {
        /// Agent to use for the task
        #[arg(long)]
        agent: Option<String>,
        /// Task description
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        prompt: Vec<String>,
    },
}

impl DelegateArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if !is_enabled(os) {
            return Err(ChatError::Custom(
                "Delegate feature is not enabled. Enable it with /experiment command.".into(),
            ));
        }

        let executions = gather_executions(os)
            .await
            .map_err(|e| ChatError::Custom(e.to_string().into()))?;

        let result = match self {
            DelegateArgs::Status { agent_name } => {
                show_status(
                    agent_name.as_deref(),
                    &executions.iter().map(|(e, _)| e).collect::<Vec<_>>(),
                )
                .await
            },
            DelegateArgs::Read { agent_name } => {
                let (execution, path) = executions
                    .iter()
                    .find(|(e, _)| e.agent.as_str() == agent_name)
                    .ok_or(ChatError::Custom("No task found".into()))?;

                let execution_as_str =
                    serde_json::to_string(&execution).map_err(|e| ChatError::Custom(e.to_string().into()))?;

                _ = os.fs.remove_file(path).await;

                return Ok(ChatState::HandleInput {
                    input: format!(
                        "Delegate task with agent {} has concluded with the following content: {}",
                        &execution.agent, execution_as_str,
                    ),
                });
            },
            DelegateArgs::Delete { agent_name } => {
                let (_, path) = executions
                    .iter()
                    .find(|(e, _)| e.agent.as_str() == agent_name)
                    .ok_or(ChatError::Custom("No task found".into()))?;
                os.fs.remove_file(path).await?;

                Ok(format!("Task with agent {agent_name} has been deleted"))
            },
            DelegateArgs::Launch { agent, prompt } => {
                let prompt_str = prompt.join(" ");
                if prompt_str.trim().is_empty() {
                    return Err(ChatError::Custom("Please provide a prompt for the task".into()));
                }

                launch_agent(
                    os,
                    agent.as_deref().unwrap_or(DEFAULT_AGENT_NAME),
                    &session.conversation.agents,
                    &prompt_str,
                )
                .await
            },
        };

        match result {
            Ok(output) => {
                crossterm::queue!(session.stderr, crossterm::style::Print(format!("{}\n", output)))?;
            },
            Err(e) => {
                crossterm::queue!(session.stderr, crossterm::style::Print(format!("Error: {}\n", e)))?;
            },
        }

        session.stderr.flush()?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}

fn is_enabled(os: &Os) -> bool {
    ExperimentManager::is_enabled(os, ExperimentName::Delegate)
}

async fn gather_executions(os: &Os) -> Result<Vec<(AgentExecution, PathBuf)>> {
    let mut dir_walker = os.fs.read_dir(subagents_dir(os).await?).await?;
    let mut executions = Vec::<(AgentExecution, PathBuf)>::new();

    while let Ok(Some(file)) = dir_walker.next_entry().await {
        let bytes = os.fs.read(file.path()).await?;
        let execution = serde_json::from_slice::<AgentExecution>(&bytes)?;

        executions.push((execution, file.path()));
    }

    Ok(executions)
}

async fn show_status(agent_name: Option<&str>, executions: &[&AgentExecution]) -> Result<String> {
    if let Some(agent_name) = agent_name {
        let execution = executions
            .iter()
            .find(|e| e.agent.as_str() == agent_name)
            .ok_or(eyre::eyre!("Execution not found"))?;

        Ok(format!(
            "📦 Subagent Status: {}\n🤖 agent: {}\n📋 Task: {}\n⏰ Launched: {}",
            execution.status, execution.agent, execution.task, execution.launched_at
        ))
    } else {
        let mut active_count = 0;
        let mut completed_count = 0;
        let mut failed_count = 0;

        for execution in executions {
            match execution.status {
                AgentStatus::Running => active_count += 1,
                AgentStatus::Completed => completed_count += 1,
                AgentStatus::Failed => failed_count += 1,
            }
        }

        Ok(format!(
            "📊 Subagent Summary:\n🟢 Active: {}\n✅ Completed: {}\n❌ Failed: {}\n📈 Total: {}",
            active_count,
            completed_count,
            failed_count,
            executions.len()
        ))
    }
}
