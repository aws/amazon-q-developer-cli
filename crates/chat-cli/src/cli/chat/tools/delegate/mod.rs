mod agent;
mod execution;
mod types;
mod ui;

// Re-export types for external use
use std::io::Write;

use agent::{
    list_available_agents,
    load_agent_execution,
    request_user_approval,
    validate_agent_availability,
};
use execution::{spawn_agent_process, status_agent, status_all_agents};
use ui::display_default_agent_warning;
use eyre::{
    Result,
    eyre,
};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
};
use crate::database::settings::Setting;
use crate::os::Os;

const DEFAULT_AGENT: &str = "default";
const ALL_AGENTS: &str = "all";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegate {
    /// Operation to perform: launch, status, or list
    pub operation: String,
    /// Agent name to use (optional - uses "default" if not specified)
    #[serde(default)]
    pub agent: Option<String>,
    /// Task description (required for launch operation)
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Debug, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
enum Operation {
    Launch,
    Status,
    List,
}

#[allow(unused_imports)]
pub use types::{
    AgentConfig,
    AgentExecution,
    AgentStatus,
};

impl Delegate {
    pub async fn invoke(&self, os: &Os, _output: &mut impl Write) -> Result<InvokeOutput> {
        if !is_enabled(os) {
            return Ok(InvokeOutput {
                output: OutputKind::Text(
                    "Delegate tool is experimental and not enabled. Use /experiment to enable it.".to_string(),
                ),
            });
        }

        // Validate operation first
        let operation = self.operation.parse::<Operation>()
            .map_err(|_| eyre!("Invalid operation. Use: launch, status, or list"))?;

        // Validate required fields based on operation
        match operation {
            Operation::Launch => {
                if self.task.is_none() {
                    return Err(eyre!("Task description is required for launch operation"));
                }
                if self.agent.is_none() {
                    return Err(eyre!("Agent name is required for launch operation. Use 'list' operation to see available agents, then specify agent name."));
                }
                
                // Validate agent name exists
                let agent_name = self.agent.as_ref().unwrap();
                if agent_name != DEFAULT_AGENT {
                    let available_agents = list_available_agents(os).await?;
                    if !available_agents.contains(agent_name) {
                        return Err(eyre!(
                            "Agent '{}' not found. Available agents: default, {}. Use exact names only.", 
                            agent_name, 
                            available_agents.join(", ")
                        ));
                    }
                }
            },
            Operation::Status | Operation::List => {
                // No additional validation needed
            }
        }

        let agent_name = self.get_agent_name();
        
        let result = match operation {
            Operation::Launch => {
                let task = self.task.as_ref().unwrap(); // Safe due to validation above
                launch_agent(os, agent_name, task).await?
            },
            Operation::Status => {
                if agent_name == ALL_AGENTS {
                    status_all_agents(os).await?
                } else {
                    status_agent(os, agent_name).await?
                }
            },
            Operation::List => {
                list_agents(os).await?
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        if let Ok(operation) = self.operation.parse::<Operation>() {
            match operation {
                Operation::Launch => writeln!(output, "Delegating task to agent")?,
                Operation::Status => writeln!(output, "Checking agent status")?,
                Operation::List => writeln!(output, "Listing available agents")?,
            }
        } else {
            writeln!(
                output,
                "Invalid operation '{}'. Use: launch, status, or list",
                self.operation
            )?;
        }
        Ok(())
    }

    fn get_agent_name(&self) -> &str {
        if let Ok(operation) = self.operation.parse::<Operation>() {
            match operation {
                Operation::Launch => {
                    // Agent is required for launch (validated above)
                    self.agent.as_deref().unwrap_or("") 
                },
                Operation::Status => self.agent.as_deref().unwrap_or(ALL_AGENTS),
                Operation::List => "", // Agent name not needed for list operation
            }
        } else {
            self.agent.as_deref().unwrap_or("")
        }
    }
}

async fn list_agents(os: &Os) -> Result<String> {
    let agents = list_available_agents(os).await?;
    if agents.is_empty() {
        Ok("No custom agents configured. Only 'default' agent is available.".to_string())
    } else {
        Ok(format!("Available agents: default, {}", agents.join(", ")))
    }
}

async fn launch_agent(os: &Os, agent: &str, task: &str) -> Result<String> {
    validate_agent_availability(os, agent).await?;
    
    // Check if agent is already running
    if let Some(execution) = load_agent_execution(os, agent).await? {
        if execution.status == AgentStatus::Running {
            return Err(eyre::eyre!("Agent '{}' is already running. Use status operation to check progress or wait for completion.", agent));
        }
    }
    
    if agent == DEFAULT_AGENT {
        // Show warning for default agent but no approval needed
        display_default_agent_warning()?;
    } else {
        // Show agent info and require approval for specific agents
        request_user_approval(os, agent, task).await?;
    }
    
    let _execution = spawn_agent_process(os, agent, task).await?;
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
