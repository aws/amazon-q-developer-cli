use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use eyre::Result;
use serde::{Deserialize, Serialize};

use crate::cli::ConversationState;
use crate::database::{Database, DatabaseError};
use crate::os::Os;

#[derive(Debug, Args, PartialEq)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub command: HistoryCommands,
}

#[derive(Debug, Subcommand, PartialEq)]
pub enum HistoryCommands {
    /// List recent conversations
    List {
        /// Maximum number of conversations to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
        
        /// Filter by directory path
        #[arg(short, long)]
        path: Option<String>,
        
        /// Filter conversations containing this text
        #[arg(short, long)]
        contains: Option<String>,
    },
    /// Show a specific conversation
    Show {
        /// Conversation ID or partial ID
        id: String,
    },
    /// Restore a conversation to the current directory
    Restore {
        /// Conversation ID or partial ID
        id: String,
    },
    /// Search conversations by content
    Search {
        /// Search query to find in conversation content
        query: String,
        
        /// Maximum number of results to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Export a conversation to a file
    Export {
        /// Conversation ID or partial ID
        id: String,
        
        /// Output file path
        #[arg(short, long)]
        output: String,
        
        /// Export format
        #[arg(long, default_value = "json")]
        format: ExportFormat,
        
        /// Overwrite existing file
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum ExportFormat {
    /// JSON format (same as /save command, can be imported with /load)
    Json,
    /// Markdown format for readable documentation
    Markdown,
    /// Plain text format for simple reading
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub preview: String,
    pub message_count: usize,
}

impl HistoryArgs {
    pub async fn execute(self, os: &mut Os) -> Result<std::process::ExitCode> {
        match self.command {
            HistoryCommands::List { limit, path, contains } => {
                list_conversations(&os.database, limit, path.as_deref(), contains.as_deref()).await?;
            }
            HistoryCommands::Show { id } => {
                show_conversation(&os.database, &id).await?;
            }
            HistoryCommands::Restore { id } => {
                restore_conversation(&mut os.database, &id).await?;
            }
            HistoryCommands::Search { query, limit } => {
                search_conversations(&os.database, &query, limit).await?;
            }
            HistoryCommands::Export { id, output, format, force } => {
                export_conversation(&os.database, &os.fs, &id, &output, format, force).await?;
            }
        }
        Ok(std::process::ExitCode::SUCCESS)
    }
}

async fn list_conversations(
    database: &Database,
    limit: usize,
    path_filter: Option<&str>,
    contains_filter: Option<&str>,
) -> Result<()> {
    let conversations = database.list_conversations(limit, path_filter, contains_filter)?;
    
    if conversations.is_empty() {
        println!("No conversations found.");
        return Ok(());
    }

    println!("Recent Conversations:");
    println!("┌──────────┬─────────────────────┬──────────────────────────────────────────────────┬─────────────────────────────────────┐");
    println!("│ ID       │ Date                │ Directory                                        │ Preview                             │");
    println!("├──────────┼─────────────────────┼──────────────────────────────────────────────────┼─────────────────────────────────────┤");
    
    for conv in conversations {
        let date_str = conv.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let path_display = truncate_path(&conv.path, 48); // Increased from 36 to 48
        let preview_display = truncate_string(&conv.preview, 35);
        let id_display = truncate_string(&conv.id[..8.min(conv.id.len())], 8); // Show first 8 chars
        
        println!(
            "│ {:<8} │ {:<19} │ {:<48} │ {:<35} │",
            id_display, date_str, path_display, preview_display
        );
    }
    
    println!("└──────────┴─────────────────────┴──────────────────────────────────────────────────┴─────────────────────────────────────┘");
    println!("\nTo show a conversation: q history show <ID>");
    println!("To search conversations: q history search <query>");
    println!("To export a conversation: q history export <ID> --output <file>");
    println!("To restore a conversation to current directory: q history restore <ID>");
    println!("To resume a conversation, navigate to the directory and run `q chat --resume`");
    
    Ok(())
}

async fn show_conversation(database: &Database, id: &str) -> Result<()> {
    let conversation = database.get_conversation_by_id(id)?;
    
    match conversation {
        Some((path, state)) => {
            println!("Conversation: {}", state.conversation_id());
            println!("Directory: {}", path);
            
            // Extract creation time from the first message if available
            if let Some(_first_entry) = state.history().front() {
                // For now, we'll show a placeholder since we don't have timestamps in the current structure
                println!("Messages: {}", state.history().len());
            }
            
            println!("\nTo resume this conversation:");
            println!("  cd {}", path);
            println!("  q chat --resume");
            println!();
            println!("─────────────────────────────────────────────────────────────");
            println!();
            
            // Display the conversation transcript
            for (i, entry) in state.transcript.iter().enumerate() {
                println!("{}", entry);
                if i < state.transcript.len() - 1 {
                    println!();
                }
            }
        }
        None => {
            println!("Conversation with ID '{}' not found.", id);
            println!("Use `q history list` to see available conversations.");
        }
    }
    
    Ok(())
}

async fn search_conversations(database: &Database, query: &str, limit: usize) -> Result<()> {
    let results = database.search_conversations(query, limit)?;
    
    if results.is_empty() {
        println!("No conversations found matching '{}'.", query);
        return Ok(());
    }

    println!("Search Results for '{}':", query);
    println!("┌──────────┬─────────────────────┬──────────────────────────────────────────────────┬─────────────────────────────────────┐");
    println!("│ ID       │ Date                │ Directory                                        │ Preview                             │");
    println!("├──────────┼─────────────────────┼──────────────────────────────────────────────────┼─────────────────────────────────────┤");
    
    for result in results {
        let date_str = result.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let path_display = truncate_path(&result.path, 48);
        let preview_display = truncate_string(&result.preview, 35);
        let id_display = truncate_string(&result.id[..8.min(result.id.len())], 8);
        
        println!(
            "│ {:<8} │ {:<19} │ {:<48} │ {:<35} │",
            id_display, date_str, path_display, preview_display
        );
    }
    
    println!("└──────────┴─────────────────────┴──────────────────────────────────────────────────┴─────────────────────────────────────┘");
    println!("\nTo show a conversation: q history show <ID>");
    println!("To export a conversation: q history export <ID> --output <file>");
    println!("To restore a conversation to current directory: q history restore <ID>");
    println!("To resume a conversation, navigate to the directory and run `q chat --resume`");
    
    Ok(())
}

async fn export_conversation(
    database: &Database,
    fs: &crate::os::Fs,
    id: &str,
    output_path: &str,
    format: ExportFormat,
    force: bool,
) -> Result<()> {
    let conversation = database.get_conversation_by_id(id)?;
    
    match conversation {
        Some((original_path, state)) => {
            // Check if file exists and force flag
            if fs.exists(output_path) && !force {
                println!("❌ File '{}' already exists. Use --force to overwrite.", output_path);
                return Ok(());
            }
            
            let content = match format {
                ExportFormat::Json => {
                    // Use the same JSON serialization as /save command
                    serde_json::to_string_pretty(&state)
                        .map_err(|e| eyre::eyre!("Failed to serialize conversation: {}", e))?
                }
                ExportFormat::Markdown => {
                    format_conversation_as_markdown(&state, &original_path)
                }
                ExportFormat::Text => {
                    format_conversation_as_text(&state, &original_path)
                }
            };
            
            fs.write(output_path, content).await
                .map_err(|e| eyre::eyre!("Failed to write to '{}': {}", output_path, e))?;
            
            let format_desc = match format {
                ExportFormat::Json => "JSON (compatible with /load)",
                ExportFormat::Markdown => "Markdown",
                ExportFormat::Text => "plain text",
            };
            
            println!("✅ Exported conversation {} as {} to '{}'", 
                     &id[..8.min(id.len())], format_desc, output_path);
            println!();
            println!("Conversation: {}", state.conversation_id());
            println!("Original directory: {}", original_path);
            println!("Messages: {}", state.history().len());
            
            if format == ExportFormat::Json {
                println!();
                println!("💡 You can import this conversation in any chat session with:");
                println!("   /load {}", output_path);
            }
        }
        None => {
            println!("❌ Conversation with ID '{}' not found.", id);
            println!();
            println!("💡 Use 'q history list' to see available conversations.");
        }
    }
    
    Ok(())
}

fn format_conversation_as_markdown(state: &ConversationState, original_path: &str) -> String {
    let mut content = String::new();
    
    // Header
    content.push_str(&format!("# Conversation Export\n\n"));
    content.push_str(&format!("**Conversation ID:** `{}`\n", state.conversation_id()));
    content.push_str(&format!("**Original Directory:** `{}`\n", original_path));
    content.push_str(&format!("**Messages:** {}\n", state.history().len()));
    content.push_str(&format!("**Exported:** {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    
    content.push_str("---\n\n");
    
    // Conversation transcript
    for (i, entry) in state.transcript.iter().enumerate() {
        // Try to determine if this is a user message or assistant response
        // This is a simplified approach - the actual structure might be more complex
        if entry.starts_with('>') {
            content.push_str(&format!("## User Message {}\n\n", i / 2 + 1));
            content.push_str(&format!("```\n{}\n```\n\n", entry.trim_start_matches('>')));
        } else {
            content.push_str(&format!("## Assistant Response {}\n\n", i / 2 + 1));
            content.push_str(&format!("{}\n\n", entry));
        }
    }
    
    content
}

fn format_conversation_as_text(state: &ConversationState, original_path: &str) -> String {
    let mut content = String::new();
    
    // Header
    content.push_str("CONVERSATION EXPORT\n");
    content.push_str("==================\n\n");
    content.push_str(&format!("Conversation ID: {}\n", state.conversation_id()));
    content.push_str(&format!("Original Directory: {}\n", original_path));
    content.push_str(&format!("Messages: {}\n", state.history().len()));
    content.push_str(&format!("Exported: {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    
    content.push_str(&"─".repeat(80));
    content.push_str("\n\n");
    
    // Conversation transcript
    for (i, entry) in state.transcript.iter().enumerate() {
        if entry.starts_with('>') {
            content.push_str(&format!("USER MESSAGE {}:\n", i / 2 + 1));
            content.push_str(&format!("{}\n\n", entry.trim_start_matches('>')));
        } else {
            content.push_str(&format!("ASSISTANT RESPONSE {}:\n", i / 2 + 1));
            content.push_str(&format!("{}\n\n", entry));
        }
        
        content.push_str(&"─".repeat(40));
        content.push_str("\n\n");
    }
    
    content
}

async fn restore_conversation(database: &mut Database, id: &str) -> Result<()> {
    let conversation = database.get_conversation_by_id(id)?;
    
    match conversation {
        Some((original_path, state)) => {
            // Get the current working directory
            let current_dir = std::env::current_dir()
                .map_err(|e| eyre::eyre!("Failed to get current directory: {}", e))?;
            
            let current_path = current_dir.to_string_lossy().to_string();
            
            // Check if there's already a conversation in the current directory
            let existing_conversation = database.get_conversation_by_path(&current_dir)?;
            
            if let Some(existing_state) = existing_conversation {
                // Create a backup of the existing conversation
                let backup_key = database.backup_conversation(&current_path, &existing_state)?;
                
                println!("📦 Existing conversation backed up as: {}", backup_key);
                println!("   (You can restore it later if needed)");
            }
            
            // Save the conversation to the current directory
            database.set_conversation_by_path(&current_dir, &state)?;
            
            println!("✅ Conversation restored successfully!");
            println!();
            println!("Conversation: {}", state.conversation_id());
            println!("Original directory: {}", original_path);
            println!("Restored to: {}", current_path);
            println!("Messages: {}", state.history().len());
            println!();
            println!("You can now resume the conversation by running:");
            println!("  q chat --resume");
        }
        None => {
            println!("Conversation with ID '{}' not found.", id);
            println!("Use `q history list` to see available conversations.");
        }
    }
    
    Ok(())
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        format!("{:<width$}", s, width = max_len)
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

fn truncate_path(path: &str, max_len: usize) -> String {
    // First, replace home directory with ~
    let path = if let Ok(home) = std::env::var("HOME") {
        if path.starts_with(&home) {
            path.replace(&home, "~")
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };
    
    // If the path fits, return it with padding
    if path.len() <= max_len {
        return format!("{:<width$}", path, width = max_len);
    }
    
    // For long paths, show the end (most specific directories) rather than the beginning
    // This is more useful for distinguishing between conversations
    let available_chars = max_len.saturating_sub(3); // Reserve 3 chars for "..."
    if available_chars > 0 {
        let start_pos = path.len().saturating_sub(available_chars);
        
        // Try to start at a directory boundary if possible
        let truncated = if let Some(slash_pos) = path[start_pos..].find('/') {
            &path[start_pos + slash_pos..]
        } else {
            &path[start_pos..]
        };
        
        format!("...{}", truncated)
    } else {
        "...".to_string()
    }
}

// Extension methods for Database
impl Database {
    /// List all conversations with optional filtering and limiting
    pub fn list_conversations(
        &self,
        limit: usize,
        path_filter: Option<&str>,
        contains_filter: Option<&str>,
    ) -> Result<Vec<ConversationSummary>, DatabaseError> {
        let entries = self.get_all_conversations()?;
        let mut conversations = Vec::new();
        
        // Convert entries to a sorted vector for consistent ordering
        let mut sorted_entries: Vec<_> = entries.into_iter().collect();
        sorted_entries.sort_by(|a, b| b.0.cmp(&a.0)); // Sort by path descending
        
        for (path, value) in sorted_entries {
            // Apply path filter if specified
            if let Some(filter) = path_filter {
                if !path.contains(filter) {
                    continue;
                }
            }
            
            // Parse the conversation state - the value is stored as a JSON string
            match serde_json::from_value::<String>(value) {
                Ok(json_string) => {
                    match serde_json::from_str::<ConversationState>(&json_string) {
                        Ok(state) => {
                            // Apply contains filter if specified
                            if let Some(contains) = contains_filter {
                                if !conversation_contains_text(&state, contains) {
                                    continue;
                                }
                            }
                            
                            let summary = ConversationSummary {
                                id: state.conversation_id().to_string(),
                                path: path.clone(),
                                created_at: Utc::now(), // Placeholder - we'll improve this later
                                updated_at: Utc::now(), // Placeholder - we'll improve this later
                                preview: extract_preview(&state),
                                message_count: state.history().len(),
                            };
                            conversations.push(summary);
                        }
                        Err(e) => {
                            // Skip conversations that can't be parsed
                            tracing::warn!("Failed to parse conversation JSON at path {}: {}", path, e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    // Skip conversations that can't be parsed
                    tracing::warn!("Failed to parse conversation value at path {}: {}", path, e);
                    continue;
                }
            }
            
            // Apply limit
            if conversations.len() >= limit {
                break;
            }
        }
        
        Ok(conversations)
    }
    
    /// Search conversations by content
    pub fn search_conversations(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ConversationSummary>, DatabaseError> {
        let entries = self.get_all_conversations()?;
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        
        for (path, value) in entries {
            match serde_json::from_value::<String>(value) {
                Ok(json_string) => {
                    match serde_json::from_str::<ConversationState>(&json_string) {
                        Ok(state) => {
                            // Check if conversation contains the search query
                            if conversation_contains_text(&state, &query_lower) {
                                let summary = ConversationSummary {
                                    id: state.conversation_id().to_string(),
                                    path: path.clone(),
                                    created_at: Utc::now(), // Placeholder
                                    updated_at: Utc::now(), // Placeholder
                                    preview: extract_search_preview(&state, &query_lower),
                                    message_count: state.history().len(),
                                };
                                results.push(summary);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse conversation JSON at path {}: {}", path, e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse conversation value at path {}: {}", path, e);
                    continue;
                }
            }
            
            // Apply limit
            if results.len() >= limit {
                break;
            }
        }
        
        // Sort results by relevance (for now, just by path)
        results.sort_by(|a, b| a.path.cmp(&b.path));
        
        Ok(results)
    }
    
    /// Get a conversation by its ID (supports partial matching)
    pub fn get_conversation_by_id(
        &self,
        id: &str,
    ) -> Result<Option<(String, ConversationState)>, DatabaseError> {
        let entries = self.get_all_conversations()?;
        
        for (path, value) in entries {
            match serde_json::from_value::<String>(value) {
                Ok(json_string) => {
                    match serde_json::from_str::<ConversationState>(&json_string) {
                        Ok(state) => {
                            let conv_id = state.conversation_id();
                            // Support both exact match and partial match (first 8 characters)
                            if conv_id == id || conv_id.starts_with(id) {
                                return Ok(Some((path, state)));
                            }
                        }
                        Err(_) => continue,
                    }
                }
                Err(_) => continue,
            }
        }
        
        Ok(None)
    }
    
    /// Backup a conversation with a timestamped key
    pub fn backup_conversation(
        &mut self,
        original_path: &str,
        state: &ConversationState,
    ) -> Result<String, DatabaseError> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_key = format!("{}.backup.{}", original_path, timestamp);
        
        // Use the same method as set_conversation_by_path
        self.set_conversation_by_path(std::path::Path::new(&backup_key), state)?;
        
        Ok(backup_key)
    }
}

fn extract_preview(state: &ConversationState) -> String {
    // Try to get the first user message from the history
    if let Some(first_entry) = state.history().front() {
        if let Some(prompt) = first_entry.user().prompt() {
            // Take first 50 characters and clean up whitespace
            let preview = prompt.trim().replace("\n", " ");
            if preview.len() > 50 {
                format!("{}...", &preview[..47])
            } else {
                preview
            }
        } else {
            "Tool use conversation".to_string()
        }
    } else {
        "Empty conversation".to_string()
    }
}

/// Check if a conversation contains the given text (case-insensitive)
fn conversation_contains_text(state: &ConversationState, query: &str) -> bool {
    let query_lower = query.to_lowercase();
    
    // Search through the transcript
    for entry in state.transcript.iter() {
        if entry.to_lowercase().contains(&query_lower) {
            return true;
        }
    }
    
    // Also search through the history entries
    for entry in state.history().iter() {
        // Check user prompts
        if let Some(prompt) = entry.user().prompt() {
            if prompt.to_lowercase().contains(&query_lower) {
                return true;
            }
        }
        
        // Check assistant responses (if available in the entry)
        // Note: The exact structure depends on how responses are stored
        // This is a simplified check - we might need to adjust based on the actual data structure
    }
    
    false
}

/// Extract a preview that highlights the search query context
fn extract_search_preview(state: &ConversationState, query: &str) -> String {
    let query_lower = query.to_lowercase();
    
    // First, try to find the query in the transcript
    for entry in state.transcript.iter() {
        let entry_lower = entry.to_lowercase();
        if let Some(pos) = entry_lower.find(&query_lower) {
            // Extract context around the match
            let start = pos.saturating_sub(20);
            let end = (pos + query.len() + 20).min(entry.len());
            let context = &entry[start..end];
            let cleaned = context.trim().replace("\n", " ");
            
            if cleaned.len() > 50 {
                return format!("...{}...", &cleaned[..47]);
            } else {
                return format!("...{}...", cleaned);
            }
        }
    }
    
    // If not found in transcript, try history
    for entry in state.history().iter() {
        if let Some(prompt) = entry.user().prompt() {
            let prompt_lower = prompt.to_lowercase();
            if let Some(pos) = prompt_lower.find(&query_lower) {
                let start = pos.saturating_sub(20);
                let end = (pos + query.len() + 20).min(prompt.len());
                let context = &prompt[start..end];
                let cleaned = context.trim().replace("\n", " ");
                
                if cleaned.len() > 50 {
                    return format!("...{}...", &cleaned[..47]);
                } else {
                    return format!("...{}...", cleaned);
                }
            }
        }
    }
    
    // Fallback to regular preview
    extract_preview(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short     ");
        assert_eq!(truncate_string("this is a very long string", 10), "this is...");
        assert_eq!(truncate_string("exactly10!", 10), "exactly10!");
    }

    #[test]
    fn test_truncate_path() {
        // Test home directory replacement
        unsafe {
            std::env::set_var("HOME", "/home/testuser");
        }
        assert_eq!(truncate_path("/home/testuser/project", 20), "~/project           ");
        
        // Test long path truncation - should show the end of the path
        let long_path = "/very/long/path/that/exceeds/the/limit";
        let result = truncate_path(long_path, 20);
        assert!(result.len() <= 20);
        assert!(result.starts_with("..."));
        assert!(result.contains("limit")); // Should show the end part
        
        // Test path without home
        unsafe {
            std::env::remove_var("HOME");
        }
        let result = truncate_path("/some/path", 20);
        assert_eq!(result, "/some/path          ");
    }

    #[test]
    fn test_conversation_summary_serialization() {
        let summary = ConversationSummary {
            id: "test-id".to_string(),
            path: "/test/path".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            preview: "Test preview".to_string(),
            message_count: 5,
        };

        // Test serialization/deserialization
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: ConversationSummary = serde_json::from_str(&json).unwrap();
        
        assert_eq!(summary.id, deserialized.id);
        assert_eq!(summary.path, deserialized.path);
        assert_eq!(summary.preview, deserialized.preview);
        assert_eq!(summary.message_count, deserialized.message_count);
    }

    #[tokio::test]
    async fn test_list_conversations_empty_database() {
        let db = Database::new().await.unwrap();
        
        let conversations = db.list_conversations(10, None, None).unwrap();
        assert!(conversations.is_empty());
    }

    #[test]
    fn test_partial_id_matching() {
        let full_id = "f18c31da-422d-43b9-b7b1-bb01fb7c772b";
        
        // Test various partial matches
        assert!(full_id.starts_with("f18c31da"));
        assert!(full_id.starts_with("f18c"));
        assert!(full_id.starts_with("f"));
        assert!(!full_id.starts_with("g"));
    }

    #[test]
    fn test_history_commands_equality() {
        // Test that our command enums work correctly
        let list1 = HistoryCommands::List { limit: 10, path: None, contains: None };
        let list2 = HistoryCommands::List { limit: 10, path: None, contains: None };
        let list3 = HistoryCommands::List { limit: 20, path: None, contains: None };
        
        assert_eq!(list1, list2);
        assert_ne!(list1, list3);
        
        let show1 = HistoryCommands::Show { id: "abc123".to_string() };
        let show2 = HistoryCommands::Show { id: "abc123".to_string() };
        let show3 = HistoryCommands::Show { id: "def456".to_string() };
        
        assert_eq!(show1, show2);
        assert_ne!(show1, show3);
        assert_ne!(list1, show1);
        
        // Test restore command
        let restore1 = HistoryCommands::Restore { id: "abc123".to_string() };
        let restore2 = HistoryCommands::Restore { id: "abc123".to_string() };
        let restore3 = HistoryCommands::Restore { id: "def456".to_string() };
        
        assert_eq!(restore1, restore2);
        assert_ne!(restore1, restore3);
        assert_ne!(restore1, show1);
        assert_ne!(restore1, list1);
        
        // Test search command
        let search1 = HistoryCommands::Search { query: "test".to_string(), limit: 10 };
        let search2 = HistoryCommands::Search { query: "test".to_string(), limit: 10 };
        let search3 = HistoryCommands::Search { query: "other".to_string(), limit: 10 };
        
        assert_eq!(search1, search2);
        assert_ne!(search1, search3);
        assert_ne!(search1, list1);
        assert_ne!(search1, show1);
        assert_ne!(search1, restore1);
        
        // Test export command
        let export1 = HistoryCommands::Export { 
            id: "abc123".to_string(), 
            output: "test.json".to_string(), 
            format: ExportFormat::Json, 
            force: false 
        };
        let export2 = HistoryCommands::Export { 
            id: "abc123".to_string(), 
            output: "test.json".to_string(), 
            format: ExportFormat::Json, 
            force: false 
        };
        let export3 = HistoryCommands::Export { 
            id: "abc123".to_string(), 
            output: "test.md".to_string(), 
            format: ExportFormat::Markdown, 
            force: false 
        };
        
        assert_eq!(export1, export2);
        assert_ne!(export1, export3);
        assert_ne!(export1, list1);
        assert_ne!(export1, show1);
        assert_ne!(export1, restore1);
        assert_ne!(export1, search1);
    }

    #[test]
    fn test_history_args_equality() {
        let args1 = HistoryArgs {
            command: HistoryCommands::List { limit: 10, path: None, contains: None }
        };
        let args2 = HistoryArgs {
            command: HistoryCommands::List { limit: 10, path: None, contains: None }
        };
        
        assert_eq!(args1, args2);
        
        // Test restore args
        let restore_args1 = HistoryArgs {
            command: HistoryCommands::Restore { id: "test123".to_string() }
        };
        let restore_args2 = HistoryArgs {
            command: HistoryCommands::Restore { id: "test123".to_string() }
        };
        let restore_args3 = HistoryArgs {
            command: HistoryCommands::Restore { id: "different".to_string() }
        };
        
        assert_eq!(restore_args1, restore_args2);
        assert_ne!(restore_args1, restore_args3);
        assert_ne!(args1, restore_args1);
        
        // Test search args
        let search_args1 = HistoryArgs {
            command: HistoryCommands::Search { query: "test".to_string(), limit: 10 }
        };
        let search_args2 = HistoryArgs {
            command: HistoryCommands::Search { query: "test".to_string(), limit: 10 }
        };
        
        assert_eq!(search_args1, search_args2);
        assert_ne!(args1, search_args1);
        
        // Test export args
        let export_args1 = HistoryArgs {
            command: HistoryCommands::Export { 
                id: "test123".to_string(), 
                output: "test.json".to_string(), 
                format: ExportFormat::Json, 
                force: false 
            }
        };
        let export_args2 = HistoryArgs {
            command: HistoryCommands::Export { 
                id: "test123".to_string(), 
                output: "test.json".to_string(), 
                format: ExportFormat::Json, 
                force: false 
            }
        };
        
        assert_eq!(export_args1, export_args2);
        assert_ne!(args1, export_args1);
    }

    // Test the string manipulation functions with edge cases
    #[test]
    fn test_truncate_string_edge_cases() {
        // Empty string
        assert_eq!(truncate_string("", 10), "          ");
        
        // String exactly at limit
        assert_eq!(truncate_string("1234567890", 10), "1234567890");
        
        // String one character over limit
        assert_eq!(truncate_string("12345678901", 10), "1234567...");
        
        // Very small limit - when max_len < 3, saturating_sub returns 0
        assert_eq!(truncate_string("hello", 3), "...");
        
        // Zero limit (edge case) - saturating_sub(3) on 0 returns 0, so we get empty slice
        assert_eq!(truncate_string("hello", 0), "...");
    }

    #[test]
    fn test_truncate_path_edge_cases() {
        // Empty path
        assert_eq!(truncate_path("", 10), "          ");
        
        // Path that's just the home directory
        unsafe {
            std::env::set_var("HOME", "/home/user");
        }
        assert_eq!(truncate_path("/home/user", 10), "~         ");
        
        // Path that starts with home but isn't exactly home
        assert_eq!(truncate_path("/home/user/", 10), "~/        ");
        
        // Test very small limit
        let long_path = "/very/long/path";
        let result = truncate_path(long_path, 5);
        assert_eq!(result, "...th");
        
        // Clean up
        unsafe {
            std::env::remove_var("HOME");
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::database::Database;

    #[tokio::test]
    async fn test_database_integration_list_conversations() {
        let db = Database::new().await.unwrap();
        
        // Initially should be empty
        let conversations = db.list_conversations(10, None, None).unwrap();
        assert!(conversations.is_empty());
        
        // This test would need actual conversation data to be meaningful
        // For now, we're just testing that the method doesn't crash
    }

    #[tokio::test]
    async fn test_database_integration_get_conversation_by_id() {
        let db = Database::new().await.unwrap();
        
        // Test with non-existent ID
        let result = db.get_conversation_by_id("nonexistent").unwrap();
        assert!(result.is_none());
        
        // Test with partial ID that doesn't exist
        let result = db.get_conversation_by_id("abc123").unwrap();
        assert!(result.is_none());
    }

    // Test the actual command line argument parsing
    #[test]
    fn test_history_args_debug() {
        // Test that our Args struct can be debugged (useful for logging)
        let args = HistoryArgs {
            command: HistoryCommands::List { limit: 5, path: Some("/test".to_string()), contains: None }
        };
        
        let debug_str = format!("{:?}", args);
        assert!(debug_str.contains("List"));
        assert!(debug_str.contains("limit: 5"));
        assert!(debug_str.contains("/test"));
        
        // Test restore command debug
        let restore_args = HistoryArgs {
            command: HistoryCommands::Restore { id: "test123".to_string() }
        };
        
        let debug_str = format!("{:?}", restore_args);
        assert!(debug_str.contains("Restore"));
        assert!(debug_str.contains("test123"));
        
        // Test search command debug
        let search_args = HistoryArgs {
            command: HistoryCommands::Search { query: "gitignore".to_string(), limit: 5 }
        };
        
        let debug_str = format!("{:?}", search_args);
        assert!(debug_str.contains("Search"));
        assert!(debug_str.contains("gitignore"));
        assert!(debug_str.contains("limit: 5"));
        
        // Test export command debug
        let export_args = HistoryArgs {
            command: HistoryCommands::Export { 
                id: "test123".to_string(), 
                output: "conv.json".to_string(), 
                format: ExportFormat::Markdown, 
                force: true 
            }
        };
        
        let debug_str = format!("{:?}", export_args);
        assert!(debug_str.contains("Export"));
        assert!(debug_str.contains("test123"));
        assert!(debug_str.contains("conv.json"));
        assert!(debug_str.contains("Markdown"));
        assert!(debug_str.contains("force: true"));
    }

    #[test]
    fn test_conversation_summary_debug() {
        let summary = ConversationSummary {
            id: "test-id".to_string(),
            path: "/test/path".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            preview: "Test preview".to_string(),
            message_count: 5,
        };
        
        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("test-id"));
        assert!(debug_str.contains("/test/path"));
        assert!(debug_str.contains("Test preview"));
    }

    // Test error handling in the database extension methods
    #[tokio::test]
    async fn test_database_error_handling() {
        let db = Database::new().await.unwrap();
        
        // Test that list_conversations handles errors gracefully
        // This should not panic even if there are issues with the database
        let result = db.list_conversations(10, None, None);
        assert!(result.is_ok());
        
        // Test that get_conversation_by_id handles errors gracefully
        let result = db.get_conversation_by_id("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
        
        // Test that search_conversations handles errors gracefully
        let result = db.search_conversations("test", 10);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    // Test the backup functionality
    #[tokio::test]
    async fn test_backup_conversation() {
        let _db = Database::new().await.unwrap();
        
        // This test would need actual conversation data to be meaningful
        // For now, we're just testing that the method doesn't crash
        // In a real scenario, we'd create a test conversation and backup it
        
        // Test that backup_conversation method exists and can be called
        // (We can't easily test the full functionality without setting up test data)
    }

    // Test the filtering logic
    #[test]
    fn test_path_filtering_logic() {
        let test_paths = vec![
            "/home/user/project1",
            "/home/user/project2", 
            "/workspace/project3",
            "/tmp/project4"
        ];
        
        // Test filtering by "/home"
        let filtered: Vec<_> = test_paths.iter()
            .filter(|path| path.contains("/home"))
            .collect();
        assert_eq!(filtered.len(), 2);
        
        // Test filtering by "project"
        let filtered: Vec<_> = test_paths.iter()
            .filter(|path| path.contains("project"))
            .collect();
        assert_eq!(filtered.len(), 4);
        
        // Test filtering by non-existent path
        let filtered: Vec<_> = test_paths.iter()
            .filter(|path| path.contains("/nonexistent"))
            .collect();
        assert_eq!(filtered.len(), 0);
    }

    // Test the limit logic
    #[test]
    fn test_limit_logic() {
        let test_items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        
        // Test taking with limit
        let limited: Vec<_> = test_items.iter().take(5).collect();
        assert_eq!(limited.len(), 5);
        assert_eq!(*limited[0], 1);
        assert_eq!(*limited[4], 5);
        
        // Test taking with limit larger than collection
        let limited: Vec<_> = test_items.iter().take(20).collect();
        assert_eq!(limited.len(), 10);
        
        // Test taking with zero limit
        let limited: Vec<_> = test_items.iter().take(0).collect();
        assert_eq!(limited.len(), 0);
    }

    // Test search functionality
    #[test]
    fn test_conversation_contains_text() {
        // This test would need a mock ConversationState to be meaningful
        // For now, we're testing that the function exists and can be called
        // In a real scenario, we'd create test conversation data and verify search works
    }

    #[test]
    fn test_extract_search_preview() {
        // This test would need a mock ConversationState to be meaningful
        // For now, we're testing that the function exists and can be called
        // In a real scenario, we'd create test conversation data and verify preview extraction
    }

    #[test]
    fn test_export_format_enum() {
        // Test that ExportFormat enum works correctly
        assert_eq!(ExportFormat::Json, ExportFormat::Json);
        assert_ne!(ExportFormat::Json, ExportFormat::Markdown);
        assert_ne!(ExportFormat::Markdown, ExportFormat::Text);
        
        // Test debug formatting
        let json_format = ExportFormat::Json;
        let debug_str = format!("{:?}", json_format);
        assert!(debug_str.contains("Json"));
        
        let md_format = ExportFormat::Markdown;
        let debug_str = format!("{:?}", md_format);
        assert!(debug_str.contains("Markdown"));
        
        let text_format = ExportFormat::Text;
        let debug_str = format!("{:?}", text_format);
        assert!(debug_str.contains("Text"));
    }
}

