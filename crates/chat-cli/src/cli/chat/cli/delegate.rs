use std::collections::HashMap;
use std::io::{
    Write,
    stdin,
    stdout,
};
use std::path::PathBuf;
use std::process::Stdio;

use crossterm::execute;
use crossterm::style::{
    Color,
    Print,
    SetForegroundColor,
};
use eyre::{
    Result,
    bail,
    eyre,
};
use serde::{
    Deserialize,
    Serialize,
};
use time::OffsetDateTime;
use tokio::process::Command;
use uuid::Uuid;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::directories::home_dir;

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
        /// Specific task UUID (optional)
        uuid: Option<String>,
    },
    /// Read output from a task
    Read {
        /// Task UUID
        uuid: String,
    },
    /// Delete a task and its files
    Delete {
        /// Task UUID
        uuid: String,
    },
    /// List all tasks
    List,
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

        let result = match self {
            DelegateArgs::Status { uuid } => show_status(os, uuid.as_deref()).await,
            DelegateArgs::Read { uuid } => {
                // For read command, we want to trigger LLM analysis
                let full_uuid = match find_subagent_by_partial_uuid(os, &uuid).await {
                    Ok(Some(uuid)) => uuid,
                    Ok(None) => return Err(ChatError::Custom("Subagent not found".into())),
                    Err(e) => return Err(ChatError::Custom(e.to_string().into())),
                };

                let content_path = match get_subagents_dir(os).await {
                    Ok(dir) => dir.join(format!("{}.json", full_uuid)),
                    Err(e) => return Err(ChatError::Custom(e.to_string().into())),
                };

                // Print the basic info first
                match read_subagent(os, &uuid, session).await {
                    Ok(message) => println!("{}", message),
                    Err(e) => return Err(ChatError::Custom(e.to_string().into())),
                }

                // Return HandleInput state to trigger LLM analysis
                return Ok(ChatState::HandleInput {
                    input: format!(
                        "Please read and summarize the subagent results from this file: {}\n\nProvide a concise 2-3 bullet point summary of what was accomplished.",
                        content_path.display()
                    ),
                });
            },
            DelegateArgs::Delete { uuid } => {
                let full_uuid = match find_subagent_by_partial_uuid(os, &uuid).await {
                    Ok(Some(uuid)) => uuid,
                    Ok(None) => return Err(ChatError::Custom("Task not found".into())),
                    Err(e) => return Err(ChatError::Custom(e.to_string().into())),
                };

                delete_subagent(os, &full_uuid).await
            },
            DelegateArgs::List => list_subagents(os).await,
            DelegateArgs::Launch { agent, prompt } => {
                let prompt_str = prompt.join(" ");
                if prompt_str.trim().is_empty() {
                    return Err(ChatError::Custom("Please provide a prompt for the task".into()));
                }

                launch_subagent(os, session, agent.as_deref(), &prompt_str).await
            },
        };

        match result {
            Ok(output) => {
                writeln!(session.stderr, "{}", output)?;
            },
            Err(e) => {
                writeln!(session.stderr, "Error: {}", e)?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}

fn is_enabled(os: &Os) -> bool {
    os.database.settings.get_bool(Setting::EnabledDelegate).unwrap_or(false)
}

async fn get_subagents_dir(os: &Os) -> Result<PathBuf> {
    let home = home_dir(os)?;
    let dir = home.join(".aws").join("amazonq").join(".subagents");
    os.fs.create_dir_all(&dir).await?;
    Ok(dir)
}

async fn load_status_file(os: &Os) -> Result<StatusFile> {
    let status_path = get_subagents_dir(os).await?.join("status.json");

    if !os.fs.exists(&status_path) {
        return Ok(StatusFile::default());
    }

    let content = os.fs.read_to_string(&status_path).await?;
    serde_json::from_str(&content).map_err(|e| eyre!("Failed to parse status file: {}", e))
}

async fn save_status_file(os: &Os, status: &StatusFile) -> Result<()> {
    let status_path = get_subagents_dir(os).await?.join("status.json");
    let content = serde_json::to_string_pretty(status)?;
    os.fs.write(&status_path, content).await?;
    Ok(())
}

async fn launch_subagent(os: &Os, session: &ChatSession, agent: Option<&str>, prompt: &str) -> Result<String> {
    // If agent is specified, show details and ask for approval
    if let Some(agent_name) = agent {
        // Load agent configuration
        let agent_config = match session.conversation.agents.agents.get(agent_name) {
            Some(agent) => agent,
            None => {
                return Err(eyre!("Agent '{}' not found", agent_name));
            },
        };

        // Get short description (first sentence or truncate)
        let description = agent_config.description.as_deref().unwrap_or("No description");
        let short_desc = if let Some(pos) = description.find('.') {
            &description[..pos + 1]
        } else if description.len() > 60 {
            &format!("{}...", &description[..57])
        } else {
            description
        };

        // Display agent details and ask for approval
        execute!(
            stdout(),
            Print(format!("Agent: {}\n", agent_name)),
            Print(format!("Description: {}\n", short_desc)),
            Print(format!("Task: {}\n", prompt)),
        )?;

        // Show allowed tools if any
        if !agent_config.allowed_tools.is_empty() {
            let tools: Vec<&str> = agent_config.allowed_tools.iter().map(|s| s.as_str()).collect();
            execute!(stdout(), Print(format!("Tools: {}\n", tools.join(", "))),)?;
        }

        execute!(
            stdout(),
            Print("\n"),
            SetForegroundColor(Color::Yellow),
            Print(
                "⚠️  This task will run with trust-all permissions and can execute commands or consume system/cloud resources. Continue? [y/N]: "
            ),
            SetForegroundColor(Color::Reset),
        )?;

        stdout().flush()?;

        let mut input = String::new();
        stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            return Ok("❌ Task delegation cancelled by user.".to_string());
        }

        println!(); // Add blank line after approval
    }

    let uuid = Uuid::new_v4().to_string();
    let now = OffsetDateTime::now_utc().to_string();

    // Show warning only for non-agent case (no approval was shown)
    if agent.is_none() {
        execute!(
            stdout(),
            SetForegroundColor(Color::Yellow),
            Print(
                "WARNING: Tasks run with trust-all permissions and can execute commands or consume system/cloud resources without approval.\n\n"
            ),
            SetForegroundColor(Color::Reset),
        )?;
    }

    let mut cmd = Command::new("q");
    cmd.arg("chat").arg("--non-interactive").arg("--trust-all-tools");

    if let Some(agent) = agent {
        cmd.arg("--agent").arg(agent);
    }

    cmd.arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);

    let header = SubagentHeader {
        launched_at: now.clone(),
        agent: agent.map(|s| s.to_string()),
        prompt: prompt.to_string(),
        status: "active".to_string(),
        pid,
        completed_at: None,
    };

    let mut status = load_status_file(os).await?;
    status.subagents.insert(uuid.clone(), header);
    status.last_updated = now;
    save_status_file(os, &status).await?;

    let content = SubagentContent::default();
    let content_path = get_subagents_dir(os).await?.join(format!("{}.json", uuid));
    let content_json = serde_json::to_string_pretty(&content)?;
    os.fs.write(&content_path, content_json).await?;

    tokio::spawn(monitor_subagent(uuid.clone(), child, os.clone()));

    let agent_info = agent.map(|a| format!(" with agent '{}'", a)).unwrap_or_default();
    Ok(format!(
        "✅ Task launched{}\nTask: {}\nUUID: {} | Use '/delegate status {}' to check progress",
        agent_info,
        prompt,
        uuid,
        &uuid[..8]
    ))
}

async fn monitor_subagent(uuid: String, child: tokio::process::Child, os: Os) {
    let result = async {
        let output = child.wait_with_output().await?;

        let combined_output = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let content = SubagentContent {
            output: combined_output,
            exit_code: output.status.code(),
        };

        let content_path = get_subagents_dir(&os).await?.join(format!("{}.json", uuid));
        let content_json = serde_json::to_string_pretty(&content)?;
        os.fs.write(&content_path, content_json).await?;

        let mut status = load_status_file(&os).await?;
        if let Some(header) = status.subagents.get_mut(&uuid) {
            header.status = if output.status.success() { "completed" } else { "failed" }.to_string();
            header.completed_at = Some(OffsetDateTime::now_utc().to_string());
        }
        status.last_updated = OffsetDateTime::now_utc().to_string();
        save_status_file(&os, &status).await?;

        Ok::<(), eyre::Error>(())
    }
    .await;

    if let Err(e) = result {
        eprintln!("Error monitoring subagent {}: {}", uuid, e);
    }
}

async fn find_subagent_by_partial_uuid(os: &Os, partial_uuid: &str) -> Result<Option<String>> {
    let status = load_status_file(os).await?;

    let matches: Vec<String> = status
        .subagents
        .keys()
        .filter(|uuid| uuid.starts_with(partial_uuid))
        .cloned()
        .collect();

    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches[0].clone())),
        _ => bail!(
            "Ambiguous UUID '{}' matches multiple subagents: {}",
            partial_uuid,
            matches.join(", ")
        ),
    }
}

async fn show_status(os: &Os, uuid: Option<&str>) -> Result<String> {
    let status = load_status_file(os).await?;

    if let Some(partial_uuid) = uuid {
        let full_uuid = match find_subagent_by_partial_uuid(os, partial_uuid).await? {
            Some(uuid) => uuid,
            None => return Ok(format!("❌ Subagent {} not found", partial_uuid)),
        };

        if let Some(header) = status.subagents.get(&full_uuid) {
            let agent_info = header
                .agent
                .as_ref()
                .map(|a| format!("Agent: {}\n", a))
                .unwrap_or_default();

            Ok(format!(
                "📊 Subagent Status: {}\n{}🆔 UUID: {}\n📋 Task: {}\n⏰ Launched: {}{}",
                header.status.to_uppercase(),
                agent_info,
                &full_uuid[..8], // Show short UUID for readability
                header.prompt,
                header.launched_at,
                header
                    .completed_at
                    .as_ref()
                    .map(|t| format!("\n✅ Completed: {}", t))
                    .unwrap_or_default()
            ))
        } else {
            Ok(format!("❌ Subagent {} not found", partial_uuid))
        }
    } else {
        let active_count = status.subagents.values().filter(|h| h.status == "active").count();
        let completed_count = status.subagents.values().filter(|h| h.status == "completed").count();
        let failed_count = status.subagents.values().filter(|h| h.status == "failed").count();

        Ok(format!(
            "📊 Subagent Summary:\n🟢 Active: {}\n✅ Completed: {}\n❌ Failed: {}\n📈 Total: {}",
            active_count,
            completed_count,
            failed_count,
            status.subagents.len()
        ))
    }
}

async fn read_subagent(os: &Os, partial_uuid: &str, _session: &mut ChatSession) -> Result<String> {
    let full_uuid = match find_subagent_by_partial_uuid(os, partial_uuid).await? {
        Some(uuid) => uuid,
        None => bail!("Subagent {} not found", partial_uuid),
    };

    let content_path = get_subagents_dir(os).await?.join(format!("{}.json", full_uuid));

    if !os.fs.exists(&content_path) {
        bail!("Subagent {} not found", partial_uuid);
    }

    let content_json = os.fs.read_to_string(&content_path).await?;
    let content: SubagentContent = serde_json::from_str(&content_json)?;

    // Get subagent info for context
    let status = load_status_file(os).await?;
    let header = status.subagents.get(&full_uuid).unwrap();

    if content.output.trim().is_empty() {
        return Ok(format!(
            "Task {} Output:\n\nNo output yet - task may still be running.",
            &full_uuid[..8]
        ));
    }

    Ok(format!(
        "Task {} Results\n\nTask: {}\nStatus: {} (Exit Code: {})",
        &full_uuid[..8],
        header.prompt,
        header.status.to_uppercase(),
        content.exit_code.map_or("N/A".to_string(), |c| c.to_string())
    ))
}

async fn delete_subagent(os: &Os, uuid: &str) -> Result<String> {
    let subagents_dir = get_subagents_dir(os).await?;
    let content_path = subagents_dir.join(format!("{}.json", uuid));

    // Check if task file exists
    if !content_path.exists() {
        return Err(eyre!("Task file not found: {}", uuid));
    }

    // Remove the task file
    std::fs::remove_file(&content_path).map_err(|e| eyre!("Failed to delete task file: {}", e))?;

    // Remove from status.json
    let mut status = load_status_file(os).await?;
    if status.subagents.remove(uuid).is_some() {
        status.last_updated = time::OffsetDateTime::now_utc().to_string();
        save_status_file(os, &status).await?;
        Ok(format!("✅ Task deleted: {}", &uuid[..8]))
    } else {
        Ok(format!("⚠️  Task file deleted but not found in status: {}", &uuid[..8]))
    }
}

async fn list_subagents(os: &Os) -> Result<String> {
    let status = load_status_file(os).await?;

    if status.subagents.is_empty() {
        return Ok("📋 No tasks found".to_string());
    }

    let mut result = String::from("📋 Tasks:\n\n");

    // Sort by timestamp (newest first)
    let mut sorted_subagents: Vec<_> = status.subagents.iter().collect();
    sorted_subagents.sort_by(|a, b| {
        let time_a = a.1.completed_at.as_ref().unwrap_or(&a.1.launched_at);
        let time_b = b.1.completed_at.as_ref().unwrap_or(&b.1.launched_at);
        time_b.cmp(time_a) // Reverse order for newest first
    });

    for (uuid, header) in sorted_subagents {
        let status_icon = match header.status.as_str() {
            "active" => "🟢",
            "completed" => "✅",
            "failed" => "❌",
            _ => "❓",
        };

        let agent_info = header.agent.as_ref().map(|a| format!(" [{}]", a)).unwrap_or_default();

        // Format timestamp - use completed_at if available, otherwise launched_at
        let timestamp_str = header.completed_at.as_ref().unwrap_or(&header.launched_at);
        let timestamp = if timestamp_str.len() >= 16 {
            // Extract YYYY-MM-DD HH:MM from YYYY-MM-DDTHH:MM:SS.microseconds+timezone
            format!("{} {}", &timestamp_str[0..10], &timestamp_str[11..16])
        } else {
            timestamp_str.clone()
        };

        // Truncate prompt at word boundary
        let truncated_prompt = if header.prompt.len() > 50 {
            let mut truncated = String::new();
            let mut char_count = 0;
            for word in header.prompt.split_whitespace() {
                if char_count + word.len() + 1 > 50 {
                    break;
                }
                if !truncated.is_empty() {
                    truncated.push(' ');
                    char_count += 1;
                }
                truncated.push_str(word);
                char_count += word.len();
            }
            if truncated.len() < header.prompt.len() {
                truncated.push_str("...");
            }
            truncated
        } else {
            header.prompt.clone()
        };

        result.push_str(&format!(
            "{} {} | {}{} | {} | {}\n",
            status_icon,
            &uuid[..8],
            header.status.to_uppercase(),
            agent_info,
            timestamp,
            truncated_prompt
        ));
    }

    Ok(result.trim_end().to_string())
}
