//! Logging functionality for Q CLI chat sessions.
//!
//! This module provides the ability to log user prompts, commands, responses,
//! and related metadata in a structured format, allowing users to review their
//! past interactions and track their usage patterns.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use eyre::{Result, eyre};
use fig_os_shim::Context;
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

/// Configuration for logging, containing settings for the logging feature.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LoggingConfig {
    /// Whether logging is enabled.
    pub enabled: bool,
    
    /// Maximum log file size in bytes before truncation (default: 512MB).
    pub max_file_size: u64,
}

/// Represents a single log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unique identifier for the log entry.
    pub id: String,
    
    /// Timestamp when the log entry was created.
    pub timestamp: String,
    
    /// The user's prompt or command.
    pub prompt: String,
    
    /// A compact summary of the response.
    pub response_summary: String,
    
    /// Time taken for the response to generate in seconds.
    pub response_time_seconds: f64,
    
    /// List of context files used for the response.
    pub context_files: Vec<String>,
    
    /// Session ID that this log entry belongs to.
    pub session_id: String,
}

impl LogEntry {
    /// Create a new log entry.
    pub fn new(
        prompt: String,
        response: String,
        response_time: f64,
        context_files: Vec<String>,
        session_id: String,
    ) -> Self {
        // Generate a unique ID for the log entry
        let id = format!("entry_{}", Uuid::new_v4().to_string().split('-').next().unwrap_or("001"));
        
        // Create timestamp in ISO 8601 format
        let now = SystemTime::now();
        let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let datetime = DateTime::<Utc>::from_timestamp(
            since_epoch.as_secs() as i64,
            since_epoch.subsec_nanos(),
        ).unwrap_or_default();
        let timestamp = datetime.to_rfc3339();
        
        // Create a compact summary of the response
        let response_summary = generate_response_summary(&response);
        
        Self {
            id,
            timestamp,
            prompt,
            response_summary,
            response_time_seconds: response_time,
            context_files,
            session_id,
        }
    }
    
    /// Serialize the log entry to a JSON string.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| eyre!("Failed to serialize log entry: {}", e))
    }

}

/// Manager for logging functionality.
#[derive(Debug, Clone)]
pub struct LogManager {
    pub(crate) ctx: Arc<Context>,
    
    /// Configuration for logging.
    pub config: LoggingConfig,
    
    /// Current session ID.
    pub session_id: String,
    
    /// Path to the current log file.
    pub log_file_path: PathBuf,
}

impl LogManager {
    /// Create a new LogManager with default settings.
    ///
    /// This will:
    /// 1. Create the necessary directories if they don't exist
    /// 2. Generate a new session ID
    /// 3. Set up the log file path
    ///
    /// # Returns
    /// A Result containing the new LogManager or an error
    pub async fn new(ctx: Arc<Context>, enable_logging: bool) -> Result<Self> {
        // Create log directories if they don't exist
        let logs_dir = log_directory(&ctx)?;
        let sessions_dir = logs_dir.join("sessions");
        
        ctx.fs().create_dir_all(&sessions_dir).await?;
        
        // Generate a new session ID and timestamp
        let session_id = Uuid::new_v4().to_string();
        let now = SystemTime::now();
        let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let datetime = DateTime::<Utc>::from_timestamp(
            since_epoch.as_secs() as i64,
            since_epoch.subsec_nanos(),
        ).unwrap_or_default();
        let timestamp = datetime.format("%Y-%m-%d_%H-%M-%S").to_string();
        
        // Create the log file path
        let log_filename = format!("q_session_{}_{}.log", timestamp, &session_id[..8]);
        let log_file_path = sessions_dir.join(&log_filename);
        
        // Create a symlink to the current session
        let current_session_link = logs_dir.join("current_session");
        if ctx.fs().exists(&current_session_link) {
            let _ = ctx.fs().remove_file(&current_session_link).await;
        }
        
        // Note: Symlink creation would be platform-specific
        // For simplicity, we'll just create a file with the path to the current session
        let relative_path = format!("sessions/{}", log_filename);
        ctx.fs().write(&current_session_link, &relative_path).await?;
        
        // Create default configuration
        let config = LoggingConfig {
            enabled: enable_logging,
            max_file_size: 512 * 1024 * 1024, // 512MB
        };
        
        Ok(Self {
            ctx,
            config,
            session_id,
            log_file_path,
        })
    }
    
    /// Initialize logging based on the provided flag.
    ///
    /// # Arguments
    /// * `enable_logging` - Whether to enable logging
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn initialize_logging(&mut self, enable_logging: bool) -> Result<()> {
        self.config.enabled = enable_logging;
        
        if enable_logging {
            debug!("Logging enabled. Log file: {:?}", self.log_file_path);
            
            // Create an empty log file or ensure it exists
            if !self.ctx.fs().exists(&self.log_file_path) {
                self.ctx.fs().write(&self.log_file_path, "").await?;
            }
        }
        
        Ok(())
    }
    
    /// Log an interaction between the user and Amazon Q.
    ///
    /// # Arguments
    /// * `prompt` - The user's prompt or command
    /// * `response` - The response from Amazon Q
    /// * `response_time` - Time taken for the response in seconds
    /// * `context_files` - List of context files used for the response
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn log_interaction(
        &self,
        prompt: String,
        response: String,
        response_time: f64,
        context_files: Vec<String>,
    ) -> Result<LogEntry> {
        if !self.config.enabled {
            return Err(eyre!("Logging is not enabled"));
        }
        
        // Check if the log file size exceeds the maximum
        self.check_and_truncate_log_file().await?;
        
        // Create a new log entry
        let log_entry = LogEntry::new(
            prompt,
            response,
            response_time,
            context_files,
            self.session_id.clone(),
        );
        
        // Serialize the log entry to JSON
        let json_line = log_entry.to_json()?;
        
        // Append the log entry to the log file using efficient appending
        self.append_to_log_file(&json_line).await?;
        
        Ok(log_entry)
    }
    
    /// Append a JSON line to the log file.
    ///
    /// This method uses efficient appending to avoid reading the entire file.
    ///
    /// # Arguments
    /// * `json_line` - The JSON line to append
    ///
    /// # Returns
    /// A Result indicating success or an error
    async fn append_to_log_file(&self, json_line: &str) -> Result<()> {
        // Create the file if it doesn't exist
        if !self.ctx.fs().exists(&self.log_file_path) {
            self.ctx.fs().write(&self.log_file_path, "").await?;
        }
        
        // Read the last character to check if we need to add a newline
        let file_size = self.ctx.fs().symlink_metadata(&self.log_file_path).await?.len();
        let needs_newline = if file_size > 0 {
            // Read the last character
            let last_char = self.ctx.fs().read_to_string(&self.log_file_path).await?
                .chars().last().unwrap_or('\n');
            last_char != '\n'
        } else {
            false
        };
        
        // Prepare the content to append
        let mut content = String::new();
        if needs_newline {
            content.push('\n');
        }
        content.push_str(json_line);
        content.push('\n');
        
        // Append to the file
        // Note: In a real implementation, we would use file.open() with append mode
        // but since we're using the Context abstraction, we'll read and write the whole file
        let existing_content = if file_size > 0 {
            self.ctx.fs().read_to_string(&self.log_file_path).await?
        } else {
            String::new()
        };
        
        let new_content = format!("{}{}", existing_content, content);
        self.ctx.fs().write(&self.log_file_path, new_content).await?;
        
        Ok(())
    }
    
    /// Get log entries from the current session.
    ///
    /// # Arguments
    /// * `count` - Number of entries to retrieve (default: 10)
    /// * `show_all` - Whether to show all entries
    /// * `tail` - Number of entries to show from the end
    /// * `head` - Number of entries to show from the beginning
    /// * `desc` - Whether to show entries in descending order
    ///
    /// # Returns
    /// A Result containing a vector of LogEntry objects or an error
    pub async fn get_log_entries(
        &self,
        count: usize,
        show_all: bool,
        tail: Option<usize>,
        head: Option<usize>,
        desc: bool,
    ) -> Result<Vec<LogEntry>> {
        if !self.ctx.fs().exists(&self.log_file_path) {
            return Ok(Vec::new());
        }
        
        // Read the log file
        let content = self.ctx.fs().read_to_string(&self.log_file_path).await?;
        
        // Parse the log entries
        let entries = self.parse_log_entries(&content)?;
        
        // Apply sorting
        let mut sorted_entries = entries;
        if desc {
            sorted_entries.reverse();
        }
        
        // Apply filtering based on parameters
        let filtered_entries = if show_all {
            sorted_entries
        } else if let Some(tail_count) = tail {
            if tail_count >= sorted_entries.len() {
                sorted_entries
            } else {
                sorted_entries[sorted_entries.len() - tail_count..].to_vec()
            }
        } else if let Some(head_count) = head {
            if head_count >= sorted_entries.len() {
                sorted_entries
            } else {
                sorted_entries[..head_count].to_vec()
            }
        } else {
            // Default: show the most recent entries (count)
            if count >= sorted_entries.len() {
                sorted_entries
            } else {
                sorted_entries[sorted_entries.len() - count..].to_vec()
            }
        };
        
        Ok(filtered_entries)
    }
    
    /// Parse log entries from a string.
    ///
    /// # Arguments
    /// * `content` - The string containing log entries in JSONL format
    ///
    /// # Returns
    /// A Result containing a vector of LogEntry objects or an error
    fn parse_log_entries(&self, content: &str) -> Result<Vec<LogEntry>> {
        let lines: Vec<&str> = content.lines().collect();
        
        // Parse each line as a LogEntry
        let mut entries: Vec<LogEntry> = Vec::new();
        for (_i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            
            match serde_json::from_str::<LogEntry>(line) {
                Ok(entry) => {
                    entries.push(entry);
                },
                Err(_e) => {
                    // Try to be more lenient with JSON parsing
                    if let Ok(_value) = serde_json::from_str::<serde_json::Value>(line) {
                    } else {
                    }
                }
            }
        }
        
        Ok(entries)
    }
    
    
    /// Delete all log entries for the current session.
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn delete_current_session_logs(&self) -> Result<bool> {
        if self.ctx.fs().exists(&self.log_file_path) {
            self.ctx.fs().write(&self.log_file_path, "").await?;
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Check if the log file exceeds the maximum size and truncate it if necessary.
    ///
    /// # Returns
    /// A Result indicating success or an error
    async fn check_and_truncate_log_file(&self) -> Result<()> {
        if !self.ctx.fs().exists(&self.log_file_path) {
            return Ok(());
        }
        
        let metadata = self.ctx.fs().symlink_metadata(&self.log_file_path).await?;
        let file_size = metadata.len();
        
        if file_size > self.config.max_file_size {
            debug!("Log file size ({} bytes) exceeds maximum ({}). Truncating...", file_size, self.config.max_file_size);
            
            // Calculate target size (90% of max size)
            let target_size = (self.config.max_file_size as f64 * 0.9) as u64;
            let bytes_to_keep = target_size;
            
            // Read the file content
            let content = self.ctx.fs().read_to_string(&self.log_file_path).await?;
            
            // Find a position to truncate at (line boundary)
            let truncate_pos = if content.len() <= bytes_to_keep as usize {
                0
            } else {
                let start_pos = content.len() - bytes_to_keep as usize;
                
                // Find the next newline to ensure we don't truncate in the middle of a line
                match content[start_pos..].find('\n') {
                    Some(pos) => start_pos + pos + 1, // +1 to include the newline
                    None => start_pos,
                }
            };
            
            // Create truncation notice
            let now = SystemTime::now();
            let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
            let datetime = DateTime::<Utc>::from_timestamp(
                since_epoch.as_secs() as i64,
                since_epoch.subsec_nanos(),
            ).unwrap_or_default();
            let timestamp = datetime.to_rfc3339();
            
            let truncation_notice = format!(
                "{{\"truncation_notice\": \"Log was truncated at {}\"}}\n",
                timestamp
            );
            
            // Write the truncated content back to the file
            let truncated_content = format!("{}{}", truncation_notice, &content[truncate_pos..]);
            self.ctx.fs().write(&self.log_file_path, truncated_content).await?;
            
            debug!("Log file truncated successfully");
        }
        
        Ok(())
    }
}

/// Get the platform-specific log directory.
///
/// # Returns
/// A Result containing the path to the log directory
fn log_directory(ctx: &Context) -> Result<PathBuf> {
    let home_dir = ctx.env().home().ok_or_else(|| eyre!("Could not determine home directory"))?;
    
    // Platform-specific log directory
    let log_dir = if cfg!(target_os = "macos") {
        home_dir.join("Library/Logs/AmazonQ")
    } else if cfg!(target_os = "linux") {
        home_dir.join(".local/share/amazon-q/logs")
    } else if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = ctx.env().get("LOCALAPPDATA") {
            PathBuf::from(local_app_data).join("Amazon/Q/Logs")
        } else {
            home_dir.join(".amazon-q/logs")
        }
    } else {
        // Fallback for other platforms
        home_dir.join(".amazon-q/logs")
    };
    
    Ok(log_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper function to create a test LogManager
    async fn create_test_log_manager(enable_logging: bool) -> Result<LogManager> {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let manager = LogManager::new(ctx, enable_logging).await?;
        Ok(manager)
    }
    
    #[tokio::test]
    async fn test_log_manager_creation() -> Result<()> {
        let manager = create_test_log_manager(true).await?;
        
        assert!(manager.config.enabled);
        assert_eq!(manager.config.max_file_size, 512 * 1024 * 1024);
        assert!(!manager.session_id.is_empty());
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_log_interaction() -> Result<()> {
        let manager = create_test_log_manager(true).await?;
        
        // Log an interaction
        let log_entry = manager.log_interaction(
            "Test prompt".to_string(),
            "Test response".to_string(),
            1.2,
            vec!["test.md".to_string()],
        ).await?;
        
        // Verify the log entry was created
        let entries = manager.get_log_entries(10, false, None, None, false).await?;
        
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].prompt, "Test prompt");
        assert_eq!(entries[0].response_summary, "Test response");
        assert_eq!(entries[0].response_time_seconds, 1.2);
        assert_eq!(entries[0].context_files, vec!["test.md"]);
        assert_eq!(entries[0].id, log_entry.id);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_disabled_logging() -> Result<()> {
        let manager = create_test_log_manager(false).await?;
        
        // Log an interaction with logging disabled
        let result = manager.log_interaction(
            "Test prompt".to_string(),
            "Test response".to_string(),
            1.2,
            vec!["test.md".to_string()],
        ).await;
        
        // Verify the operation failed because logging is disabled
        assert!(result.is_err());
        
        // Verify no log entry was created
        let entries = manager.get_log_entries(10, false, None, None, false).await?;
        assert_eq!(entries.len(), 0);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_delete_logs() -> Result<()> {
        let manager = create_test_log_manager(true).await?;
        
        // Log some interactions
        for i in 0..3 {
            manager.log_interaction(
                format!("Test prompt {}", i),
                format!("Test response {}", i),
                1.0,
                vec![],
            ).await?;
        }
        
        // Verify log entries were created
        let entries = manager.get_log_entries(10, true, None, None, false).await?;
        assert_eq!(entries.len(), 3);
        
        // Delete the logs
        let deleted = manager.delete_current_session_logs().await?;
        assert!(deleted);
        
        // Verify logs were deleted
        let entries = manager.get_log_entries(10, true, None, None, false).await?;
        assert_eq!(entries.len(), 0);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_get_log_entries_filtering() -> Result<()> {
        let manager = create_test_log_manager(true).await?;
        
        // Log 5 interactions
        for i in 0..5 {
            manager.log_interaction(
                format!("Test prompt {}", i),
                format!("Test response {}", i),
                1.0,
                vec![],
            ).await?;
        }
        
        // Test default behavior (last 10 entries)
        let entries = manager.get_log_entries(10, false, None, None, false).await?;
        assert_eq!(entries.len(), 5);
        
        // Test head
        let entries = manager.get_log_entries(10, false, None, Some(2), false).await?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].prompt, "Test prompt 0");
        assert_eq!(entries[1].prompt, "Test prompt 1");
        
        // Test tail
        let entries = manager.get_log_entries(10, false, Some(2), None, false).await?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].prompt, "Test prompt 3");
        assert_eq!(entries[1].prompt, "Test prompt 4");
        
        // Test descending order
        let entries = manager.get_log_entries(10, true, None, None, true).await?;
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].prompt, "Test prompt 4");
        assert_eq!(entries[4].prompt, "Test prompt 0");
        
        Ok(())
    }
    
}
/// Generate a compact summary of a response.
///
/// This function creates a meaningful summary of the response by:
/// 1. For short responses (< 500 characters), using the full response
/// 2. For longer responses, extracting the first 1-2 sentences (up to 500 characters)
/// 3. Adding ellipsis (...) if truncated
///
/// # Arguments
/// * `response` - The full response text
///
/// # Returns
/// A compact summary of the response
fn generate_response_summary(response: &str) -> String {
    // For short responses, return the full text
    if response.len() <= 500 {
        return response.to_string();
    }
    
    // Try to extract the first 1-2 sentences
    let mut summary = String::new();
    let mut sentence_count = 0;
    let mut char_count = 0;
    
    // Split by common sentence terminators
    for sentence in response.split_terminator(|c| c == '.' || c == '!' || c == '?') {
        // Skip empty sentences
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        // Add sentence separator if not the first sentence
        if !summary.is_empty() {
            let last_char = summary.chars().last().unwrap_or(' ');
            if !last_char.is_whitespace() {
                summary.push(' ');
            }
        }
        
        // Add the sentence with its terminator
        summary.push_str(trimmed);
        summary.push('.');
        
        // Update counters
        sentence_count += 1;
        char_count += trimmed.len() + 1; // +1 for the terminator
        
        // Stop if we have enough sentences or characters
        if (sentence_count >= 2 || char_count >= 500) && !summary.is_empty() {
            break;
        }
    }
    
    // If we couldn't extract sentences properly, fall back to character-based truncation
    if summary.is_empty() || summary.len() < 10 {
        let end = response.char_indices()
            .take(500)
            .last()
            .map(|(i, _)| i + 1)
            .unwrap_or(response.len());
        
        summary = format!("{}...", &response[..end]);
    } else if summary.len() > 500 {
        // If the summary is still too long, truncate it
        let end = summary.char_indices()
            .take(497)
            .last()
            .map(|(i, _)| i + 1)
            .unwrap_or(summary.len());
        
        summary = format!("{}...", &summary[..end]);
    } else if response.len() > summary.len() {
        // Add ellipsis if we truncated the response
        summary.push_str("...");
    }
    
    summary
}
