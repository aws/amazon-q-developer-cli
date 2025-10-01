mod agent;
mod execution;
mod types;
mod ui;

// Re-export types for external use
use std::io::Write;

use agent::{
    load_agent_execution,
    request_user_approval,
    validate_agent_availability,
};
use crossterm::{
    queue,
    style,
};
use execution::{
    spawn_agent_process,
    status_agent,
    status_all_agents,
};
use eyre::Result;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use strum::Display;
use ui::display_default_agent_warning;

use crate::cli::DEFAULT_AGENT_NAME;
use crate::cli::agent::Agents;
use crate::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
};
use crate::database::settings::Setting;
use crate::os::Os;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
/// Launch and manage asynchronous agent processes. This tool allows you to delegate tasks to agents
/// that run independently in the background.\n\nOperations:\n- launch: Start a new task with an
/// agent (requires task parameter, agent is optional)\n- status: Check agent status and get full
/// output if completed. Agent is optional - defaults to 'all' if not specified\n\nIf no agent is
/// specified for launch, uses 'default_agent'. Only one task can run per agent at a time. Files are
/// stored in ~/.aws/amazonq/.subagents/\n\nIMPORTANT: If a specific agent is requested but not
/// found, DO NOT automatically retry with 'default_agent' or any other agent. Simply report the
/// error and available agents to the user.\n\nExample usage:\n1. Launch with agent: {\"operation\":
/// \"launch\", \"agent\": \"rust-agent\", \"task\": \"Create a snake game\"}\n2. Launch without
/// agent: {\"operation\": \"launch\", \"task\": \"Write a Python script\"}\n3. Check specific
/// agent: {\"operation\": \"status\", \"agent\": \"rust-agent\"}\n4. Check all agents:
/// {\"operation\": \"status\", \"agent\": \"all\"}\n5. Check all agents (shorthand):
/// {\"operation\": \"status\"}
pub struct Delegate {
    /// Operation to perform: launch, status, or list
    pub operation: Operation,
    /// Agent name to use (optional - uses "q_cli_default" if not specified)
    #[serde(default)]
    pub agent: Option<String>,
    /// Task description (required for launch operation)
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Serialize, Clone, Deserialize, Debug, Display, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    /// Launch a new agent with a specified task
    Launch,
    /// Check the status of a specific agent or all agents if None is provided
    Status(Option<String>),
    /// List all available agents
    List,
}

#[allow(unused_imports)]
pub use types::{
    AgentConfig,
    AgentExecution,
    AgentStatus,
};

impl Delegate {
    pub async fn invoke(&self, os: &Os, _output: &mut impl Write, agents: &Agents) -> Result<InvokeOutput> {
        if !is_enabled(os) {
            return Ok(InvokeOutput {
                output: OutputKind::Text(
                    "Delegate tool is experimental and not enabled. Use /experiment to enable it.".to_string(),
                ),
            });
        }

        let result = match &self.operation {
            Operation::Launch => {
                let task = self
                    .task
                    .as_ref()
                    .ok_or(eyre::eyre!("Task description is required for launch operation"))?;

                let agent_name = self.agent.as_deref().unwrap_or(DEFAULT_AGENT_NAME);

                launch_agent(os, agent_name, agents, task).await?
            },
            Operation::Status(name) => match name {
                Some(agent_name) => status_agent(os, agent_name).await?,
                None => status_all_agents(os).await?,
            },
            Operation::List => agents.agents.keys().cloned().fold(
                format!("Available agents: \n- {DEFAULT_AGENT_NAME}\n"),
                |mut acc, name| {
                    acc.push_str(&format!("- {name}\n"));
                    acc
                },
            ),
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        match self.operation {
            Operation::Launch => queue!(output, style::Print("Delegating task to agent\n"))?,
            Operation::Status(_) => queue!(output, style::Print("Checking agent status\n"))?,
            Operation::List => queue!(output, style::Print("Listing available agents\n"))?,
        }

        Ok(())
    }
}

async fn launch_agent(os: &Os, agent: &str, agents: &Agents, task: &str) -> Result<String> {
    validate_agent_availability(os, agent).await?;

    // Check if agent is already running
    if let Some(execution) = load_agent_execution(os, agent).await? {
        if execution.status == AgentStatus::Running {
            return Err(eyre::eyre!(
                "Agent '{}' is already running. Use status operation to check progress or wait for completion.",
                agent
            ));
        }
    }

    if agent == DEFAULT_AGENT_NAME {
        // Show warning for default agent but no approval needed
        display_default_agent_warning()?;
    } else {
        // Show agent info and require approval for specific agents
        request_user_approval(agent, agents, task).await?;
    }

    spawn_agent_process(os, agent, task).await?;

    Ok(format_launch_success(agent, task))
}

fn format_launch_success(agent: &str, task: &str) -> String {
    format!(
        "✓ Agent '{}' launched successfully.\nTask: {}\n\nUse 'status' operation to check progress.",
        agent, task
    )
}

fn is_enabled(os: &Os) -> bool {
    os.database.settings.get_bool(Setting::EnabledDelegate).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_schema() {
        let schema = schemars::schema_for!(Delegate);
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    }
}
