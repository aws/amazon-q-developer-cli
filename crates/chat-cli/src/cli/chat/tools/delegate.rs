use std::io::{Write, stdin, stdout};
use std::process::Stdio;

use crossterm::execute;
use crossterm::style::{Color, Print, SetForegroundColor};
use eyre::{Result, eyre};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::process::Command;

use crate::cli::chat::tools::{InvokeOutput, OutputKind};
use crate::database::settings::Setting;
use crate::os::Os;

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

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct AgentExecution {
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub status: String, // "running", "completed", "failed"
    #[serde(default)]
    pub launched_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub output: String,
}

impl Delegate {
    pub async fn invoke(&self, os: &Os, _stdout: &mut impl Write) -> Result<InvokeOutput> {
        // Check if delegate experiment is enabled
        if !is_enabled(os) {
            return Ok(InvokeOutput {
                output: OutputKind::Text("Delegate tool is experimental and not enabled. Use /experiment to enable it.".to_string()),
            });
        }

        // Use "default_agent" if no agent specified for launch, "all" for status
        let agent_name = match self.operation.as_str() {
            "launch" => self.agent.as_deref().unwrap_or("default_agent"),
            "status" => self.agent.as_deref().unwrap_or("all"),
            _ => self.agent.as_deref().unwrap_or("default_agent"),
        };

        let result = match self.operation.as_str() {
            "launch" => {
                let task = self.task.as_ref().ok_or_else(|| eyre!("Task description required for launch operation"))?;
                launch_agent(os, agent_name, task).await?
            }
            "status" => {
                if agent_name == "all" {
                    status_all_agents(os).await?
                } else {
                    status_agent(os, agent_name).await?
                }
            }
            _ => return Err(eyre!("Invalid operation. Use: launch or status")),
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        let agent_name = self.agent.as_deref().unwrap_or("default_agent");
        match self.operation.as_str() {
            "launch" => {
                writeln!(output, "Launching agent '{}'", agent_name)?;
            }
            "status" => {
                writeln!(output, "Checking status of agent '{}'", agent_name)?;
            }
            _ => {
                writeln!(output, "Delegate operation '{}' on agent '{}'", self.operation, agent_name)?;
            }
        }
        Ok(())
    }
}

async fn get_agents_dir(os: &Os) -> Result<std::path::PathBuf> {
    let home_dir = os.env.home().unwrap_or_default();
    let agents_dir = home_dir.join(".aws").join("amazonq").join(".subagents");
    
    if !agents_dir.exists() {
        std::fs::create_dir_all(&agents_dir)?;
    }
    
    Ok(agents_dir)
}

async fn get_agent_file_path(os: &Os, agent: &str) -> Result<std::path::PathBuf> {
    let agents_dir = get_agents_dir(os).await?;
    Ok(agents_dir.join(format!("{}.json", agent)))
}

async fn load_agent_execution(os: &Os, agent: &str) -> Result<Option<AgentExecution>> {
    let file_path = get_agent_file_path(os, agent).await?;
    
    if !file_path.exists() {
        return Ok(None);
    }
    
    let content = os.fs.read_to_string(&file_path).await?;
    let execution: AgentExecution = serde_json::from_str(&content)?;
    Ok(Some(execution))
}

async fn save_agent_execution(os: &Os, execution: &AgentExecution) -> Result<()> {
    let file_path = get_agent_file_path(os, &execution.agent).await?;
    let content = serde_json::to_string_pretty(execution)?;
    os.fs.write(&file_path, content).await?;
    Ok(())
}

async fn launch_agent(
    os: &Os,
    agent: &str,
    task: &str,
) -> Result<String> {
    // Check if agent is already running
    if let Some(existing) = load_agent_execution(os, agent).await? {
        if existing.status == "running" {
            return Ok(format!("Agent '{}' is already running a task", agent));
        }
    }
    
    // Load available agents from ~/.aws/amazonq/agents/
    let agents_config = load_available_agents(os).await?;
    
    // If specific agent requested, validate it exists
    if agent != "default_agent" {
        if let Some(agent_config) = agents_config.get(agent) {
            // Show agent details and ask for approval
            let short_desc = agent_config.description.as_deref().unwrap_or("No description");
            let short_desc = if let Some(pos) = short_desc.find('.') {
                &short_desc[..pos + 1]
            } else if short_desc.len() > 60 {
                &format!("{}...", &short_desc[..57])
            } else {
                short_desc
            };
            
            execute!(
                stdout(),
                Print(format!("Agent: {}\n", agent)),
                Print(format!("Description: {}\n", short_desc)),
                Print(format!("Task: {}\n", task)),
            )?;
            
            if !agent_config.allowed_tools.is_empty() {
                let tools: Vec<&str> = agent_config.allowed_tools.iter().map(|s| s.as_str()).collect();
                execute!(
                    stdout(),
                    Print(format!("Tools: {}\n", tools.join(", "))),
                )?;
            }
            
            execute!(
                stdout(),
                Print("\n"),
                SetForegroundColor(Color::Yellow),
                Print("! This task will run with trust-all permissions and can execute commands or consume system/cloud resources. Continue? [y/N]: "),
                SetForegroundColor(Color::Reset),
            )?;
            
            stdout().flush()?;
            
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            
            if input != "y" && input != "yes" {
                return Ok("✗ Task delegation cancelled by user.".to_string());
            }
            
            println!(); // Add blank line after approval
        } else {
            // Agent not found - return error with available agents
            let available: Vec<String> = agents_config.keys().cloned().collect();
            if available.is_empty() {
                return Ok(format!("✗ I can't find agent '{}'. No agents are configured. You need to set up agents first.", agent));
            } else {
                return Ok(format!("✗ I can't find agent '{}'. Available agents: {}\n\nPlease use one of the available agents or set up the '{}' agent first.", 
                    agent, 
                    available.join(", "),
                    agent
                ));
            }
        }
    } else {
        // For default_agent, show warning
        execute!(
            stdout(),
            Print("\n"),
            SetForegroundColor(Color::Yellow),
            Print("! This task will run with trust-all permissions and can execute commands or consume system/cloud resources.\n\n"),
            SetForegroundColor(Color::Reset),
        )?;
    }
    
    // Launch the agent
    let now = OffsetDateTime::now_utc().to_string();
    
    let mut cmd = Command::new("q");
    cmd.arg("chat")
        .arg("--non-interactive")
        .arg("--trust-all-tools");
    
    if agent != "default_agent" {
        cmd.arg("--agent").arg(agent);
    }
    
    cmd.arg(task)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    
    let child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);
    
    // Create execution record
    let execution = AgentExecution {
        agent: agent.to_string(),
        task: task.to_string(),
        status: "running".to_string(),
        launched_at: now,
        completed_at: None,
        pid,
        exit_code: None,
        output: String::new(),
    };
    
    save_agent_execution(os, &execution).await?;
    
    // Monitor the execution in background
    tokio::spawn(monitor_agent_execution(agent.to_string(), child, os.clone()));
    
    Ok(format!(
        "✓ Task launched with agent '{}'\nTask: {}\nAgent is now working independently. Use the delegate tool with 'status' operation to check progress.",
        agent, task
    ))
}

async fn status_all_agents(os: &Os) -> Result<String> {
    let agents_dir = get_agents_dir(os).await?;
    let mut results = Vec::new();
    
    // Check all agent execution files
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries.flatten() {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".json") && file_name != "status.json" {
                    let agent_name = file_name.trim_end_matches(".json");
                    if let Some(execution) = load_agent_execution(os, agent_name).await? {
                        let status_summary = match execution.status.as_str() {
                            "running" => format!("● {} - Running (started {})", agent_name, execution.launched_at),
                            "completed" => format!("✓ {} - Completed ({})", agent_name, execution.completed_at.unwrap_or_default()),
                            "failed" => format!("✗ {} - Failed (exit code: {})", agent_name, execution.exit_code.unwrap_or(-1)),
                            _ => format!("? {} - Unknown status: {}", agent_name, execution.status),
                        };
                        results.push(status_summary);
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

async fn status_agent(os: &Os, agent: &str) -> Result<String> {
    match load_agent_execution(os, agent).await? {
        Some(execution) => {
            match execution.status.as_str() {
                "running" => {
                    Ok(format!(
                        "Agent '{}' is currently running.\nTask: {}\nStarted: {}",
                        execution.agent,
                        execution.task,
                        execution.launched_at
                    ))
                }
                "completed" => {
                    let output_preview = if execution.output.is_empty() {
                        "No output available".to_string()
                    } else {
                        // Send full output to LLM for summarization instead of truncating
                        format!("Agent completed successfully. Output ({} characters):\n\n{}", 
                            execution.output.len(),
                            execution.output)
                    };
                    
                    Ok(format!(
                        "Agent '{}' completed successfully.\nTask: {}\nStarted: {}\nCompleted: {}\n\nOutput:\n{}",
                        execution.agent,
                        execution.task,
                        execution.launched_at,
                        execution.completed_at.unwrap_or_default(),
                        output_preview
                    ))
                }
                "failed" => {
                    let error_info = if execution.output.is_empty() {
                        "No error details available".to_string()
                    } else if execution.output.len() > 1000 {
                        format!("{}...\n\n[Error output truncated - {} characters total]", 
                            &execution.output[..1000], 
                            execution.output.len())
                    } else {
                        execution.output.clone()
                    };
                    
                    Ok(format!(
                        "Agent '{}' failed.\nTask: {}\nStarted: {}\nFailed: {}\nExit code: {}\n\nError details:\n{}",
                        execution.agent,
                        execution.task,
                        execution.launched_at,
                        execution.completed_at.unwrap_or_default(),
                        execution.exit_code.unwrap_or(-1),
                        error_info
                    ))
                }
                _ => {
                    Ok(format!("Agent '{}' has unknown status: {}", agent, execution.status))
                }
            }
        }
        None => Ok(format!("No execution found for agent '{}'", agent)),
    }
}

async fn load_available_agents(os: &Os) -> Result<std::collections::HashMap<String, AgentConfig>> {
    let home_dir = os.env.home().unwrap_or_default();
    let agents_dir = home_dir.join(".aws").join("amazonq").join("cli-agents");
    
    let mut agents = std::collections::HashMap::new();
    
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

#[derive(Debug, Deserialize)]
struct AgentConfig {
    description: Option<String>,
    #[serde(rename = "allowedTools")]
    allowed_tools: Vec<String>,
}

async fn monitor_agent_execution(agent: String, child: tokio::process::Child, os: Os) {
    let result = async {
        let output = child.wait_with_output().await?;
        
        // Load current execution state
        let mut execution = load_agent_execution(&os, &agent).await?
            .ok_or_else(|| eyre!("Agent execution not found"))?;
        
        // Update with completion info
        execution.status = if output.status.success() { "completed" } else { "failed" }.to_string();
        execution.completed_at = Some(OffsetDateTime::now_utc().to_string());
        execution.exit_code = output.status.code();
        
        // Combine stdout and stderr
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        execution.output = if stderr_str.is_empty() {
            stdout_str.to_string()
        } else {
            format!("{}\n\nSTDERR:\n{}", stdout_str, stderr_str)
        };
        
        save_agent_execution(&os, &execution).await?;
        
        Ok::<(), eyre::Error>(())
    }.await;
    
    if let Err(e) = result {
        eprintln!("Error monitoring agent execution: {}", e);
    }
}

fn is_enabled(os: &Os) -> bool {
    os.database.settings.get_bool(Setting::EnabledDelegate).unwrap_or(false)
}
