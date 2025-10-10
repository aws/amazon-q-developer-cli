use std::io::Write;

use clap::Subcommand;
use crossterm::queue;
use crossterm::style::{
    self,
};
use eyre::Result;
use semantic_search_client::SystemStatus;

use crate::cli::chat::tools::sanitize_path_tool_arg;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::os::Os;
use crate::theme::StyledText;
use crate::util::knowledge_store::KnowledgeStore;

/// Knowledge base management commands
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum KnowledgeSubcommand {
    /// Display the knowledge base contents and background operations
    Show,
    /// Add a file or directory to knowledge base
    Add {
        /// Name for the knowledge base entry
        #[arg(long, short = 'n')]
        name: String,
        /// Path to file or directory to add
        #[arg(long, short = 'p')]
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
        ExperimentManager::is_enabled(os, ExperimentName::Knowledge)
    }

    fn write_feature_disabled_message(session: &mut ChatSession) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            StyledText::error_fg(),
            style::Print("\nKnowledge tool is disabled. Enable it with: q settings chat.enableKnowledge true\n"),
            StyledText::warning_fg(),
            style::Print("💡 Your knowledge base data is preserved and will be available when re-enabled.\n\n"),
            StyledText::reset(),
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
                name,
                path,
                include,
                exclude,
                index_type,
            } => Self::handle_add(os, session, name, path, include, exclude, index_type).await,
            KnowledgeSubcommand::Remove { path } => Self::handle_remove(os, session, path).await,
            KnowledgeSubcommand::Update { path } => Self::handle_update(os, session, path).await,
            KnowledgeSubcommand::Clear => Self::handle_clear(os, session).await,
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
                StyledText::emphasis_fg(),
                style::Print(format!("👤 Agent ({}):\n", agent)),
                StyledText::reset_attributes(),
            )?;

            match KnowledgeStore::get_async_instance(os, Self::get_agent(session)).await {
                Ok(store) => {
                    let store = store.lock().await;
                    let contexts = store.get_all().await.unwrap_or_default();
                    let status_data = store.get_status_data().await.ok();

                    // Show contexts if any exist
                    if !contexts.is_empty() {
                        Self::format_knowledge_entries_with_indent(session, &contexts, "    ")?;
                    }

                    // Show operations if any exist
                    if let Some(status) = &status_data {
                        if !status.operations.is_empty() {
                            let formatted_status = Self::format_status_display(status);
                            if !formatted_status.is_empty() {
                                queue!(session.stderr, style::Print(format!("{}\n", formatted_status)))?;
                            }
                        }
                    }

                    // Only show <none> if both contexts and operations are empty
                    if contexts.is_empty()
                        && (status_data.is_none() || status_data.as_ref().unwrap().operations.is_empty())
                    {
                        queue!(
                            session.stderr,
                            StyledText::secondary_fg(),
                            style::Print("    <none>\n\n"),
                            StyledText::reset(),
                        )?;
                    }
                },
                Err(_) => {
                    queue!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("    <none>\n\n"),
                        StyledText::reset(),
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
                StyledText::secondary_fg(),
                style::Print(&ctx.name),
                StyledText::success_fg(),
                style::Print(format!(" ({})", &ctx.id[..8])),
                StyledText::reset_attributes(),
                StyledText::reset(),
                style::Print("\n")
            )?;

            // Path line if available (matching operation format)
            if let Some(source_path) = &ctx.source_path {
                queue!(
                    session.stderr,
                    style::Print(format!("{}   ", indent)),
                    StyledText::secondary_fg(),
                    style::Print(format!("{}\n", source_path)),
                    StyledText::reset(),
                )?;
            }

            // Stats line with improved colors
            queue!(
                session.stderr,
                style::Print(format!("{}   ", indent)),
                StyledText::success_fg(),
                style::Print(format!("{} items", ctx.item_count)),
                StyledText::secondary_fg(),
                style::Print(" • "),
                StyledText::info_fg(),
                style::Print(ctx.embedding_type.description()),
                StyledText::secondary_fg(),
                style::Print(" • "),
                StyledText::secondary_fg(),
                style::Print(format!("{}", ctx.updated_at.format("%m/%d %H:%M"))),
                StyledText::reset(),
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
        name: &str,
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

                match store.add(name, &sanitized_path.clone(), options).await {
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

    /// Format status data for display (UI rendering responsibility)
    fn format_status_display(status: &SystemStatus) -> String {
        if status.operations.is_empty() {
            return String::new(); // No operations, no output
        }

        let mut output = String::new();
        for op in &status.operations {
            let operation_desc = op.operation_type.display_name();

            // Main entry line with operation name and ID (like knowledge entries)
            output.push_str(&format!("    🔄 {} ({})\n", operation_desc, &op.short_id));

            // Description/path line (indented like knowledge entries)
            // Use actual path from operation type if available, otherwise use message
            let description = match &op.operation_type {
                semantic_search_client::OperationType::Indexing { path, .. } => path.clone(),
                semantic_search_client::OperationType::Clearing => op.message.clone(),
            };
            output.push_str(&format!("       {}\n", description));

            // Status/progress line with ETA if available
            if op.is_cancelled {
                output.push_str("       Cancelled\n");
            } else if op.is_failed {
                output.push_str("       Failed\n");
            } else if op.is_waiting {
                output.push_str("       Waiting\n");
            } else if Self::should_show_progress_bar(op.current, op.total) {
                let percentage = (op.current as f64 / op.total as f64 * 100.0) as u8;
                if let Some(eta) = op.eta {
                    output.push_str(&format!("       {}% • ETA: {}s\n", percentage, eta.as_secs()));
                } else {
                    output.push_str(&format!("       {}%\n", percentage));
                }
            } else {
                output.push_str("       In progress\n");
            }
        }

        output.trim_end().to_string() // Remove trailing newline
    }

    /// Check if progress bar should be shown
    fn should_show_progress_bar(current: u64, total: u64) -> bool {
        total > 0 && current <= total
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
                    StyledText::success_fg(),
                    style::Print(format!("\n{}\n\n", msg)),
                    StyledText::reset(),
                )
            },
            OperationResult::Info(msg) => {
                if !msg.trim().is_empty() {
                    queue!(
                        session.stderr,
                        style::Print(format!("\n{}\n\n", msg)),
                        StyledText::reset(),
                    )?;
                }
                Ok(())
            },
            OperationResult::Warning(msg) => {
                queue!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print(format!("\n{}\n\n", msg)),
                    StyledText::reset(),
                )
            },
            OperationResult::Error(msg) => {
                queue!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("\nError: {}\n\n", msg)),
                    StyledText::reset(),
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
            KnowledgeSubcommand::Cancel { .. } => "cancel",
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser, Debug)]
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
            "--name",
            "my-project",
            "--path",
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
            name,
            path,
            include,
            exclude,
            ..
        } = cli.knowledge
        {
            assert_eq!(name, "my-project");
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
        let result = TestCli::try_parse_from(["test", "add", "--name", "my-project", "--path", "/some/path"]);
        assert!(result.is_ok());

        let cli = result.unwrap();
        if let KnowledgeSubcommand::Add {
            name,
            path,
            include,
            exclude,
            ..
        } = cli.knowledge
        {
            assert_eq!(name, "my-project");
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
            "--name",
            "my-project",
            "--path",
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
    fn test_add_command_with_name_and_path() {
        // Test that add command accepts both name and path parameters
        let result = TestCli::try_parse_from(["test", "add", "--name", "my-project", "--path", "/path/to/project"]);

        assert!(result.is_ok());
        let cli = result.unwrap();

        if let KnowledgeSubcommand::Add { name, path, .. } = cli.knowledge {
            assert_eq!(name, "my-project");
            assert_eq!(path, "/path/to/project");
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
            "--name",
            "my-project",
            "--path",
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
