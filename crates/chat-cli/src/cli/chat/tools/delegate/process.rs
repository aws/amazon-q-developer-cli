use std::process::Stdio;

use eyre::Result;
use time::OffsetDateTime;
use tokio::process::Command;

use crate::cli::chat::tools::delegate::agent_manager::save_agent_execution;
use crate::cli::chat::tools::delegate::types::{
    AgentExecution,
    AgentStatus,
};
use crate::os::Os;

const DEFAULT_AGENT: &str = "default";

pub async fn spawn_agent_process(os: &Os, agent: &str, task: &str) -> Result<AgentExecution> {
    let now = OffsetDateTime::now_utc().to_string();

    let mut cmd = Command::new("q");
    cmd.arg("chat").arg("--non-interactive").arg("--trust-all-tools");

    if agent != DEFAULT_AGENT {
        cmd.arg("--agent").arg(agent);
    }

    cmd.arg(task)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);

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
    Ok(execution)
}

pub async fn start_monitoring(execution: AgentExecution, os: Os) {
    tokio::spawn(monitor_agent_execution(execution, os));
}

pub fn format_launch_success(agent: &str, task: &str) -> String {
    format!(
        "✓ Task launched with agent '{}'\nTask: {}\nAgent is now working independently. Use the delegate tool with 'status' operation to check progress.",
        agent, task
    )
}

async fn monitor_agent_execution(mut execution: AgentExecution, os: Os) {
    let result = async {
        let child = tokio::process::Command::new("q")
            .arg("chat")
            .arg("--non-interactive")
            .arg("--trust-all-tools")
            .args(if execution.agent != DEFAULT_AGENT {
                vec!["--agent", &execution.agent]
            } else {
                vec![]
            })
            .arg(&execution.task)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()?;

        let output = child.wait_with_output().await?;

        execution.status = if output.status.success() {
            AgentStatus::Completed
        } else {
            AgentStatus::Failed
        };
        execution.completed_at = Some(OffsetDateTime::now_utc().to_string());
        execution.exit_code = output.status.code();

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        execution.output = if stderr_str.is_empty() {
            stdout_str.to_string()
        } else {
            format!("{}\n\nSTDERR:\n{}", stdout_str, stderr_str)
        };

        save_agent_execution(&os, &execution).await?;

        Ok::<(), eyre::Error>(())
    }
    .await;

    if let Err(e) = result {
        eprintln!("Error monitoring agent execution: {}", e);
    }
}
