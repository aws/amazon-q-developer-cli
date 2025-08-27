use std::io::Write;

use clap::Subcommand;
use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use semantic_search_client::{
    OperationStatus,
    SystemStatus,
};

use crate::cli::chat::tools::sanitize_path_tool_arg;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::knowledge_store::KnowledgeStore;

/// Knowledge base management commands
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum KnowledgeSubcommand {
    /// Display the knowledge base contents
    Show,
    /// Add a file or directory to knowledge base
    Add {
        path: String,
        /// Include patterns (e.g., `**/*.ts`, `**/*.md`)
        #[arg(long, action = clap::ArgAction::Append)]
        include: Vec<String>,
        /// Exclude patterns (e.g., `node_modules/**`, `target/**`)
        #[arg(long, action = clap::ArgAction::Append)]
        exclude: Vec<String>,
        /// Index type to use (Fast, Best)
        #[arg(long)]
        index_type: Option<String>,
    },
    /// Remove specified knowledge base entry by path
    #[command(alias = "rm")]
    Remove { path: String },
    /// Update a file or directory in knowledge base
    Update { path: String },
    /// Remove all knowledge base entries
    Clear,
    /// Show background operation status
    Status,
    /// Cancel a background operation
    Cancel {
        /// Operation ID to cancel (optional - cancels most recent if not provided)
        operation_id: Option<String>,
    },
}

#[derive(Debug)]
enum OperationResult {
    Success(String),
    Info(String),
    Warning(String),
    Error(String),
}

impl KnowledgeSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if !Self::is_feature_enabled(os) {
            Self::write_feature_disabled_message(session)?;
            return Ok(Self::default_chat_state());
        }

        let result = self.execute_operation(os, session).await;

        Self::write_operation_result(session, result)?;

        Ok(Self::default_chat_state())
    }

    fn is_feature_enabled(os: &Os) -> bool {
        os.database
            .settings
            .get_bool(Setting::EnabledKnowledge)
            .unwrap_or(false)
    }

    fn write_feature_disabled_message(session: &mut ChatSession) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("\nKnowledge tool is disabled. Enable it with: q settings chat.enableKnowledge true\n"),
            style::SetForegroundColor(Color::Yellow),
            style::Print("💡 Your knowledge base data is preserved and will be available when re-enabled.\n\n"),
            style::SetForegroundColor(Color::Reset)
        )
    }

    fn default_chat_state() -> ChatState {
        ChatState::PromptUser {
            skip_printing_tools: true,
        }
    }

    /// Get the current agent from the session
    fn get_agent(session: &ChatSession) -> Option<&crate::cli::Agent> {
        session.conversation.agents.get_active()
    }

    async fn execute_operation(&self, os: &Os, session: &mut ChatSession) -> OperationResult {
        match self {
            KnowledgeSubcommand::Show => {
                match Self::handle_show(os, session).await {
                    Ok(_) => OperationResult::Info("".to_string()), // Empty Info, formatting already done
                    Err(e) => OperationResult::Error(format!("Failed to show knowledge base entries: {}", e)),
                }
            },
            KnowledgeSubcommand::Add {
                path,
                include,
                exclude,
                index_type,
            } => Self::handle_add(os, session, path, include, exclude, index_type).await,
            KnowledgeSubcommand::Remove { path } => Self::handle_remove(os, session, path).await,
            KnowledgeSubcommand::Update { path } => Self::handle_update(os, session, path).await,
            KnowledgeSubcommand::Clear => Self::handle_clear(os, session).await,
            KnowledgeSubcommand::Status => Self::handle_status(os, session).await,
            KnowledgeSubcommand::Cancel { operation_id } => {
                Self::handle_cancel(os, session, operation_id.as_deref()).await
            },
        }
    }

    async fn handle_show(os: &Os, session: &mut ChatSession) -> Result<(), std::io::Error> {
        let agent_name = Self::get_agent(session).map(|a| a.name.clone());

        // Show agent-specific knowledge
        if let Some(agent) = agent_name {
            queue!(
                session.stderr,
                style::SetAttribute(crossterm::style::Attribute::Bold),
                style::SetForegroundColor(Color::Magenta),
                style::Print(format!("👤 Agent ({}):\n", agent)),
                style::SetAttribute(crossterm::style::Attribute::Reset),
            )?;

            match KnowledgeStore::get_async_instance(os, Self::get_agent(session)).await {
                Ok(store) => {
                    let store = store.lock().await;
                    let contexts = store.get_all().await.unwrap_or_default();

                    if contexts.is_empty() {
                        queue!(
                            session.stderr,
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print("    <none>\n\n"),
                            style::SetForegroundColor(Color::Reset)
                        )?;
                    } else {
                        Self::format_knowledge_entries_with_indent(session, &contexts, "    ")?;
                    }
                },
                Err(_) => {
                    queue!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("    <none>\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                },
            }
        }

        Ok(())
    }

    fn format_knowledge_entries_with_indent(
        session: &mut ChatSession,
        contexts: &[semantic_search_client::KnowledgeContext],
        indent: &str,
    ) -> Result<(), std::io::Error> {
        for ctx in contexts {
            // Main entry line with name and ID
            queue!(
                session.stderr,
                style::Print(format!("{}📂 ", indent)),
                style::SetAttribute(style::Attribute::Bold),
                style::SetForegroundColor(Color::Grey),
                style::Print(&ctx.name),
                style::SetForegroundColor(Color::Green),
                style::Print(format!(" ({})", &ctx.id[..8])),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n")
            )?;

            // Description line with original description
            queue!(
                session.stderr,
                style::Print(format!("{}   ", indent)),
                style::SetForegroundColor(Color::Grey),
                style::Print(format!("{}\n", ctx.description)),
                style::SetForegroundColor(Color::Reset)
            )?;

            // Stats line with improved colors
            queue!(
                session.stderr,
                style::Print(format!("{}   ", indent)),
                style::SetForegroundColor(Color::Green),
                style::Print(format!("{} items", ctx.item_count)),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(" • "),
                style::SetForegroundColor(Color::Blue),
                style::Print(ctx.embedding_type.description()),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(" • "),
                style::SetForegroundColor(Color::DarkGrey),
                style::Print(format!("{}", ctx.updated_at.format("%m/%d %H:%M"))),
                style::SetForegroundColor(Color::Reset),
                style::Print("\n\n")
            )?;
        }
        Ok(())
    }

    /// Handle add operation
    fn get_db_patterns(os: &crate::os::Os, setting: crate::database::settings::Setting) -> Vec<String> {
        os.database
            .settings
            .get(setting)
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    }

    async fn handle_add(
        os: &Os,
        session: &mut ChatSession,
        path: &str,
        include_patterns: &[String],
        exclude_patterns: &[String],
        index_type: &Option<String>,
    ) -> OperationResult {
        match Self::validate_and_sanitize_path(os, path) {
            Ok(sanitized_path) => {
                let agent = Self::get_agent(session);

                let async_knowledge_store = match KnowledgeStore::get_async_instance(os, agent).await {
                    Ok(store) => store,
                    Err(e) => return OperationResult::Error(format!("Error accessing knowledge base: {}", e)),
                };
                let mut store = async_knowledge_store.lock().await;

                let include = if include_patterns.is_empty() {
                    Self::get_db_patterns(os, crate::database::settings::Setting::KnowledgeDefaultIncludePatterns)
                } else {
                    include_patterns.to_vec()
                };

                let exclude = if exclude_patterns.is_empty() {
                    Self::get_db_patterns(os, crate::database::settings::Setting::KnowledgeDefaultExcludePatterns)
                } else {
                    exclude_patterns.to_vec()
                };

                let embedding_type_resolved = index_type.clone().or_else(|| {
                    os.database
                        .settings
                        .get(crate::database::settings::Setting::KnowledgeIndexType)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                });

                let options = crate::util::knowledge_store::AddOptions::new()
                    .with_include_patterns(include)
                    .with_exclude_patterns(exclude)
                    .with_embedding_type(embedding_type_resolved);

                match store.add(path, &sanitized_path.clone(), options).await {
                    Ok(message) => OperationResult::Info(message),
                    Err(e) => {
                        if e.contains("Invalid include pattern") || e.contains("Invalid exclude pattern") {
                            OperationResult::Error(e)
                        } else {
                            OperationResult::Error(format!("Failed to add: {}", e))
                        }
                    },
                }
            },
            Err(e) => OperationResult::Error(format!("Invalid path: {}", e)),
        }
    }

    /// Handle remove operation
    async fn handle_remove(os: &Os, session: &ChatSession, path: &str) -> OperationResult {
        let sanitized_path = sanitize_path_tool_arg(os, path);
        let agent = Self::get_agent(session);

        let async_knowledge_store = match KnowledgeStore::get_async_instance(os, agent).await {
            Ok(store) => store,
            Err(e) => return OperationResult::Error(format!("Error accessing knowledge base: {}", e)),
        };
        let mut store = async_knowledge_store.lock().await;

        let scope_desc = "agent";

        // Try path first, then name
        if store.remove_by_path(&sanitized_path.to_string_lossy()).await.is_ok() {
            OperationResult::Success(format!(
                "Removed {} knowledge base entry with path '{}'",
                scope_desc, path
            ))
        } else if store.remove_by_name(path).await.is_ok() {
            OperationResult::Success(format!(
                "Removed {} knowledge base entry with name '{}'",
                scope_desc, path
            ))
        } else {
            OperationResult::Warning(format!("Entry not found in {} knowledge base: {}", scope_desc, path))
        }
    }

    /// Handle update operation
    async fn handle_update(os: &Os, session: &ChatSession, path: &str) -> OperationResult {
        match Self::validate_and_sanitize_path(os, path) {
            Ok(sanitized_path) => {
                let agent = Self::get_agent(session);
                let async_knowledge_store = match KnowledgeStore::get_async_instance(os, agent).await {
                    Ok(store) => store,
                    Err(e) => {
                        return OperationResult::Error(format!("Error accessing knowledge base directory: {}", e));
                    },
                };
                let mut store = async_knowledge_store.lock().await;

                match store.update_by_path(&sanitized_path).await {
                    Ok(message) => OperationResult::Info(message),
                    Err(e) => OperationResult::Error(format!("Failed to update: {}", e)),
                }
            },
            Err(e) => OperationResult::Error(e),
        }
    }

    /// Handle clear operation
    async fn handle_clear(os: &Os, session: &mut ChatSession) -> OperationResult {
        // Require confirmation
        queue!(
            session.stderr,
            style::Print("⚠️  This action will remove all knowledge base entries.\n"),
            style::Print("Clear the knowledge base? (y/N): ")
        )
        .unwrap();
        session.stderr.flush().unwrap();

        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            return OperationResult::Error("Failed to read input".to_string());
        }

        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            return OperationResult::Info("Clear operation cancelled".to_string());
        }

        let agent = Self::get_agent(session);
        let async_knowledge_store = match KnowledgeStore::get_async_instance(os, agent).await {
            Ok(store) => store,
            Err(e) => return OperationResult::Error(format!("Error accessing knowledge base directory: {}", e)),
        };
        let mut store = async_knowledge_store.lock().await;

        // First, cancel any pending operations
        queue!(
            session.stderr,
            style::Print("🛑 Cancelling any pending operations...\n")
        )
        .unwrap();
        if let Err(e) = store.cancel_operation(None).await {
            queue!(
                session.stderr,
                style::Print(&format!("⚠️  Warning: Failed to cancel operations: {}\n", e))
            )
            .unwrap();
        }

        // Now perform immediate synchronous clear
        queue!(
            session.stderr,
            style::Print("🗑️  Clearing all knowledge base entries...\n")
        )
        .unwrap();
        match store.clear_immediate().await {
            Ok(message) => OperationResult::Success(message),
            Err(e) => OperationResult::Error(format!("Failed to clear: {}", e)),
        }
    }

    /// Handle status operation
    async fn handle_status(os: &Os, session: &ChatSession) -> OperationResult {
        let agent = Self::get_agent(session);
        let async_knowledge_store = match KnowledgeStore::get_async_instance(os, agent).await {
            Ok(store) => store,
            Err(e) => return OperationResult::Error(format!("Error accessing knowledge base directory: {}", e)),
        };
        let store = async_knowledge_store.lock().await;

        match store.get_status_data().await {
            Ok(status_data) => {
                let formatted_status = Self::format_status_display(&status_data);
                OperationResult::Info(formatted_status)
            },
            Err(e) => OperationResult::Error(format!("Failed to get status: {}", e)),
        }
    }

    /// Format status data for display (UI rendering responsibility)
    fn format_status_display(status: &SystemStatus) -> String {
        let mut status_lines = Vec::new();

        // Show knowledge base summary
        status_lines.push(format!(
            "📚 Total knowledge base entries: {} ({} persistent, {} volatile)",
            status.total_contexts, status.persistent_contexts, status.volatile_contexts
        ));

        if status.operations.is_empty() {
            status_lines.push("✅ No active operations".to_string());
            return status_lines.join("\n");
        }

        status_lines.push("📊 Active Operations:".to_string());
        status_lines.push(format!(
            "  📈 Queue Status: {} active, {} waiting (max {} concurrent)",
            status.active_count, status.waiting_count, status.max_concurrent
        ));

        for op in &status.operations {
            let formatted_operation = Self::format_operation_display(op);
            status_lines.push(formatted_operation);
        }

        status_lines.join("\n")
    }

    /// Format a single operation for display
    fn format_operation_display(op: &OperationStatus) -> String {
        let elapsed = op.started_at.elapsed().unwrap_or_default();

        let (status_icon, status_info) = if op.is_cancelled {
            ("🛑", "Cancelled".to_string())
        } else if op.is_failed {
            ("❌", op.message.clone())
        } else if op.is_waiting {
            ("⏳", op.message.clone())
        } else if Self::should_show_progress_bar(op.current, op.total) {
            ("🔄", Self::create_progress_bar(op.current, op.total, &op.message))
        } else {
            ("🔄", op.message.clone())
        };

        let operation_desc = op.operation_type.display_name();

        // Format with conditional elapsed time and ETA
        if op.is_cancelled || op.is_failed {
            format!(
                "  {} {} | {}\n    {}",
                status_icon, op.short_id, operation_desc, status_info
            )
        } else {
            let mut time_info = format!("Elapsed: {}s", elapsed.as_secs());

            if let Some(eta) = op.eta {
                time_info.push_str(&format!(" | ETA: {}s", eta.as_secs()));
            }

            format!(
                "  {} {} | {}\n    {} | {}",
                status_icon, op.short_id, operation_desc, status_info, time_info
            )
        }
    }

    /// Check if progress bar should be shown
    fn should_show_progress_bar(current: u64, total: u64) -> bool {
        total > 0 && current <= total
    }

    /// Create progress bar display
    fn create_progress_bar(current: u64, total: u64, message: &str) -> String {
        if total == 0 {
            return message.to_string();
        }

        let percentage = (current as f64 / total as f64 * 100.0) as u8;
        let filled = (current as f64 / total as f64 * 30.0) as usize;
        let empty = 30 - filled;

        let mut bar = String::new();
        bar.push_str(&"█".repeat(filled));
        if filled < 30 && current < total {
            bar.push('▓');
            bar.push_str(&"░".repeat(empty.saturating_sub(1)));
        } else {
            bar.push_str(&"░".repeat(empty));
        }

        format!("{} {}% ({}/{}) {}", bar, percentage, current, total, message)
    }

    /// Handle cancel operation
    async fn handle_cancel(os: &Os, session: &ChatSession, operation_id: Option<&str>) -> OperationResult {
        let agent = Self::get_agent(session);
        let async_knowledge_store = match KnowledgeStore::get_async_instance(os, agent).await {
            Ok(store) => store,
            Err(e) => return OperationResult::Error(format!("Error accessing knowledge base directory: {}", e)),
        };
        let mut store = async_knowledge_store.lock().await;

        match store.cancel_operation(operation_id).await {
            Ok(result) => OperationResult::Success(result),
            Err(e) => OperationResult::Error(format!("Failed to cancel operation: {}", e)),
        }
    }

    /// Validate and sanitize path
    fn validate_and_sanitize_path(os: &Os, path: &str) -> Result<String, String> {
        if path.contains('\n') {
            return Ok(path.to_string());
        }

        let os_path = sanitize_path_tool_arg(os, path);
        if !os_path.exists() {
            return Err(format!("Path '{}' does not exist", path));
        }

        Ok(os_path.to_string_lossy().to_string())
    }

    fn write_operation_result(session: &mut ChatSession, result: OperationResult) -> Result<(), std::io::Error> {
        match result {
            OperationResult::Success(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n{}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
            OperationResult::Info(msg) => {
                if !msg.trim().is_empty() {
                    queue!(
                        session.stderr,
                        style::Print(format!("\n{}\n\n", msg)),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                }
                Ok(())
            },
            OperationResult::Warning(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(format!("\n{}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
            OperationResult::Error(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\nError: {}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            KnowledgeSubcommand::Show => "show",
            KnowledgeSubcommand::Add { .. } => "add",
            KnowledgeSubcommand::Remove { .. } => "remove",
            KnowledgeSubcommand::Update { .. } => "update",
            KnowledgeSubcommand::Clear => "clear",
            KnowledgeSubcommand::Status => "status",
            KnowledgeSubcommand::Cancel { .. } => "cancel",
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    #[command(name = "test")]
    struct TestCli {
        #[command(subcommand)]
        knowledge: KnowledgeSubcommand,
    }

    #[test]
    fn test_include_exclude_patterns_parsing() {
        // Test that include and exclude patterns are parsed correctly
        let result = TestCli::try_parse_from([
            "test",
            "add",
            "/some/path",
            "--include",
            "*.rs",
            "--include",
            "**/*.md",
            "--exclude",
            "node_modules/**",
            "--exclude",
            "target/**",
        ]);

        assert!(result.is_ok());
        let cli = result.unwrap();

        if let KnowledgeSubcommand::Add {
            path, include, exclude, ..
        } = cli.knowledge
        {
            assert_eq!(path, "/some/path");
            assert_eq!(include, vec!["*.rs", "**/*.md"]);
            assert_eq!(exclude, vec!["node_modules/**", "target/**"]);
        } else {
            panic!("Expected Add subcommand");
        }
    }

    #[test]
    fn test_clap_markdown_parsing_issue() {
        let help_result = TestCli::try_parse_from(["test", "add", "--help"]);
        match help_result {
            Err(err) if err.kind() == clap::error::ErrorKind::DisplayHelp => {
                // This is expected for --help
                // The actual issue would be visible in the help text formatting
                // We can't easily test the exact formatting here, but this documents the issue
            },
            _ => panic!("Expected help output"),
        }
    }

    #[test]
    fn test_empty_patterns_allowed() {
        // Test that commands work without any patterns
        let result = TestCli::try_parse_from(["test", "add", "/some/path"]);
        assert!(result.is_ok());

        let cli = result.unwrap();
        if let KnowledgeSubcommand::Add {
            path, include, exclude, ..
        } = cli.knowledge
        {
            assert_eq!(path, "/some/path");
            assert!(include.is_empty());
            assert!(exclude.is_empty());
        } else {
            panic!("Expected Add subcommand");
        }
    }

    #[test]
    fn test_multiple_include_patterns() {
        // Test multiple include patterns
        let result = TestCli::try_parse_from([
            "test",
            "add",
            "/some/path",
            "--include",
            "*.rs",
            "--include",
            "*.md",
            "--include",
            "*.txt",
        ]);

        assert!(result.is_ok());
        let cli = result.unwrap();

        if let KnowledgeSubcommand::Add { include, .. } = cli.knowledge {
            assert_eq!(include, vec!["*.rs", "*.md", "*.txt"]);
        } else {
            panic!("Expected Add subcommand");
        }
    }

    #[test]
    fn test_multiple_exclude_patterns() {
        // Test multiple exclude patterns
        let result = TestCli::try_parse_from([
            "test",
            "add",
            "/some/path",
            "--exclude",
            "node_modules/**",
            "--exclude",
            "target/**",
            "--exclude",
            ".git/**",
        ]);

        assert!(result.is_ok());
        let cli = result.unwrap();

        if let KnowledgeSubcommand::Add { exclude, .. } = cli.knowledge {
            assert_eq!(exclude, vec!["node_modules/**", "target/**", ".git/**"]);
        } else {
            panic!("Expected Add subcommand");
        }
    }
}
