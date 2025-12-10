use std::io::Write;

use clap::Subcommand;
use crossterm::queue;
use crossterm::style::{
    self,
};
use eyre::Result;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::feature_flags::FeatureFlags;
use crate::os::Os;
use crate::theme::StyledText;

/// Code intelligence commands using LSP servers
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum CodeSubcommand {
    /// Show workspace status (default if no subcommand provided)
    Status,
    /// Initialize workspace and start LSP servers
    Init {
        /// Force re-initialization even if already initialized
        #[arg(short, long)]
        force: bool,
    },
    /// Display LSP logs with filtering
    Logs {
        /// Log level filter (ERROR, WARN, INFO, DEBUG, TRACE). Default: ERROR
        #[arg(short, long, default_value = "ERROR")]
        level: String,
        /// Number of log lines to display. Default: 20
        #[arg(short = 'n', long, default_value = "20")]
        lines: usize,
        /// Export logs to JSON file at specified path
        #[arg(short, long)]
        path: Option<std::path::PathBuf>,
    },
}

impl CodeSubcommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Init { .. } => "init",
            Self::Logs { .. } => "logs",
        }
    }

    pub async fn execute(&self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Check if code intelligence feature is enabled
        if !FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            use crossterm::style::{
                Color,
                Stylize,
            };
            let error_msg = format!(
                "{}: unrecognized subcommand {}",
                "error".with(Color::Red),
                format!("'{}'", self.name()).with(Color::Yellow)
            );
            return Err(ChatError::Custom(error_msg.into()));
        }

        // Check if code tool is in the agent's tools list
        use crate::cli::chat::tools::ToolMetadata;

        if !session.conversation.agents.has_tool(ToolMetadata::CODE.aliases) {
            queue!(
                session.stderr,
                StyledText::error_fg(),
                style::Print("Code tool is not enabled for this agent\n"),
                StyledText::reset(),
            )?;
            session.stderr.flush()?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        match self {
            Self::Status => self.show_workspace_status(os, session, false, false).await,
            Self::Init { force } => self.show_workspace_status(os, session, true, *force).await,
            Self::Logs { level, lines, path } => self.show_logs(session, level, *lines, path.clone()).await,
        }
    }

    async fn show_workspace_status(
        &self,
        os: &mut Os,
        session: &mut ChatSession,
        initialize: bool,
        force: bool,
    ) -> Result<ChatState, ChatError> {
        // Check if feature is enabled but client wasn't initialized at startup
        if session.conversation.code_intelligence_client.is_none() && FeatureFlags::CODE_INTELLIGENCE_ENABLED {
            queue!(
                session.stderr,
                StyledText::error_fg(),
                style::Print("Code intelligence feature was enabled after chat started.\n"),
                style::Print("Please restart the chat session to use code intelligence features.\n"),
                StyledText::reset(),
            )?;
            session.stderr.flush()?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        // Track if we need to reload tools after initialization
        let mut should_reload_tools = false;

        if let Some(code_client_lock) = &session.conversation.code_intelligence_client {
            let mut code_client = code_client_lock.write().await;

            // Force re-initialization if requested
            if force && code_client.workspace_status() != code_agent_sdk::sdk::WorkspaceStatus::NotInitialized {
                code_client.reset_initialization().await;
            }

            // Handle initialization based on current status and whether init was requested
            match code_client.workspace_status() {
                code_agent_sdk::sdk::WorkspaceStatus::NotInitialized => {
                    if initialize {
                        match code_client.initialize().await {
                            Ok(_) => {
                                should_reload_tools = true;

                                queue!(
                                    session.stderr,
                                    StyledText::success_fg(),
                                    style::Print("✓ Workspace initialization started\n\n"),
                                    StyledText::reset(),
                                )?;
                            },
                            Err(e) => {
                                queue!(
                                    session.stderr,
                                    StyledText::error_fg(),
                                    style::Print("Failed to initialize workspace: "),
                                    StyledText::reset(),
                                    style::Print(format!("{e}\n")),
                                )?;
                                session.stderr.flush()?;
                                return Ok(ChatState::PromptUser {
                                    skip_printing_tools: true,
                                });
                            },
                        }
                    } else {
                        queue!(
                            session.stderr,
                            StyledText::warning_fg(),
                            style::Print("⚠ Workspace not initialized. Run `/code init` to initialize.\n\n"),
                            StyledText::reset(),
                        )?;
                    }
                },
                code_agent_sdk::sdk::WorkspaceStatus::Initializing => {
                    queue!(
                        session.stderr,
                        StyledText::warning_fg(),
                        style::Print("◐ Workspace initialization in progress...\n\n"),
                        StyledText::reset(),
                    )?;
                },
                code_agent_sdk::sdk::WorkspaceStatus::Initialized => {
                    if initialize {
                        queue!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Workspace already initialized\n\n"),
                            StyledText::reset(),
                        )?;
                    } else {
                        queue!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print("✓ Workspace initialized\n\n"),
                            StyledText::reset(),
                        )?;
                    }
                },
            }

            // Show workspace info
            match code_client.detect_workspace() {
                Ok(workspace_info) => {
                    queue!(
                        session.stderr,
                        StyledText::brand_fg(),
                        style::Print("Workspace: "),
                        StyledText::reset(),
                        style::Print(format!("{}\n", workspace_info.root_path.display())),
                    )?;

                    queue!(
                        session.stderr,
                        StyledText::brand_fg(),
                        style::Print("Detected Languages: "),
                        StyledText::reset(),
                        style::Print(format!("{:?}\n", workspace_info.detected_languages)),
                    )?;

                    if !workspace_info.project_markers.is_empty() {
                        queue!(
                            session.stderr,
                            StyledText::brand_fg(),
                            style::Print("Project Markers: "),
                            StyledText::reset(),
                            style::Print(format!("{:?}\n", workspace_info.project_markers)),
                        )?;
                    }

                    queue!(
                        session.stderr,
                        StyledText::brand_fg(),
                        style::Print("\nAvailable LSPs:\n"),
                        StyledText::reset(),
                    )?;

                    // Check which languages are detected in this workspace
                    let detected_langs: std::collections::HashSet<String> =
                        workspace_info.detected_languages.iter().cloned().collect();

                    let workspace_initialized =
                        code_client.workspace_status() == code_agent_sdk::sdk::WorkspaceStatus::Initialized;

                    // Sort LSPs alphabetically by name for consistent display
                    let mut lsp_indices: Vec<usize> = (0..workspace_info.available_lsps.len()).collect();
                    lsp_indices.sort_by_key(|&i| &workspace_info.available_lsps[i].name);

                    let mut has_failed_lsp = false;
                    for &idx in &lsp_indices {
                        let lsp = &workspace_info.available_lsps[idx];
                        // Determine if this LSP is relevant (supports detected languages)
                        let is_relevant = lsp.languages.iter().any(|lang| detected_langs.contains(lang));

                        // Use status field if available, otherwise fall back to is_initialized
                        let (symbol, status_text, color) = match lsp.status.as_deref() {
                            Some("initialized") => {
                                let time_str = lsp.init_duration_ms.map(format_duration_ms).unwrap_or_default();
                                ("✓", format!("initialized{time_str}"), StyledText::success_fg())
                            },
                            Some("initializing" | "registered") if lsp.is_available && is_relevant => {
                                ("◐", "initializing...".to_string(), StyledText::warning_fg())
                            },
                            Some(s) if s.starts_with("failed:") => {
                                has_failed_lsp = true;
                                ("✗", "failed to initialize".to_string(), StyledText::error_fg())
                            },
                            _ if lsp.is_initialized => {
                                let time_str = lsp.init_duration_ms.map(format_duration_ms).unwrap_or_default();
                                ("✓", format!("initialized{time_str}"), StyledText::success_fg())
                            },
                            _ if lsp.is_available && is_relevant && workspace_initialized => {
                                // Only mark as failed if workspace was initialized but LSP didn't start
                                has_failed_lsp = true;
                                ("✗", "failed to initialize".to_string(), StyledText::error_fg())
                            },
                            _ if !lsp.is_available => ("○", "not installed".to_string(), StyledText::secondary_fg()),
                            _ => ("○", "available".to_string(), StyledText::secondary_fg()),
                        };

                        queue!(
                            session.stderr,
                            style::Print(format!("{symbol} ")),
                            style::Print(format!("{} ", lsp.name)),
                            StyledText::secondary_fg(),
                            style::Print(format!("({})", lsp.languages.join(", "))),
                            StyledText::reset(),
                            style::Print(" - "),
                            color,
                            style::Print(&status_text),
                            StyledText::reset(),
                            style::Print("\n"),
                        )?;

                        // Show workspace folders if any
                        if !lsp.workspace_folders.is_empty() {
                            for folder in &lsp.workspace_folders {
                                let path = folder.strip_prefix("file://").unwrap_or(folder);
                                queue!(
                                    session.stderr,
                                    style::Print("    "),
                                    StyledText::secondary_fg(),
                                    style::Print(format!("• {path}\n")),
                                    StyledText::reset(),
                                )?;
                            }
                        }
                    }

                    if has_failed_lsp && workspace_initialized {
                        queue!(
                            session.stderr,
                            style::Print("\n"),
                            StyledText::secondary_fg(),
                            style::Print("Run /code logs to troubleshoot LSP failures\n"),
                            StyledText::reset(),
                        )?;
                    }
                },
                Err(e) => {
                    queue!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print("Failed to detect workspace: "),
                        StyledText::reset(),
                        style::Print(format!("{e}\n")),
                    )?;
                },
            }
        } else {
            queue!(
                session.stderr,
                StyledText::warning_fg(),
                style::Print("Code intelligence client not initialized\n"),
                StyledText::reset(),
                style::Print("Use a code tool to initialize the client automatically\n"),
            )?;
        }

        // Show config file location
        queue!(
            session.stderr,
            style::Print("\n"),
            StyledText::secondary_fg(),
            style::Print("Configuration can be updated at "),
            StyledText::reset(),
            StyledText::info_fg(),
            style::Print(".kiro/settings/lsp.json"),
            StyledText::reset(),
            style::Print("\n"),
        )?;

        session.stderr.flush()?;

        // Reload tools after initialization so code tool becomes available to model
        if should_reload_tools {
            session.reload_builtin_tools(os).await?;
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn show_logs(
        &self,
        session: &mut ChatSession,
        level: &str,
        lines: usize,
        path: Option<std::path::PathBuf>,
    ) -> Result<ChatState, ChatError> {
        use crate::util::paths::logs_dir;

        if level_priority(level) == 0 {
            return show_invalid_level_error(session, level);
        }

        let log_path = logs_dir()
            .map_err(|e| ChatError::Custom(format!("Failed to get logs dir: {e}").into()))?
            .join("lsp.log");

        if !log_path.exists() {
            return show_no_logs_warning(session);
        }

        let entries = read_and_filter_logs(&log_path, level)?;
        let display_entries = get_last_n_entries(&entries, lines);

        if let Some(export_path) = path {
            export_logs_to_file(session, &display_entries, &export_path)?;
        } else {
            display_logs_to_stderr(session, &display_entries, level)?;
        }

        session.stderr.flush()?;
        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }
}

#[derive(serde::Serialize)]
struct LogEntry {
    timestamp: String,
    level: String,
    message: String,
}

fn parse_log_line(line: &str) -> Option<LogEntry> {
    // Format: 2025-11-27T23:18:26.165059Z DEBUG code_agent_sdk::sdk::workspace_manager: 288: Message
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // Strip ANSI codes
    let clean = strip_ansi(line);

    let parts: Vec<&str> = clean.splitn(4, ' ').collect();
    if parts.len() < 4 {
        return None;
    }

    let timestamp = parts[0].to_string();
    let level = parts[1].to_string();

    // Strip source metadata and line number: "module::path: line_num: Message"
    // Result should be just the message without "536: " prefix
    let rest = parts[3..].join(" ");
    let mut message = rest.clone();

    // Find ": <number>: " pattern and extract message after it
    let bytes = rest.as_bytes();
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i] == b':' && bytes[i + 1] == b' ' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 && j + 1 < bytes.len() && bytes[j] == b':' && bytes[j + 1] == b' ' {
                message = rest[j + 2..].to_string();
                break;
            }
        }
    }

    // Also strip leading "line_num: " if message still starts with digits followed by ": "
    let msg_bytes = message.as_bytes();
    let mut k = 0;
    while k < msg_bytes.len() && msg_bytes[k].is_ascii_digit() {
        k += 1;
    }
    if k > 0 && k + 1 < msg_bytes.len() && msg_bytes[k] == b':' && msg_bytes[k + 1] == b' ' {
        message = message[k + 2..].to_string();
    }

    Some(LogEntry {
        timestamp,
        level,
        message,
    })
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        result.push(c);
    }
    result
}

fn show_invalid_level_error(session: &mut ChatSession, level: &str) -> Result<ChatState, ChatError> {
    queue!(
        session.stderr,
        StyledText::error_fg(),
        style::Print(format!(
            "Invalid log level '{level}'. Valid levels: ERROR, WARN, INFO, DEBUG, TRACE\n"
        )),
        StyledText::reset(),
    )?;
    session.stderr.flush()?;
    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

fn show_no_logs_warning(session: &mut ChatSession) -> Result<ChatState, ChatError> {
    queue!(
        session.stderr,
        StyledText::warning_fg(),
        style::Print("No LSP logs found. Run /code init first.\n"),
        StyledText::reset(),
    )?;
    session.stderr.flush()?;
    Ok(ChatState::PromptUser {
        skip_printing_tools: true,
    })
}

fn read_and_filter_logs(log_path: &std::path::Path, level: &str) -> Result<Vec<LogEntry>, ChatError> {
    let content = std::fs::read_to_string(log_path)
        .map_err(|e| ChatError::Custom(format!("Failed to read log file: {e}").into()))?;
    let min_level = level_priority(level);
    Ok(content
        .lines()
        .filter_map(parse_log_line)
        .filter(|entry| level_priority(&entry.level) >= min_level)
        .collect())
}

fn get_last_n_entries(entries: &[LogEntry], n: usize) -> Vec<&LogEntry> {
    entries
        .iter()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn export_logs_to_file(
    session: &mut ChatSession,
    entries: &[&LogEntry],
    export_path: &std::path::Path,
) -> Result<(), ChatError> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| ChatError::Custom(format!("Failed to serialize: {e}").into()))?;
    std::fs::write(export_path, json).map_err(|e| ChatError::Custom(format!("Failed to write file: {e}").into()))?;
    queue!(
        session.stderr,
        StyledText::success_fg(),
        style::Print(format!(
            "✓ Exported {} logs to {}\n",
            entries.len(),
            export_path.display()
        )),
        StyledText::reset(),
    )?;
    Ok(())
}

fn display_logs_to_stderr(session: &mut ChatSession, entries: &[&LogEntry], level: &str) -> Result<(), ChatError> {
    for entry in entries {
        let level_style = match entry.level.as_str() {
            "ERROR" => StyledText::error_fg(),
            "WARN" => StyledText::warning_fg(),
            "INFO" => StyledText::success_fg(),
            "DEBUG" | "TRACE" => StyledText::info_fg(),
            _ => StyledText::secondary_fg(),
        };
        queue!(
            session.stderr,
            StyledText::secondary_fg(),
            style::Print(&entry.timestamp),
            style::Print(" "),
            level_style,
            style::Print(format!("{:5}", entry.level)),
            StyledText::reset(),
            style::Print(format!(" {}\n", entry.message)),
        )?;
    }
    if entries.is_empty() {
        queue!(
            session.stderr,
            StyledText::secondary_fg(),
            style::Print(format!("No logs at {level} level or above\n")),
            StyledText::reset(),
        )?;
    }
    Ok(())
}

/// Format duration in milliseconds to human-readable string
fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!(" ({ms}ms)")
    } else {
        format!(" ({:.1}s)", ms as f64 / 1000.0)
    }
}

fn level_priority(level: &str) -> u8 {
    match level.to_uppercase().as_str() {
        "ERROR" => 5,
        "WARN" => 4,
        "INFO" => 3,
        "DEBUG" => 2,
        "TRACE" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_line() {
        let line =
            "2025-11-27T23:18:26.165059Z DEBUG code_agent_sdk::sdk::workspace_manager: 288: Background task received";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.timestamp, "2025-11-27T23:18:26.165059Z");
        assert_eq!(entry.level, "DEBUG");
        // Message includes line number since there's no space after the colon in this format
        assert!(entry.message.contains("Background task received"));
    }

    #[test]
    fn test_parse_log_line_with_ansi() {
        let line = "\x1b[2m2025-11-27T23:18:26.165059Z\x1b[0m \x1b[34mDEBUG\x1b[0m \x1b[2mcode_agent_sdk::lsp::client\x1b[0m\x1b[2m:\x1b[0m \x1b[2m395:\x1b[0m Processing notification";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.level, "DEBUG");
        assert!(entry.message.contains("Processing"));
    }

    #[test]
    fn test_level_priority() {
        assert!(level_priority("ERROR") > level_priority("WARN"));
        assert!(level_priority("WARN") > level_priority("INFO"));
        assert!(level_priority("INFO") > level_priority("DEBUG"));
        assert!(level_priority("DEBUG") > level_priority("TRACE"));
        assert_eq!(level_priority("error"), level_priority("ERROR"));
    }

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[2m2025-11-27\x1b[0m \x1b[34mDEBUG\x1b[0m";
        let output = strip_ansi(input);
        assert_eq!(output, "2025-11-27 DEBUG");
    }
}
