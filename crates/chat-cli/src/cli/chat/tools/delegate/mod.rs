mod agent_manager;
mod agent_paths;
mod errors;
mod file_ops;
mod process;
mod status;
mod types;
mod ui;

// Re-export types for external use
use std::io::Write;

use agent_manager::{
    request_user_approval,
    validate_agent_availability,
};
use eyre::{
    Result,
    eyre,
};
use process::{
    format_launch_success,
    spawn_agent_process,
    start_monitoring,
};
use serde::{
    Deserialize,
    Serialize,
};
use status::{
    status_agent,
    status_all_agents,
};
#[allow(unused_imports)]
pub use types::{
    AgentConfig,
    AgentExecution,
    AgentStatus,
};

use crate::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
};
use crate::database::settings::Setting;
use crate::os::Os;

const OPERATION_LAUNCH: &str = "launch";
const OPERATION_STATUS: &str = "status";
const DEFAULT_AGENT: &str = "default";
const ALL_AGENTS: &str = "all";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delegate {
    /// Operation to perform: launch or status
    pub operation: String,
    /// Agent name to use (optional - uses "default_agent" if not specified)
    #[serde(default)]
    pub agent: Option<String>,
    /// Task description (required for launch operation)
    #[serde(default)]
    pub task: Option<String>,
}

impl Delegate {
    pub async fn invoke(&self, os: &Os, _stdout: &mut impl Write) -> Result<InvokeOutput> {
        if !is_enabled(os) {
            return Ok(InvokeOutput {
                output: OutputKind::Text(
                    "Delegate tool is experimental and not enabled. Use /experiment to enable it.".to_string(),
                ),
            });
        }

        let agent_name = self.get_agent_name();
        let result = match self.operation.as_str() {
            OPERATION_LAUNCH => {
                let task = self
                    .task
                    .as_ref()
                    .ok_or_else(|| eyre!("Task description required for launch operation"))?;
                launch_agent(os, agent_name, task).await?
            },
            OPERATION_STATUS => {
                if agent_name == ALL_AGENTS {
                    status_all_agents(os).await?
                } else {
                    status_agent(os, agent_name).await?
                }
            },
            _ => {
                return Err(eyre!(
                    "Invalid operation. Use: {} or {}",
                    OPERATION_LAUNCH,
                    OPERATION_STATUS
                ));
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        let agent_name = self.get_agent_name();
        match self.operation.as_str() {
            OPERATION_LAUNCH => writeln!(output, "Launching agent '{}'", agent_name)?,
            OPERATION_STATUS => writeln!(output, "Checking status of agent '{}'", agent_name)?,
            _ => writeln!(
                output,
                "Delegate operation '{}' on agent '{}'",
                self.operation, agent_name
            )?,
        }
        Ok(())
    }

    fn get_agent_name(&self) -> &str {
        match self.operation.as_str() {
            OPERATION_LAUNCH => self.agent.as_deref().unwrap_or(DEFAULT_AGENT),
            OPERATION_STATUS => self.agent.as_deref().unwrap_or(ALL_AGENTS),
            _ => self.agent.as_deref().unwrap_or(DEFAULT_AGENT),
        }
    }
}

async fn launch_agent(os: &Os, agent: &str, task: &str) -> Result<String> {
    validate_agent_availability(os, agent).await?;
    request_user_approval(os, agent, task).await?;
    let execution = spawn_agent_process(os, agent, task).await?;
    start_monitoring(execution, os.clone()).await;
    Ok(format_launch_success(agent, task))
}

fn is_enabled(os: &Os) -> bool {
    os.database.settings.get_bool(Setting::EnabledDelegate).unwrap_or(false)
}
