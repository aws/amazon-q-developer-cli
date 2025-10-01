use std::process::Command;

use chrono::Utc;
use eyre::Result;

use crate::cli::chat::tools::delegate::agent::{
    load_agent_execution,
    save_agent_execution,
};
use crate::cli::chat::tools::delegate::types::{
    AgentExecution,
    AgentStatus,
};
use crate::os::Os;

pub async fn spawn_agent_process(os: &Os, agent: &str, task: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    // Run Q chat with specific agent in background, non-interactive
    let mut cmd = tokio::process::Command::new("q");
    cmd.args(["chat", "--agent", agent, task]);

    // Redirect to capture output (runs silently)
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdin(std::process::Stdio::null()); // No user input

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
    tokio::spawn(monitor_child_process(child, execution, os.clone()));

    Ok(())
}

async fn monitor_child_process(child: tokio::process::Child, mut execution: AgentExecution, os: Os) {
    match child.wait_with_output().await {
        Ok(output) => {
            execution.status = if output.status.success() {
                AgentStatus::Completed
            } else {
                AgentStatus::Failed
            };
            execution.completed_at = Some(Utc::now().to_rfc3339());
            execution.exit_code = output.status.code();

            // Combine stdout and stderr into the output field
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            execution.output = if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr)
            };

            // Save to ~/.aws/amazonq/.subagents/{agent}.json
            if let Err(e) = save_agent_execution(&os, &execution).await {
                eprintln!("Failed to save agent execution: {}", e);
            }
        },
        Err(e) => {
            execution.status = AgentStatus::Failed;
            execution.completed_at = Some(Utc::now().to_rfc3339());
            execution.exit_code = Some(-1);
            execution.output = format!("Failed to wait for process: {}", e);

            // Save to ~/.aws/amazonq/.subagents/{agent}.json
            if let Err(e) = save_agent_execution(&os, &execution).await {
                eprintln!("Failed to save agent execution: {}", e);
            }
        },
    }
}

pub async fn status_agent(os: &Os, agent: &str) -> Result<String> {
    match load_agent_execution(os, agent).await? {
        Some(mut execution) => {
            // If status is running, check if PID is still alive
            if execution.status == AgentStatus::Running && execution.pid != 0 && !is_process_alive(execution.pid) {
                // Process died, mark as failed
                execution.status = AgentStatus::Failed;
                execution.completed_at = Some(chrono::Utc::now().to_rfc3339());
                execution.exit_code = Some(-1);
                execution.output = "Process terminated unexpectedly (PID not found)".to_string();

                // Save the updated status
                save_agent_execution(os, &execution).await?;
            }

            Ok(execution.format_status())
        },
        None => Ok(format!("No execution found for agent '{}'", agent)),
    }
}

pub async fn status_all_agents(_os: &Os) -> Result<String> {
    // For now, just return a simple message
    Ok("Use --agent <name> to check specific agent status".to_string())
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Use `kill -0` to check if process exists without actually killing it
        Command::new("kill")
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
