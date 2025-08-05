use std::env::current_exe;
use std::io::Write;
use std::process::Stdio;

use crossterm::style::{
    self,
    Attribute,
    Color,
};
use crossterm::terminal::{
    Clear,
    ClearType,
};
use crossterm::{
    cursor,
    queue,
};
use eyre::Result;
use futures::future::join_all;
use serde::{
    Deserialize,
    Serialize,
};
use spinners::{
    Spinner,
    Spinners,
};
use tokio::io::{
    AsyncBufReadExt,
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::os::Os;

/// Tool for launching a new Q agent as a background process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgent {
    // 3-5 word unique name to identify agent
    pub agent_display_name: String,
    /// The prompt to send to the new agent
    pub prompt: String,
    /// Display string that summarizes prompt
    pub prompt_summary: String,
    /// Optional model to use for the agent (defaults to the system default)
    pub agent_cli_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentWrapper {
    pub subagents: Vec<SubAgent>,
}

impl SubAgentWrapper {
    pub async fn invoke(&self, updates: &mut impl Write) -> Result<InvokeOutput> {
        // Check if we're already in a subagent context to prevent nesting
        if std::env::var("Q_SUBAGENT").is_ok() {
            return Ok(InvokeOutput {
                output: OutputKind::Text("Nested subagent launch prevented for performance reasons.".to_string()),
            });
        }
        SubAgent::invoke(&self.subagents, updates).await
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::SetForegroundColor(Color::Cyan),
            style::SetAttribute(Attribute::Bold),
            style::Print(format!(
                "Launch {} Q agent(s) to perform tasks in parallel:\n\n",
                self.subagents.len()
            )),
            style::ResetColor,
            style::Print("─".repeat(50)),
            style::Print("\n\n"),
        )?;

        for agent in self.subagents.iter() {
            queue!(
                updates,
                style::SetForegroundColor(Color::Blue),
                style::Print("  • "),
                style::SetForegroundColor(Color::White),
                style::SetAttribute(Attribute::Bold),
                style::Print(&agent.agent_display_name),
                style::ResetColor,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(" ("),
                style::Print(agent.agent_cli_name.clone().unwrap_or_else(|| "Default".to_string())),
                style::Print(")\n"),
                style::ResetColor,
            )?;

            queue!(
                updates,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("    "),
                style::Print(&agent.prompt_summary),
                style::Print("\n\n"),
                style::ResetColor,
            )?;
        }

        Ok(())
    }
}

impl SubAgent {
    pub async fn invoke(agents: &[Self], updates: &mut impl Write) -> Result<InvokeOutput> {
        let prompt_template = r#"{}. SUBAGENT - You are a specialized instance delegated a task by your parent agent.
        SUBAGENT CONTEXT:
        - You are NOT the primary agent - you are a focused subprocess
        - Your parent agent is coordinating multiple subagents like you
        - Your role is to execute your specific task and report back with actionable intelligence
        - The parent agent depends on your detailed findings to make informed decisions
        - IMPORTANT: As a subagent, you are not allowed to use the launch_agent tool to avoid infinite recursion.
        
        CRITICAL REPORTING REQUIREMENTS:
        After completing your task, you MUST provide a DETAILED technical summary including:
        
        - Specific findings with concrete examples (file paths, code patterns, function names)
        - Actual implementation details and technical specifics
        - Quantifiable data (line counts, file sizes, performance metrics, etc.)
        - Key technical insights that directly inform the parent agent's next actions
        
        UNACCEPTABLE: Generic summaries like "analyzed codebase" or "completed task"
        REQUIRED: Specific technical intelligence that enables the parent agent to proceed effectively
        
        IMPORTANT: Execute your assigned subagent task, then provide your detailed technical report formatted as [SUMMARY] YOUR SUMMARY HERE [/SUMMARY]"#;

        let mut task_handles = Vec::new();
        let mut child_pids: Vec<u32> = Vec::new();
        std::fs::write("debug.log", "")?;

        // mpsc to track number of agents completed -> progress bar display
        let (progress_tx, mut progress_rx) = mpsc::channel::<u32>(agents.len());

        // Spawns a new async task for each subagent with enhanced prompt
        for agent in agents {
            let curr_prompt = prompt_template.replace("{}", &agent.prompt);
            let agent_cli_clone = agent.agent_cli_name.clone();
            let tx_clone = progress_tx.clone();
            let handle = spawn_agent_task(curr_prompt, agent_cli_clone, tx_clone).await?;
            child_pids.push(handle.0);
            task_handles.push(handle.1);
        }

        // Track completed progress with regular status updates
        drop(progress_tx);
        let mut completed = 0;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        let mut spinner: Option<Spinner> = None;
        let mut all_agents_done = false;
        let mut first_print = true;

        // Displays subagent status update every 2 seconds until join
        loop {
            tokio::select! {

                Some(_) = progress_rx.recv() => {
                    completed += 1;
                    if let Some(mut temp_spinner) = spinner.take() {
                        temp_spinner.stop();
                    }

                    // update progress spinner only when needed
                    spinner = Some(Spinner::new(Spinners::Dots,
                        format!("Progress: {}/{} agents complete", completed, agents.len())));
                    if completed >= agents.len() {
                        all_agents_done = true;
                    }
                }

                _ = interval.tick() => {

                    // Stop spinner first before any cursor operations for smoothness
                    if let Some(mut temp_spinner) = spinner.take() {
                        if !all_agents_done {
                            temp_spinner.stop();
                        }
                    }
                    updates.flush()?;

                    let mut status_output = String::new();
                    let mut new_lines_printed = 0;

                    for (i, agent) in agents.iter().enumerate() {
                        let child_pid = child_pids.get(i).unwrap_or(&0);
                        let status = match get_agent_status(*child_pid).await {
                            Ok(status) => status,
                            Err(e) => {
                                let err_msg = e.to_string();
                                if err_msg.contains("Socket not found") {
                                    "Launching agent...".to_string()
                                } else {
                                    "Agent complete...".to_string()
                                }
                            }
                        };

                        status_output.push_str(&format!(
                            "{}  • {}{}{}{} {}{}{}\n    {}{}{}\n\n",
                            style::SetForegroundColor(Color::Blue),
                            style::SetForegroundColor(Color::White),
                            style::SetAttribute(Attribute::Bold),
                            agent.agent_display_name,
                            style::ResetColor,
                            style::SetForegroundColor(Color::DarkGrey),
                            format_args!("({})", agent.agent_cli_name.clone().unwrap_or_else(|| "Default".to_string())),
                            style::ResetColor,
                            style::SetForegroundColor(Color::Cyan),
                            status,
                            style::ResetColor
                        ));

                        // 1 for agent line + 1 for status + 1 for empty line
                        new_lines_printed += 3;
                    }

                    // batch update - move cursor back to top & clear, then display everything
                    if !first_print {
                        queue!(
                                updates,
                                cursor::MoveUp(new_lines_printed as u16),
                                cursor::MoveToColumn(0),
                                Clear(ClearType::FromCursorDown),
                                style::Print(status_output)
                            )?;
                    } else {
                        queue!(
                                updates,
                                style::Print(status_output)
                            )?;
                        first_print = false;
                    }
                    updates.flush()?;

                    // force all subagents to display `Agent complete` when done...
                    if all_agents_done {
                        if let Some(mut temp_spinner) = spinner.take() {
                            temp_spinner.stop_with_message("All agents have completed.".to_string());
                        }
                        break;
                    }

                    spinner = Some(Spinner::new(Spinners::Dots,
                        format!("Progress: {}/{} agents complete", completed, agents.len())));
                }
            }
        }

        // wait till all subagents receive output
        let results = join_all(task_handles).await;
        // concatenate output + send to orchestrator
        let all_stdout = process_agent_results(results, updates)?;
        Ok(InvokeOutput {
            output: OutputKind::Text(all_stdout),
        })
    }

    /// Non-empty prompt validation
    pub async fn validate(&self, _os: &Os) -> Result<()> {
        if self.prompt.trim().is_empty() {
            return Err(eyre::eyre!("Prompt cannot be empty"));
        }
        Ok(())
    }
}

/// Uses Unix Domain Socket mechanism to query status from subagent
async fn get_agent_status(child_pid: u32) -> Result<String, eyre::Error> {
    let socket_path = format!("/tmp/qchat/{}", child_pid);
    if !std::path::Path::new(&socket_path).exists() {
        return Err(eyre::eyre!("Socket not found"));
    }

    match UnixStream::connect(&socket_path).await {
        Ok(mut stream) => {
            stream.write_all(b"LIST ").await?;
            let mut buffer = [0u8; 1024];
            let n = stream.read(&mut buffer).await?;
            if n == 0 {
                return Ok("No response".to_string());
            }
            let response_str = std::str::from_utf8(&buffer[..n]).unwrap_or("<invalid utf8>");
            match serde_json::from_str::<serde_json::Value>(response_str) {
                Ok(json) => {
                    let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("Running");
                    let tokens = json.get("tokens_used").and_then(|v| v.as_u64()).unwrap_or(0);
                    Ok(format!("{} - {} tokens used", status, tokens))
                },
                Err(_) => Err(eyre::eyre!("JSON parsing error.")),
            }
        },
        Err(_) => Err(eyre::eyre!("Stream connection error.")),
    }
}

/// Runs a q subagent process as an async tokio task with specified prompt and model
async fn spawn_agent_task(
    prompt: String,
    agent_cli_name: Option<String>,
    tx: tokio::sync::mpsc::Sender<u32>,
) -> Result<(u32, tokio::task::JoinHandle<Result<String, eyre::Error>>), eyre::Error> {
    // Run subagent with desired agent config + Q_SUBAGENT env var = 1
    let mut cmd = tokio::process::Command::new(current_exe().unwrap());
    cmd.arg("chat");
    if let Some(agent_arg) = agent_cli_name {
        cmd.arg(format!("--agent={}", agent_arg));
    }
    cmd.arg("--no-interactive");
    cmd.arg(prompt);
    cmd.env("Q_SUBAGENT", "1");

    let debug_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("debug.log")?;
    let debug_log_stderr = debug_log.try_clone()?;

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(std::process::Stdio::from(debug_log_stderr))
        .stdin(std::process::Stdio::null())
        .spawn()?;
    let child_pid = child
        .id()
        .ok_or_else(|| std::io::Error::other("Failed to get child PID"))?;

    // Allows extraction of child_pid before waiting on completion for status update
    let handle = tokio::spawn(async move {
        let output = capture_stdout_and_log(child.stdout.take().unwrap(), debug_log).await?;
        let _ = child.wait().await?;
        let _ = tx.send(1).await;
        Ok(output)
    });

    Ok((child_pid, handle))
}

/// Formats and joins all subagent summaries with error printing for user
fn process_agent_results(
    results: Vec<Result<Result<String, eyre::Error>, tokio::task::JoinError>>,
    updates: &mut impl Write,
) -> Result<String, eyre::Error> {
    let mut all_stdout = String::new();
    let mut i = 1;
    for task_result in results {
        match task_result {
            Ok(Ok(stdout_output)) => {
                if !stdout_output.trim().is_empty() {
                    all_stdout.push_str(&format!("=== Agent {} Output ===\n", i));
                    all_stdout.push_str(&stdout_output);
                    all_stdout.push_str("\n\n");
                    i += 1;
                }
            },
            Ok(Err(e)) => {
                queue!(
                    updates,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("Failed to launch agent: {}\n", e)),
                    style::ResetColor,
                )?;
            },
            Err(e) => {
                queue!(
                    updates,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("Task join error: {}\n", e)),
                    style::ResetColor,
                )?;
            },
        }
    }
    Ok(all_stdout)
}

/// Async function that captures stdout from a reader and extracts summary only from stdout
/// Reads until STDOUT pipe closes i.e. when child process exits
async fn capture_stdout_and_log(
    stdout: tokio::process::ChildStdout,
    mut debug_log: std::fs::File,
) -> Result<String, eyre::Error> {
    let mut reader = tokio::io::BufReader::new(stdout);
    let mut output = String::new();
    let mut line = String::new();

    // If no SUMMARY tag in response, pass whole response as summary to orchestrator
    while reader.read_line(&mut line).await? > 0 {
        writeln!(debug_log, "{}", line.trim_end())?;
        output.push_str(&line);
        line.clear();
    }
    let re: regex::Regex = regex::Regex::new(r"(?is)\[SUMMARY\]\s*(.*?)\s*\[/SUMMARY\]").unwrap();
    if let Some(captures) = re.captures(&output) {
        if let Some(summary) = captures.get(1) {
            return Ok(summary.as_str().trim().to_string());
        }
    }
    Ok(output)
}
