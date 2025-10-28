use std::io::{
    Write,
    stdin,
    stdout,
};
use std::path::PathBuf;

use chrono::Utc;
use crossterm::style::Print;
use crossterm::{
    execute,
    queue,
    style,
};
use eyre::{
    Result,
    bail,
};
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use strum::{
    Display,
    EnumString,
};

use crate::cli::agent::Agents;
use crate::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
};
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::cli::{
    Agent,
    DEFAULT_AGENT_NAME,
};
use crate::os::Os;
use crate::theme::StyledText;

/// Launch and manage async agent processes. Delegate tasks to agents that run independently in
/// background.
///
/// Operations:
/// - launch: Start task with agent (requires task, agent optional - defaults to 'default_agent')
/// - status: Check agent status (agent optional - defaults to 'all')
/// - list: Show available agents
///
/// Only one task per agent. Files stored in ~/.kiro-cli/.subagents/
///
/// Examples:
/// - Launch: {"operation": "launch", "agent": "rust-agent", "task": "Create snake game"}
/// - Status: {"operation": "status", "agent": "rust-agent"}
/// - List all: {"operation": "status"}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    Status,
    /// List all available agents
    List,
}

impl Delegate {
    pub fn is_enabled(os: &Os) -> bool {
        ExperimentManager::is_enabled(os, ExperimentName::Delegate)
    }

    pub async fn invoke(&self, os: &Os, _output: &mut impl Write, agents: &Agents) -> Result<InvokeOutput> {
        if !Self::is_enabled(os) {
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
            Operation::Status => match &self.agent {
                Some(agent_name) => status_agent(os, agent_name).await?,
                None => match status_all_agents(os).await {
                    Ok(execution) => execution,
                    Err(msg) => msg.to_string(),
                },
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
            Operation::Status => queue!(output, style::Print("Checking agent status\n"))?,
            Operation::List => queue!(output, style::Print("Listing available agents\n"))?,
        }

        Ok(())
    }
}

pub async fn launch_agent(os: &Os, agent: &str, agents: &Agents, task: &str) -> Result<String> {
    validate_agent_availability(os, agent).await?;

    // Check if agent is already running
    if let Some((execution, _)) = load_agent_execution(os, agent).await? {
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
    format!("✓ Agent '{agent}' launched successfully.\nTask: {task}\n\nUse 'status' operation to check progress.")
}

pub fn display_agent_info(agent: &str, task: &str, config: &AgentConfig) -> Result<()> {
    let short_desc = truncate_description(config.description.as_deref().unwrap_or("No description"));

    execute!(
        stdout(),
        Print(format!("Agent: {agent}\n")),
        Print(format!("Description: {short_desc}\n")),
        Print(format!("Task: {task}\n")),
    )?;

    if !config.allowed_tools.is_empty() {
        let tools: Vec<&str> = config.allowed_tools.iter().map(|s| s.as_str()).collect();
        execute!(stdout(), Print(format!("Tools: {}\n", tools.join(", "))))?;
    }

    // Add appropriate security warning based on agent permissions
    execute!(
        stdout(),
        Print("\n"),
        StyledText::warning_fg(),
        Print("! This task will run with the agent's specific tool permissions.\n\n"),
        StyledText::reset(),
    )?;

    Ok(())
}

pub fn truncate_description(desc: &str) -> &str {
    if let Some(pos) = desc.find('.') {
        &desc[..pos + 1]
    } else if desc.len() > 60 {
        &desc[..57]
    } else {
        desc
    }
}

pub fn display_default_agent_warning() -> Result<()> {
    execute!(
        stdout(),
        Print("\n"),
        StyledText::warning_fg(),
        Print(
            "! This task will run with trust-all permissions and can execute commands or consume system/cloud resources.\n\n"
        ),
        StyledText::reset(),
    )?;
    Ok(())
}

pub fn get_user_confirmation() -> Result<bool> {
    execute!(
        stdout(),
        StyledText::warning_fg(),
        Print("Continue? [y/N]: "),
        StyledText::reset(),
    )?;

    let mut input = String::new();
    stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input == "y" || input == "yes" {
        println!();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Running
    }
}

impl AgentStatus {
    // No methods currently needed - all functionality is in format_status
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AgentExecution {
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub status: AgentStatus,
    #[serde(default, with = "chrono::serde::ts_seconds")]
    pub launched_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, with = "chrono::serde::ts_seconds_option")]
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub output: String,
}

impl AgentExecution {
    pub fn format_status(&self) -> String {
        match self.status {
            AgentStatus::Running => {
                format!("Agent '{}' is still running. Please wait...", self.agent)
            },
            AgentStatus::Completed => {
                format!(
                    "Agent '{}' completed successfully.\n\nOutput:\n{}",
                    self.agent, self.output
                )
            },
            AgentStatus::Failed => {
                format!(
                    "Agent '{}' failed.\nExit code: {}\n\nError:\n{}",
                    self.agent,
                    self.exit_code.unwrap_or(-1),
                    self.output
                )
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AgentConfig {
    pub description: Option<String>,
    #[serde(rename = "allowedTools")]
    pub allowed_tools: Vec<String>,
}

impl From<&Agent> for AgentConfig {
    fn from(value: &Agent) -> Self {
        Self {
            description: value.description.clone(),
            allowed_tools: value.allowed_tools.iter().cloned().collect::<Vec<String>>(),
        }
    }
}

pub async fn spawn_agent_process(os: &Os, agent: &str, task: &str) -> Result<AgentExecution> {
    let now = Utc::now();

    // Run Q chat with specific agent in background, non-interactive
    let mut cmd = tokio::process::Command::new("q");
    cmd.args(["chat", "--non-interactive"]);
    if agent == DEFAULT_AGENT_NAME {
        cmd.arg("--trust-all-tools");
    }
    cmd.args(["--agent", agent, task]);

    // Redirect to capture output (runs silently)
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null()); // No user input
    cmd.envs(std::env::vars());

    #[cfg(not(windows))]
    cmd.process_group(0);

    let child = cmd.spawn()?;
    let pid = child.id().ok_or(eyre::eyre!("Process spawned had already exited"))?;

    let execution = AgentExecution {
        agent: agent.to_string(),
        task: task.to_string(),
        status: AgentStatus::Running,
        launched_at: now,
        completed_at: None,
        pid,
        exit_code: None,
        output: String::new(),
    };

    save_agent_execution(os, &execution).await?;

    // Start monitoring with the actual child process
    tokio::spawn(monitor_child_process(child, execution.clone(), os.clone()));

    Ok(execution)
}

async fn monitor_child_process(child: tokio::process::Child, mut execution: AgentExecution, os: Os) {
    match child.wait_with_output().await {
        Ok(output) => {
            execution.status = if output.status.success() {
                AgentStatus::Completed
            } else {
                AgentStatus::Failed
            };
            execution.completed_at = Some(Utc::now());
            execution.exit_code = output.status.code();

            // Combine stdout and stderr into the output field
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            execution.output = if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("STDOUT:\n{stdout}\n\nSTDERR:\n{stderr}")
            };

            // Save to ~/.kiro-cli/.subagents/{agent}.json
            if let Err(e) = save_agent_execution(&os, &execution).await {
                eprintln!("Failed to save agent execution: {e}");
            }
        },
        Err(e) => {
            execution.status = AgentStatus::Failed;
            execution.completed_at = Some(Utc::now());
            execution.exit_code = Some(-1);
            execution.output = format!("Failed to wait for process: {e}");

            // Save to ~/.kiro-cli/.subagents/{agent}.json
            if let Err(e) = save_agent_execution(&os, &execution).await {
                eprintln!("Failed to save agent execution: {e}");
            }
        },
    }
}

pub async fn status_agent(os: &Os, agent: &str) -> Result<String> {
    match load_agent_execution(os, agent).await? {
        Some((mut execution, path)) => {
            // If status is running, check if PID is still alive
            if execution.status == AgentStatus::Running && execution.pid != 0 && !is_process_alive(execution.pid) {
                // Process died, mark as failed
                execution.status = AgentStatus::Failed;
                execution.completed_at = Some(chrono::Utc::now());
                execution.exit_code = Some(-1);
                execution.output = "Process terminated unexpectedly (PID not found)".to_string();

                // Save the updated status
                save_agent_execution(os, &execution).await?;
            }

            if execution.status == AgentStatus::Completed {
                let _ = os.fs.remove_file(path).await;
            }

            Ok(execution.format_status())
        },
        None => Ok(format!("No execution found for agent '{agent}'")),
    }
}

pub async fn status_all_agents(os: &Os) -> Result<String> {
    // Because we would delete completed execution that has been read, everything that remains is
    // assumed to not be stale
    let mut dir_walker = os.fs.read_dir(subagents_dir(os).await?).await?;
    let mut status = String::new();

    while let Ok(Some(file)) = dir_walker.next_entry().await {
        let file_name = file.file_name();

        let bytes = os.fs.read(file.path()).await?;
        let execution = serde_json::from_slice::<AgentExecution>(&bytes)?;

        if execution.status != AgentStatus::Running {
            let file_name = file_name
                .as_os_str()
                .to_str()
                .ok_or(eyre::eyre!("Error obtaining execution file name"))?;

            if !status.is_empty() {
                status.push_str(", ");
            }

            status.push_str(file_name);
        }
    }

    if status.is_empty() {
        bail!("No new completed delegate task".to_string())
    } else {
        Ok(format!("The following delegate tasks are ready: {status}"))
    }
}

#[allow(unused_variables)]
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Use `kill -0` to check if process exists without actually killing it
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        // For non-Unix systems, assume process is alive (fallback)
        true
    }
}

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

pub async fn load_agent_execution(os: &Os, agent: &str) -> Result<Option<(AgentExecution, PathBuf)>> {
    let file_path = agent_file_path(os, agent).await?;

    if file_path.exists() {
        let content = os.fs.read_to_string(&file_path).await?;
        let execution: AgentExecution = serde_json::from_str(&content)?;
        Ok(Some((execution, file_path)))
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

pub async fn agent_file_path(os: &Os, agent: &str) -> Result<PathBuf> {
    let subagents_dir = subagents_dir(os).await?;
    Ok(subagents_dir.join(format!("{agent}.json")))
}

pub async fn subagents_dir(os: &Os) -> Result<PathBuf> {
    let subagents_dir = os.env.current_dir()?.join(".kiro-cli").join(".subagents");
    if !subagents_dir.exists() {
        os.fs.create_dir_all(&subagents_dir).await?;
    }
    Ok(subagents_dir)
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
