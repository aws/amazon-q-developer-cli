//! Command handler for log-related commands in Q CLI chat.
//!
//! This module provides functionality to process log-related commands like
//! `/log show`, `/log --tail N`, etc.

use std::fmt::Write;
use std::sync::Arc;

use chrono::DateTime;
use eyre::{Result, eyre};
use tracing::debug;
use crossterm::style::Stylize;

use super::logger::LogManager;

/// Handler for log-related commands.
#[derive(Debug)]
pub struct LogCommandHandler {
    /// The log manager instance.
    log_manager: Arc<LogManager>,
}

/// Command options for log commands.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct LogCommandOptions {
    /// Number of entries to show (default: 10).
    pub count: usize,
    
    /// Whether to show all entries.
    pub show_all: bool,
    
    /// Number of entries to show from the end.
    pub tail: Option<usize>,
    
    /// Number of entries to show from the beginning.
    pub head: Option<usize>,
    
    /// Whether to show entries in descending order.
    pub desc: bool,
    
    /// Whether to show only user prompts.
    pub only_user_prompts: bool,
}

impl LogCommandHandler {
    /// Create a new LogCommandHandler.
    ///
    /// # Arguments
    /// * `log_manager` - The log manager instance
    ///
    /// # Returns
    /// A new LogCommandHandler
    pub fn new(log_manager: Arc<LogManager>) -> Self {
        Self { log_manager }
    }
  
    /// Handle a log command.
    ///
    /// # Arguments
    /// * `args` - The command arguments
    ///
    /// # Returns
    /// A Result containing the command output or an error
    pub async fn handle_command(&self, args: &[&str]) -> Result<String> {
        if args.is_empty() {
            return self.handle_show_command(&LogCommandOptions::default()).await;
        }
        
        match args[0] {
            "show" => {
                let options = self.parse_show_options(&args[1..])?;
                self.handle_show_command(&options).await
            },
            "enable" => self.handle_enable_command().await,
            "disable" => self.handle_disable_command().await,
            "delete" => self.handle_delete_command().await,
            _ if args[0].starts_with("--tail") => {
                let count = self.parse_count_option(args[0], "--tail")?;
                let options = LogCommandOptions {
                    tail: Some(count),
                    ..Default::default()
                };
                self.handle_show_command(&options).await
            },
            _ if args[0].starts_with("--head") => {
                let count = self.parse_count_option(args[0], "--head")?;
                let options = LogCommandOptions {
                    head: Some(count),
                    ..Default::default()
                };
                self.handle_show_command(&options).await
            },
            _ => Err(eyre!("Unknown log command: {}. Available commands: enable, disable, show, delete, --tail N, --head N", args[0]))
        }
    }
    
    /// Parse options for the show command.
    ///
    /// # Arguments
    /// * `args` - The command arguments
    ///
    /// # Returns
    /// A Result containing the parsed options or an error
    fn parse_show_options(&self, args: &[&str]) -> Result<LogCommandOptions> {
        let mut options = LogCommandOptions::default();
        
        for arg in args {
            match *arg {
                "--all" => options.show_all = true,
                "--desc" => options.desc = true,
                "--only-user-prompts" => options.only_user_prompts = true,
                arg if arg.starts_with("--tail=") => {
                    let count = self.parse_count_option(arg, "--tail=")?;
                    options.tail = Some(count);
                },
                arg if arg.starts_with("--head=") => {
                    let count = self.parse_count_option(arg, "--head=")?;
                    options.head = Some(count);
                },
                arg if arg.starts_with("--count=") => {
                    options.count = self.parse_count_option(arg, "--count=")?;
                }
                _ => return Err(eyre!("Unknown option: {}. Available options: --all, --desc, --only-user-prompts, --tail=N, --head=N, --count=N", arg))
            }
        }
        
        Ok(options)
    }
    
    /// Parse a count option.
    ///
    /// # Arguments
    /// * `arg` - The argument string
    /// * `prefix` - The prefix to remove
    ///
    /// # Returns
    /// A Result containing the parsed count or an error
    fn parse_count_option(&self, arg: &str, prefix: &str) -> Result<usize> {
        let count_str = if arg == prefix {
            return Err(eyre!("Missing count value for {}", prefix));
        } else if arg.starts_with(&format!("{}=", prefix)) {
            &arg[prefix.len() + 1..]
        } else {
            &arg[prefix.len()..]
        };
        
        count_str.parse::<usize>()
            .map_err(|_| eyre!("Invalid count value: {}. Expected a positive integer.", count_str))
    }
    
    /// Handle the show command.
    ///
    /// # Arguments
    /// * `options` - The command options
    ///
    /// # Returns
    /// A Result containing the command output or an error
    async fn handle_show_command(&self, options: &LogCommandOptions) -> Result<String> {
        // Check if logging is enabled
        if !self.log_manager.config.enabled {
            return Ok("Logging is not enabled. Use '/log enable' to enable logging for this session or restart Q with 'q --enable-logging'.".to_string());
        }
        
        let file_exists = self.log_manager.ctx.fs().exists(&self.log_manager.log_file_path);
        
        if !file_exists {
            return Ok("No log file found. Try logging some interactions first.".to_string());
        }
        
        // Use default count of 10 if not specified
        let count = if options.count == 0 { 10 } else { options.count };
        
        let mut entries = self.log_manager.get_log_entries(
            count,
            options.show_all,
            options.tail,
            options.head,
            options.desc,
        ).await?;

        // Filter entries if only-user-prompts option is set
        if options.only_user_prompts {
            entries.retain(|entry| entry.prompt.contains("Prompt { prompt:"));
        }
        
        if entries.is_empty() {
            return Ok("No log entries found.".to_string());
        }
        
        // Define column widths for the table
        let entry_col_width = 6;      // Entry#
        let time_col_width = 19;      // Timestamp
        let resp_time_col_width = 12; // Response Time
        let prompt_col_width = 70;    // Prompt (will wrap)
        let response_col_width = 70;  // Response (will wrap)
        let context_col_width = 50;   // Context Files (will wrap)
        
        // Calculate total table width
        let total_width = entry_col_width + time_col_width + resp_time_col_width + 
                         prompt_col_width + response_col_width + context_col_width + 7; // +7 for separators
        
        let mut output = String::new();
        writeln!(output, "Log entries ({}):", entries.len())?;
        
        // Table header
        writeln!(output, "{}", "═".repeat(total_width))?;
        writeln!(output, "{}│{}│{}│{}│{}│{}", 
            format!("{:^width$}", "Entry#", width = entry_col_width).green().bold(),
            format!("{:^width$}", "Timestamp", width = time_col_width).green().bold(),
            format!("{:^width$}", "Resp Time", width = resp_time_col_width).green().bold(),
            format!("{:^width$}", "Prompt", width = prompt_col_width).green().bold(),
            format!("{:^width$}", "Response", width = response_col_width).green().bold(),
            format!("{:^width$}", "Context Files", width = context_col_width).green().bold()
        )?;
        writeln!(output, "{}", "═".repeat(total_width))?;
        
        // Helper function to wrap text to a specific width
        fn wrap_text(text: &str, width: usize) -> Vec<String> {
            let mut lines = Vec::new();
            let mut current_line = String::new();
            
            for word in text.split_whitespace() {
                if current_line.len() + word.len() + 1 <= width {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                } else {
                    if !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = String::new();
                    }
                    
                    // Handle words longer than width
                    if word.len() > width {
                        let mut chars = word.chars();
                        while current_line.len() < width && chars.next().is_some() {
                            current_line.push(chars.next().unwrap_or(' '));
                        }
                        lines.push(current_line);
                        current_line = String::new();
                        
                        let remaining: String = chars.collect();
                        if !remaining.is_empty() {
                            current_line = remaining;
                        }
                    } else {
                        current_line = word.to_string();
                    }
                }
            }
            
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            
            if lines.is_empty() {
                lines.push(String::new());
            }
            
            lines
        }
        
        for (i, entry) in entries.iter().enumerate() {
            let datetime = DateTime::parse_from_rfc3339(&entry.timestamp)
                .map_err(|_| eyre!("Invalid timestamp format"))?;
            let formatted_time = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
            
            // Wrap text for prompt and response
            let prompt_lines = wrap_text(&entry.prompt, prompt_col_width);
            let response_lines = wrap_text(&entry.response_summary, response_col_width);
            
            // Extract just the filenames from paths
            let filenames: Vec<String> = entry.context_files.iter()
                .map(|path| {
                    let path_buf = std::path::PathBuf::from(path);
                    path_buf.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.clone())
                })
                .collect();
            
            let context_text = filenames.join(", ");
            let context_lines = wrap_text(&context_text, context_col_width);
            
            // Determine the maximum number of lines needed
            let max_lines = prompt_lines.len().max(response_lines.len()).max(context_lines.len());
            
            // Print each row of the table
            for line_idx in 0..max_lines {
                let entry_num = if line_idx == 0 { 
                    format!("{}", i + 1) 
                } else { 
                    String::new() 
                };
                
                let timestamp = if line_idx == 0 { 
                    formatted_time.clone() 
                } else { 
                    String::new() 
                };
                
                let resp_time = if line_idx == 0 { 
                    format!("{:.2}s", entry.response_time_seconds) 
                } else { 
                    String::new() 
                };
                
                let prompt_line = prompt_lines.get(line_idx).cloned().unwrap_or_default();
                let response_line = response_lines.get(line_idx).cloned().unwrap_or_default();
                let context_line = context_lines.get(line_idx).cloned().unwrap_or_default();
                
                writeln!(output, "{}│{}│{}│{}│{}│{}", 
                    format!("{:^width$}", entry_num, width = entry_col_width),
                    format!("{:^width$}", timestamp, width = time_col_width),
                    format!("{:^width$}", resp_time, width = resp_time_col_width),
                    format!("{:<width$}", prompt_line, width = prompt_col_width),
                    format!("{:<width$}", response_line, width = response_col_width),
                    format!("{:<width$}", context_line, width = context_col_width)
                )?;
            }
            
            writeln!(output, "{}", "─".repeat(total_width))?;
        }
        
        Ok(output)
    }
    
    /// Handle the delete command.
    ///
    /// # Returns
    /// A Result containing the command output or an error
    async fn handle_delete_command(&self) -> Result<String> {
        // Check if logging is enabled
        if !self.log_manager.config.enabled {
            return Ok("Logging is not enabled. Use '/log enable' to enable logging for this session or restart Q with 'q --enable-logging'.".to_string());
        }
        
        let deleted = self.log_manager.delete_current_session_logs().await?;
        
        if deleted {
            Ok("All log entries for the current session have been deleted.".to_string())
        } else {
            Ok("No log entries found to delete.".to_string())
        }
    }

    /// Handle the enable command.
    ///
    /// This function enables logging for the current session.
    ///
    /// # Returns
    /// A Result containing the command output or an error
    async fn handle_enable_command(&self) -> Result<String> {
        // Clone the Arc to get a mutable reference to the LogManager
        let log_manager = Arc::clone(&self.log_manager);
        
        // Get a mutable reference to the LogManager
        // This is safe because we're only modifying the config, not the underlying file structure
        let log_manager_mut = unsafe { &mut *(Arc::as_ptr(&log_manager) as *mut LogManager) };
        
        // Enable logging without checking if it's already enabled
        log_manager_mut.config.enabled = true;
        
        // Create a block to handle the async operations
        {
            // Initialize the log file if needed
            let file_exists = log_manager_mut.ctx.fs().exists(&log_manager_mut.log_file_path);
            if !file_exists {
                log_manager_mut.ctx.fs().write(&log_manager_mut.log_file_path, "").await?;
            }
        }
        
        debug!("Logging enabled for the current session");
        
        Ok("Logging has been enabled for the current session. All prompts and responses will be logged.".to_string())
    }

    /// Handle the disable command.
    ///
    /// This function disables logging for the current session.
    ///
    /// # Returns
    /// A Result containing the command output or an error
    async fn handle_disable_command(&self) -> Result<String> {
        // Clone the Arc to get a mutable reference to the LogManager
        let log_manager = Arc::clone(&self.log_manager);
        
        // Get a mutable reference to the LogManager
        // This is safe because we're only modifying the config, not the underlying file structure
        let log_manager_mut = unsafe { &mut *(Arc::as_ptr(&log_manager) as *mut LogManager) };
        
        // Disable logging directly
        log_manager_mut.config.enabled = false;
        
        debug!("Logging disabled for the current session");
        
        Ok("Logging has been disabled for the current session. No further prompts and responses will be logged.".to_string())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logger::LogManager;
    use fig_os_shim::Context;
    
    // Helper function to create a test LogManager and LogCommandHandler
    async fn create_test_handler() -> Result<(Arc<LogManager>, LogCommandHandler)> {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let manager = LogManager::new(ctx, true).await?;
        let arc_manager = Arc::new(manager);
        let handler = LogCommandHandler::new(Arc::clone(&arc_manager));
        
        Ok((arc_manager, handler))
    }
    
    #[tokio::test]
    async fn test_handle_show_command_empty() -> Result<()> {
        let (_, handler) = create_test_handler().await?;
        
        let result = handler.handle_command(&["show"]).await?;
        assert!(result.contains("No log entries found"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_handle_show_command_with_entries() -> Result<()> {
        let (manager, handler) = create_test_handler().await?;
        
        // Add some log entries
        manager.log_interaction(
            "Test prompt 1".to_string(),
            "Test response 1".to_string(),
            1.0,
            vec![],
        ).await?;
        
        manager.log_interaction(
            "Test prompt 2".to_string(),
            "Test response 2".to_string(),
            2.0,
            vec!["test.md".to_string()],
        ).await?;
        
        // Test default show command
        let result = handler.handle_command(&["show"]).await?;
        assert!(result.contains("Log entries (2):"));
        assert!(result.contains("Test prompt 1"));
        assert!(result.contains("Test prompt 2"));
        
        // Test with --all option
        let result = handler.handle_command(&["show", "--all"]).await?;
        assert!(result.contains("Log entries (2):"));
        
        // Test with --desc option
        let result = handler.handle_command(&["show", "--desc"]).await?;
        assert!(result.contains("Log entries (2):"));
        
        // Test with --tail option
        let result = handler.handle_command(&["show", "--tail=1"]).await?;
        assert!(result.contains("Log entries (1):"));
        assert!(result.contains("Test prompt 2"));
        assert!(!result.contains("Test prompt 1"));
        
        // Test with --head option
        let result = handler.handle_command(&["show", "--head=1"]).await?;
        assert!(result.contains("Log entries (1):"));
        assert!(result.contains("Test prompt 1"));
        assert!(!result.contains("Test prompt 2"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_handle_delete_command() -> Result<()> {
        let (manager, handler) = create_test_handler().await?;
        
        // Add a log entry
        manager.log_interaction(
            "Test prompt".to_string(),
            "Test response".to_string(),
            1.0,
            vec![],
        ).await?;
        
        // Verify the entry was created
        let entries = manager.get_log_entries(10, false, None, None, false).await?;
        assert_eq!(entries.len(), 1);
        
        // Delete the entries
        let result = handler.handle_command(&["delete"]).await?;
        assert!(result.contains("All log entries for the current session have been deleted"));
        
        // Verify the entries were deleted
        let entries = manager.get_log_entries(10, false, None, None, false).await?;
        assert_eq!(entries.len(), 0);
        
        // Try deleting again
        let result = handler.handle_command(&["delete"]).await?;
        assert!(result.contains("No log entries found to delete"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_handle_tail_command() -> Result<()> {
        let (manager, handler) = create_test_handler().await?;
        
        // Add some log entries
        for i in 0..5 {
            manager.log_interaction(
                format!("Test prompt {}", i),
                format!("Test response {}", i),
                1.0,
                vec![],
            ).await?;
        }
        
        // Test --tail command
        let result = handler.handle_command(&["--tail", "2"]).await?;
        assert!(result.contains("Log entries (2):"));
        assert!(result.contains("Test prompt 3"));
        assert!(result.contains("Test prompt 4"));
        assert!(!result.contains("Test prompt 2"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_handle_head_command() -> Result<()> {
        let (manager, handler) = create_test_handler().await?;
        
        // Add some log entries
        for i in 0..5 {
            manager.log_interaction(
                format!("Test prompt {}", i),
                format!("Test response {}", i),
                1.0,
                vec![],
            ).await?;
        }
        
        // Test --head command
        let result = handler.handle_command(&["--head", "2"]).await?;
        assert!(result.contains("Log entries (2):"));
        assert!(result.contains("Test prompt 0"));
        assert!(result.contains("Test prompt 1"));
        assert!(!result.contains("Test prompt 2"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_invalid_command() -> Result<()> {
        let (_, handler) = create_test_handler().await?;
        
        let result = handler.handle_command(&["invalid"]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown log command"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_invalid_options() -> Result<()> {
        let (_, handler) = create_test_handler().await?;
        
        let result = handler.handle_command(&["show", "--invalid"]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown option"));
        
        let result = handler.handle_command(&["--tail"]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing count value"));
        
        let result = handler.handle_command(&["--tail=abc"]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid count value"));
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_handle_only_user_prompts_option() -> Result<()> {
        let (manager, handler) = create_test_handler().await?;
        
        // Add some log entries with different formats
        manager.log_interaction(
            "Regular prompt".to_string(),
            "Test response 1".to_string(),
            1.0,
            vec![],
        ).await?;
        
        manager.log_interaction(
            "Prompt { prompt: User question }".to_string(),
            "Test response 2".to_string(),
            2.0,
            vec![],
        ).await?;
        
        manager.log_interaction(
            "Another regular prompt".to_string(),
            "Test response 3".to_string(),
            1.5,
            vec![],
        ).await?;
        
        manager.log_interaction(
            "Prompt { prompt: Another user question }".to_string(),
            "Test response 4".to_string(),
            2.5,
            vec![],
        ).await?;
        
        // Test with --only-user-prompts option
        let result = handler.handle_command(&["show", "--only-user-prompts"]).await?;
        assert!(result.contains("Log entries (2):"));
        assert!(result.contains("Prompt { prompt: User question }"));
        assert!(result.contains("Prompt { prompt: Another user question }"));
        assert!(!result.contains("Regular prompt"));
        assert!(!result.contains("Another regular prompt"));
        
        // Test with combined options
        let result = handler.handle_command(&["show", "--only-user-prompts", "--head=1"]).await?;
        assert!(result.contains("Log entries (1):"));
        assert!(result.contains("Prompt { prompt: User question }"));
        assert!(!result.contains("Prompt { prompt: Another user question }"));
        
        Ok(())
    }
}
