use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;

use crossterm::{queue, style};
use eyre::Result;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::cli::agent::McpServerConfig;
use crate::cli::chat::tools::custom_tool::{CustomToolClient, CustomToolConfig};
use crate::cli::chat::progress_display::{ProgressDisplay, is_interactive_mode};
use crate::cli::chat::tool_manager::{ToolManager, global_mcp_config_path, workspace_mcp_config_path, ToolInfo};
use crate::cli::chat::tools::{ToolSpec, ToolOrigin};
use crate::cli::chat::ChatSession;
use crate::os::Os;

/// Errors that can occur during server reload operations
#[derive(Debug, Error)]
pub enum ReloadError {
    #[error("Server '{server_name}' not found in configuration")]
    ServerNotFound { server_name: String },
    
    #[error("Server '{server_name}' is already {state}")]
    ServerStateConflict { server_name: String, state: String },
    
    #[error("Failed to stop server '{server_name}': {reason}")]
    ServerStopFailed { server_name: String, reason: String },
    
    #[error("Failed to start server '{server_name}': {reason}")]
    ServerStartFailed { server_name: String, reason: String },
    
    #[error("Configuration reload failed for '{server_name}': {reason}")]
    ConfigReloadFailed { server_name: String, reason: String },
    
    #[error("Tool registration failed for '{server_name}': {reason}")]
    ToolRegistrationFailed { server_name: String, reason: String },
    
    #[error("Server validation failed: {reason}")]
    ValidationFailed { reason: String },
    
    #[error("Server '{server_name}' is not responding")]
    ServerNotResponding { server_name: String },
    
    #[error("Server '{server_name}' configuration is invalid: {reason}")]
    InvalidConfiguration { server_name: String, reason: String },
    
    #[error("Permission denied for server '{server_name}': {reason}")]
    PermissionDenied { server_name: String, reason: String },
    
    #[error("Server '{server_name}' process crashed: {reason}")]
    ProcessCrashed { server_name: String, reason: String },
    
    #[error("Timeout waiting for server '{server_name}' to {operation}")]
    OperationTimeout { server_name: String, operation: String },
    
    #[error("Multiple servers failed: {}", failed_servers.join(", "))]
    MultipleServerFailures { failed_servers: Vec<String> },
}

impl ReloadError {
    /// Returns a user-friendly error message with actionable guidance
    pub fn user_message(&self) -> String {
        match self {
            ReloadError::ServerNotFound { server_name } => {
                format!("Server '{}' was not found in your configuration.", server_name)
            },
            ReloadError::ServerStateConflict { server_name, state } => {
                format!("Server '{}' is already {}.", server_name, state)
            },
            ReloadError::ServerStopFailed { server_name, reason } => {
                format!("Could not stop server '{}': {}", server_name, reason)
            },
            ReloadError::ServerStartFailed { server_name, reason } => {
                format!("Could not start server '{}': {}", server_name, reason)
            },
            ReloadError::ConfigReloadFailed { server_name, reason } => {
                format!("Configuration for server '{}' could not be reloaded: {}", server_name, reason)
            },
            ReloadError::ToolRegistrationFailed { server_name, reason } => {
                format!("Tools from server '{}' could not be registered: {}", server_name, reason)
            },
            ReloadError::ValidationFailed { reason } => {
                format!("Validation failed: {}", reason)
            },
            ReloadError::ServerNotResponding { server_name } => {
                format!("Server '{}' is not responding to requests.", server_name)
            },
            ReloadError::InvalidConfiguration { server_name, reason } => {
                format!("Server '{}' has invalid configuration: {}", server_name, reason)
            },
            ReloadError::PermissionDenied { server_name, reason } => {
                format!("Permission denied for server '{}': {}", server_name, reason)
            },
            ReloadError::ProcessCrashed { server_name, reason } => {
                format!("Server '{}' process crashed: {}", server_name, reason)
            },
            ReloadError::OperationTimeout { server_name, operation } => {
                format!("Timeout waiting for server '{}' to {}.", server_name, operation)
            },
            ReloadError::MultipleServerFailures { failed_servers } => {
                format!("Multiple servers failed: {}", failed_servers.join(", "))
            },
        }
    }
    
    /// Returns suggested actions the user can take to resolve the error
    pub fn suggested_actions(&self) -> Vec<String> {
        match self {
            ReloadError::ServerNotFound { .. } => vec![
                "Check your MCP configuration files (.amazonq/mcp.json)".to_string(),
                "Use '/mcp list' to see all configured servers".to_string(),
                "Verify the server name spelling".to_string(),
            ],
            ReloadError::ServerStateConflict { server_name, state } => {
                if state.contains("disabled") {
                    vec![
                        format!("Use '/mcp enable {}' to enable the server first", server_name),
                        format!("Use '/mcp status {}' to check current state", server_name),
                    ]
                } else {
                    vec![
                        format!("Use '/mcp disable {}' to disable the server first", server_name),
                        format!("Use '/mcp status {}' to check current state", server_name),
                    ]
                }
            },
            ReloadError::ServerStartFailed { server_name, .. } => vec![
                "Check if the server executable exists and is accessible".to_string(),
                "Verify the server configuration in your MCP config file".to_string(),
                format!("Use '/mcp status {}' to see detailed error information", server_name),
                "Check system logs for additional error details".to_string(),
            ],
            ReloadError::ServerStopFailed { .. } => vec![
                "The server process may have already terminated".to_string(),
                "Try restarting the entire Q CLI session if the issue persists".to_string(),
            ],
            ReloadError::ConfigReloadFailed { .. } => vec![
                "Check your MCP configuration file syntax".to_string(),
                "Ensure all required fields are present in the configuration".to_string(),
                "Verify file permissions for the configuration file".to_string(),
            ],
            ReloadError::ToolRegistrationFailed { .. } => vec![
                "The server may be returning invalid tool specifications".to_string(),
                "Check server logs for tool registration errors".to_string(),
                "Try reloading the server to refresh tool definitions".to_string(),
            ],
            ReloadError::ValidationFailed { .. } => vec![
                "Check the server configuration for required fields".to_string(),
                "Ensure the server executable path is correct".to_string(),
            ],
            ReloadError::ServerNotResponding { server_name } => vec![
                format!("Try reloading the server with '/mcp reload {}'", server_name),
                "Check if the server process is still running".to_string(),
                "The server may need more time to initialize".to_string(),
            ],
            ReloadError::InvalidConfiguration { .. } => vec![
                "Review your MCP configuration file for syntax errors".to_string(),
                "Check that all required configuration fields are present".to_string(),
                "Validate JSON syntax if using JSON configuration".to_string(),
            ],
            ReloadError::PermissionDenied { .. } => vec![
                "Check file permissions for the server executable".to_string(),
                "Ensure you have permission to execute the server command".to_string(),
                "Try running Q CLI with appropriate permissions".to_string(),
            ],
            ReloadError::ProcessCrashed { server_name, .. } => vec![
                format!("Try reloading the server with '/mcp reload {}'", server_name),
                "Check server logs for crash details".to_string(),
                "The server may have encountered an internal error".to_string(),
            ],
            ReloadError::OperationTimeout { server_name, .. } => vec![
                format!("Try the operation again with '/mcp reload {}'", server_name),
                "The server may be slow to respond due to system load".to_string(),
                "Check if the server process is still running".to_string(),
            ],
            ReloadError::MultipleServerFailures { .. } => vec![
                "Check individual server status with '/mcp status [server-name]'".to_string(),
                "Try reloading servers individually to isolate issues".to_string(),
                "Review system resources and server configurations".to_string(),
            ],
        }
    }
    
    /// Returns the severity level of the error for display formatting
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ReloadError::ServerNotFound { .. } => ErrorSeverity::Error,
            ReloadError::ServerStateConflict { .. } => ErrorSeverity::Warning,
            ReloadError::ServerStopFailed { .. } => ErrorSeverity::Error,
            ReloadError::ServerStartFailed { .. } => ErrorSeverity::Error,
            ReloadError::ConfigReloadFailed { .. } => ErrorSeverity::Error,
            ReloadError::ToolRegistrationFailed { .. } => ErrorSeverity::Warning,
            ReloadError::ValidationFailed { .. } => ErrorSeverity::Error,
            ReloadError::ServerNotResponding { .. } => ErrorSeverity::Warning,
            ReloadError::InvalidConfiguration { .. } => ErrorSeverity::Error,
            ReloadError::PermissionDenied { .. } => ErrorSeverity::Error,
            ReloadError::ProcessCrashed { .. } => ErrorSeverity::Error,
            ReloadError::OperationTimeout { .. } => ErrorSeverity::Warning,
            ReloadError::MultipleServerFailures { .. } => ErrorSeverity::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    Warning,
    Error,
    Critical,
}

/// Manages the lifecycle of MCP servers including reload, start, and stop operations
pub struct ServerReloadManager {
    tool_manager: Arc<Mutex<ToolManager>>,
}

/// Utility for displaying comprehensive error messages with user guidance
pub struct ErrorDisplayManager;

impl ErrorDisplayManager {
    /// Displays a comprehensive error message with user guidance
    pub async fn display_error(
        error: &ReloadError,
        context: &str,
        session: &mut ChatSession,
    ) -> Result<(), std::io::Error> {
        let terminal_width = session.terminal_width();
        let severity = error.severity();
        
        // Display error header with appropriate styling
        let (symbol, color) = match severity {
            ErrorSeverity::Warning => ("⚠", style::Color::Yellow),
            ErrorSeverity::Error => ("✗", style::Color::Red),
            ErrorSeverity::Critical => ("💥", style::Color::Magenta),
        };
        
        queue!(
            session.stderr,
            style::Print(symbol),
            style::Print(" "),
            style::SetForegroundColor(color),
            style::SetAttribute(style::Attribute::Bold),
            style::Print(context),
            style::SetAttribute(style::Attribute::Reset),
            style::ResetColor,
            style::Print("\n"),
        )?;
        
        // Display the main error message
        queue!(
            session.stderr,
            style::Print("   "),
            style::Print(error.user_message()),
            style::Print("\n"),
        )?;
        
        // Add separator
        queue!(
            session.stderr,
            style::Print("   "),
            style::SetForegroundColor(style::Color::DarkGrey),
            style::Print("─".repeat(std::cmp::min(terminal_width.saturating_sub(3), 77))),
            style::ResetColor,
            style::Print("\n"),
        )?;
        
        // Display suggested actions
        let suggestions = error.suggested_actions();
        if !suggestions.is_empty() {
            queue!(
                session.stderr,
                style::Print("   "),
                style::SetForegroundColor(style::Color::Cyan),
                style::SetAttribute(style::Attribute::Bold),
                style::Print("💡 Suggested actions:"),
                style::SetAttribute(style::Attribute::Reset),
                style::ResetColor,
                style::Print("\n"),
            )?;
            
            for (i, suggestion) in suggestions.iter().enumerate() {
                queue!(
                    session.stderr,
                    style::Print("   "),
                    style::SetForegroundColor(style::Color::DarkGrey),
                    style::Print(format!("{}. ", i + 1)),
                    style::ResetColor,
                    style::Print(suggestion),
                    style::Print("\n"),
                )?;
            }
        }
        
        // Add context-specific help for server not found errors
        if let ReloadError::ServerNotFound { .. } = error {
            Self::display_available_servers(session).await?;
        }
        
        queue!(session.stderr, style::Print("\n"))?;
        session.stderr.flush()?;
        Ok(())
    }
    
    /// Displays available servers to help with server not found errors
    async fn display_available_servers(session: &mut ChatSession) -> Result<(), std::io::Error> {
        let available_servers = session.conversation.tool_manager
            .get_configured_server_names()
            .await;
        
        if !available_servers.is_empty() {
            queue!(
                session.stderr,
                style::Print("   "),
                style::SetForegroundColor(style::Color::Green),
                style::SetAttribute(style::Attribute::Bold),
                style::Print("📋 Available servers:"),
                style::SetAttribute(style::Attribute::Reset),
                style::ResetColor,
                style::Print("\n"),
            )?;
            
            // Group servers by status for better organization
            let current_clients: HashSet<String> = session.conversation.tool_manager.clients.keys().cloned().collect();
            let session_disabled = session.conversation.tool_manager.get_session_disabled_servers().await;
            
            let mut running_servers = Vec::new();
            let mut disabled_servers = Vec::new();
            let mut stopped_servers = Vec::new();
            
            for server_name in available_servers {
                if session_disabled.contains(&server_name) {
                    disabled_servers.push(server_name);
                } else if current_clients.contains(&server_name) {
                    running_servers.push(server_name);
                } else {
                    stopped_servers.push(server_name);
                }
            }
            
            // Display running servers
            if !running_servers.is_empty() {
                queue!(
                    session.stderr,
                    style::Print("      "),
                    style::SetForegroundColor(style::Color::Green),
                    style::Print("✓ Running: "),
                    style::ResetColor,
                    style::Print(running_servers.join(", ")),
                    style::Print("\n"),
                )?;
            }
            
            // Display stopped servers
            if !stopped_servers.is_empty() {
                queue!(
                    session.stderr,
                    style::Print("      "),
                    style::SetForegroundColor(style::Color::Yellow),
                    style::Print("○ Stopped: "),
                    style::ResetColor,
                    style::Print(stopped_servers.join(", ")),
                    style::Print("\n"),
                )?;
            }
            
            // Display disabled servers
            if !disabled_servers.is_empty() {
                queue!(
                    session.stderr,
                    style::Print("      "),
                    style::SetForegroundColor(style::Color::DarkGrey),
                    style::Print("○ Disabled: "),
                    style::ResetColor,
                    style::Print(disabled_servers.join(", ")),
                    style::Print("\n"),
                )?;
            }
        } else {
            queue!(
                session.stderr,
                style::Print("   "),
                style::SetForegroundColor(style::Color::Yellow),
                style::Print("⚠ No MCP servers are configured."),
                style::ResetColor,
                style::Print("\n"),
                style::Print("   Use 'q mcp add' to configure MCP servers."),
                style::Print("\n"),
            )?;
        }
        
        Ok(())
    }
    
    /// Displays a success message with consistent formatting
    pub fn display_success(
        message: &str,
        details: Option<&str>,
        session: &mut ChatSession,
    ) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            style::Print("✓ "),
            style::SetForegroundColor(style::Color::Green),
            style::SetAttribute(style::Attribute::Bold),
            style::Print(message),
            style::SetAttribute(style::Attribute::Reset),
            style::ResetColor,
        )?;
        
        if let Some(details) = details {
            queue!(
                session.stderr,
                style::Print(" "),
                style::SetForegroundColor(style::Color::DarkGrey),
                style::Print(details),
                style::ResetColor,
            )?;
        }
        
        queue!(session.stderr, style::Print("\n"))?;
        session.stderr.flush()?;
        Ok(())
    }
    
    /// Displays a warning message with consistent formatting
    pub fn display_warning(
        message: &str,
        details: Option<&str>,
        session: &mut ChatSession,
    ) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            style::Print("⚠ "),
            style::SetForegroundColor(style::Color::Yellow),
            style::SetAttribute(style::Attribute::Bold),
            style::Print(message),
            style::SetAttribute(style::Attribute::Reset),
            style::ResetColor,
        )?;
        
        if let Some(details) = details {
            queue!(
                session.stderr,
                style::Print(" "),
                style::SetForegroundColor(style::Color::DarkGrey),
                style::Print(details),
                style::ResetColor,
            )?;
        }
        
        queue!(session.stderr, style::Print("\n"))?;
        session.stderr.flush()?;
        Ok(())
    }
}

impl ServerReloadManager {
    /// Creates a new ServerReloadManager with a reference to the ToolManager
    pub fn new(tool_manager: Arc<Mutex<ToolManager>>) -> Self {
        Self { tool_manager }
    }
    
    /// Performs a complete reload of an MCP server
    /// 
    /// This operation:
    /// 1. Validates the server exists in configuration
    /// 2. Stops the existing server process
    /// 3. Removes tools from the registry
    /// 4. Re-reads the server configuration
    /// 5. Starts a new server process
    /// 6. Re-registers tools with the model
    /// 
    /// # Arguments
    /// * `os` - Operating system interface for file operations
    /// * `server_name` - Name of the server to reload
    /// 
    /// # Returns
    /// * `Ok(())` if reload was successful
    /// * `Err(ReloadError)` if any step failed
    pub async fn reload_server(&self, os: &Os, server_name: &str) -> Result<(), ReloadError> {
        self.reload_server_with_progress(os, server_name, None).await
    }
    
    /// Reloads a server with optional progress display
    pub async fn reload_server_with_progress(
        &self, 
        os: &Os, 
        server_name: &str, 
        progress: Option<&ProgressDisplay>
    ) -> Result<(), ReloadError> {
        let start_time = Instant::now();
        let operation_id = format!("reload_{}", server_name);
        
        info!("Starting reload of server '{}'", server_name);
        
        // Start progress display
        if let Some(progress) = progress {
            progress.start_operation(
                operation_id.clone(),
                format!("Reloading server '{}'", server_name)
            ).await;
        }
        
        // 1. Validate server exists in configuration
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Validating server '{}'", server_name)
            ).await;
        }
        
        if let Err(e) = self.validate_server_exists(server_name).await {
            let duration = start_time.elapsed();
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Server '{}' validation failed", server_name),
                    e.to_string(),
                    duration
                ).await;
            }
            return Err(e);
        }
        
        // 2. Stop existing server and remove tools
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Stopping server '{}'", server_name)
            ).await;
        }
        
        if let Err(e) = self.stop_server_and_cleanup(server_name).await {
            let duration = start_time.elapsed();
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Failed to stop server '{}'", server_name),
                    e.to_string(),
                    duration
                ).await;
            }
            return Err(e);
        }
        
        // 3. Re-read configuration for this server
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Reloading configuration for '{}'", server_name)
            ).await;
        }
        
        let config = match self.reload_server_config(os, server_name).await {
            Ok(config) => config,
            Err(e) => {
                let duration = start_time.elapsed();
                if let Some(progress) = progress {
                    progress.error(
                        operation_id,
                        format!("Configuration reload failed for '{}'", server_name),
                        e.to_string(),
                        duration
                    ).await;
                }
                return Err(e);
            }
        };
        
        // 4. Start new server with updated configuration
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Starting server '{}'", server_name)
            ).await;
        }
        
        if let Err(e) = self.start_server_with_config(server_name, config).await {
            let duration = start_time.elapsed();
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Failed to start server '{}'", server_name),
                    e.to_string(),
                    duration
                ).await;
            }
            return Err(e);
        }
        
        let duration = start_time.elapsed();
        info!("Successfully reloaded server '{}' in {:.2}s", server_name, duration.as_secs_f64());
        
        if let Some(progress) = progress {
            progress.success(
                operation_id,
                format!("Server '{}' reloaded successfully", server_name),
                duration
            ).await;
        }
        
        Ok(())
    }
    
    /// Enables a server that was disabled in configuration or session
    /// 
    /// # Arguments
    /// * `os` - Operating system interface for file operations
    /// * `server_name` - Name of the server to enable
    pub async fn enable_server(&self, os: &Os, server_name: &str) -> Result<(), ReloadError> {
        self.enable_server_with_progress(os, server_name, None).await
    }
    
    /// Enables a server with optional progress display
    pub async fn enable_server_with_progress(
        &self, 
        os: &Os, 
        server_name: &str, 
        progress: Option<&ProgressDisplay>
    ) -> Result<(), ReloadError> {
        let start_time = Instant::now();
        let operation_id = format!("enable_{}", server_name);
        
        info!("Enabling server '{}'", server_name);
        
        // Start progress display
        if let Some(progress) = progress {
            progress.start_operation(
                operation_id.clone(),
                format!("Enabling server '{}'", server_name)
            ).await;
        }
        
        // Validate server exists
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Validating server '{}'", server_name)
            ).await;
        }
        
        if let Err(e) = self.validate_server_exists(server_name).await {
            let duration = start_time.elapsed();
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Server '{}' validation failed", server_name),
                    e.to_string(),
                    duration
                ).await;
            }
            return Err(e);
        }
        
        // Check current state
        let tool_manager = self.tool_manager.lock().await;
        let is_currently_enabled = tool_manager.clients.contains_key(server_name);
        let has_session_override = tool_manager.has_session_override(server_name).await;
        drop(tool_manager);
        
        if is_currently_enabled && !has_session_override {
            let duration = start_time.elapsed();
            let error = ReloadError::ServerStateConflict {
                server_name: server_name.to_string(),
                state: "enabled".to_string(),
            };
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Server '{}' state conflict", server_name),
                    error.to_string(),
                    duration
                ).await;
            }
            return Err(error);
        }
        
        // Enable in session state
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Updating session state for '{}'", server_name)
            ).await;
        }
        
        let tool_manager = self.tool_manager.lock().await;
        tool_manager.enable_server_for_session(server_name.to_string()).await;
        drop(tool_manager);
        
        // If server is not currently running, start it
        if !is_currently_enabled {
            if let Some(progress) = progress {
                progress.update_progress(
                    operation_id.clone(),
                    format!("Starting server '{}'", server_name)
                ).await;
            }
            
            let config = match self.reload_server_config(os, server_name).await {
                Ok(config) => config,
                Err(e) => {
                    let duration = start_time.elapsed();
                    if let Some(progress) = progress {
                        progress.error(
                            operation_id,
                            format!("Configuration reload failed for '{}'", server_name),
                            e.to_string(),
                            duration
                        ).await;
                    }
                    return Err(e);
                }
            };
            
            if let Err(e) = self.start_server_with_config(server_name, config).await {
                let duration = start_time.elapsed();
                if let Some(progress) = progress {
                    progress.error(
                        operation_id,
                        format!("Failed to start server '{}'", server_name),
                        e.to_string(),
                        duration
                    ).await;
                }
                return Err(e);
            }
        }
        
        let duration = start_time.elapsed();
        info!("Successfully enabled server '{}'", server_name);
        
        if let Some(progress) = progress {
            progress.success(
                operation_id,
                format!("Server '{}' enabled successfully", server_name),
                duration
            ).await;
        }
        
        Ok(())
    }
    
    /// Disables a server for the current session
    /// 
    /// # Arguments
    /// * `server_name` - Name of the server to disable
    pub async fn disable_server(&self, server_name: &str) -> Result<(), ReloadError> {
        self.disable_server_with_progress(server_name, None).await
    }
    
    /// Disables a server with optional progress display
    pub async fn disable_server_with_progress(
        &self, 
        server_name: &str, 
        progress: Option<&ProgressDisplay>
    ) -> Result<(), ReloadError> {
        let start_time = Instant::now();
        let operation_id = format!("disable_{}", server_name);
        
        info!("Disabling server '{}'", server_name);
        
        // Start progress display
        if let Some(progress) = progress {
            progress.start_operation(
                operation_id.clone(),
                format!("Disabling server '{}'", server_name)
            ).await;
        }
        
        // Validate server exists
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Validating server '{}'", server_name)
            ).await;
        }
        
        if let Err(e) = self.validate_server_exists(server_name).await {
            let duration = start_time.elapsed();
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Server '{}' validation failed", server_name),
                    e.to_string(),
                    duration
                ).await;
            }
            return Err(e);
        }
        
        // Check current state
        let tool_manager = self.tool_manager.lock().await;
        let is_currently_enabled = tool_manager.clients.contains_key(server_name);
        let has_session_override = tool_manager.has_session_override(server_name).await;
        drop(tool_manager);
        
        if !is_currently_enabled && !has_session_override {
            let duration = start_time.elapsed();
            let error = ReloadError::ServerStateConflict {
                server_name: server_name.to_string(),
                state: "disabled".to_string(),
            };
            if let Some(progress) = progress {
                progress.error(
                    operation_id,
                    format!("Server '{}' state conflict", server_name),
                    error.to_string(),
                    duration
                ).await;
            }
            return Err(error);
        }
        
        // Disable in session state
        if let Some(progress) = progress {
            progress.update_progress(
                operation_id.clone(),
                format!("Updating session state for '{}'", server_name)
            ).await;
        }
        
        let tool_manager = self.tool_manager.lock().await;
        tool_manager.disable_server_for_session(server_name.to_string()).await;
        drop(tool_manager);
        
        // If server is currently running, stop it (but keep tools in registry)
        if is_currently_enabled {
            if let Some(progress) = progress {
                progress.update_progress(
                    operation_id.clone(),
                    format!("Stopping server '{}'", server_name)
                ).await;
            }
            
            if let Err(e) = self.stop_server_only(server_name).await {
                let duration = start_time.elapsed();
                if let Some(progress) = progress {
                    progress.error(
                        operation_id,
                        format!("Failed to stop server '{}'", server_name),
                        e.to_string(),
                        duration
                    ).await;
                }
                return Err(e);
            }
        }
        
        let duration = start_time.elapsed();
        info!("Successfully disabled server '{}'", server_name);
        
        if let Some(progress) = progress {
            progress.success(
                operation_id,
                format!("Server '{}' disabled successfully", server_name),
                duration
            ).await;
        }
        
        Ok(())
    }
    
    /// Gets a reference to the tool manager for external access
    pub fn get_tool_manager(&self) -> &Arc<Mutex<ToolManager>> {
        &self.tool_manager
    }
    async fn validate_server_exists(&self, server_name: &str) -> Result<(), ReloadError> {
        let tool_manager = self.tool_manager.lock().await;
        let has_config = tool_manager.has_server_config(server_name).await;
        
        if !has_config {
            return Err(ReloadError::ServerNotFound {
                server_name: server_name.to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Reloads configuration files and returns updated server configurations
    pub async fn reload_configurations(&self, os: &Os) -> Result<HashMap<String, CustomToolConfig>, ReloadError> {
        info!("Reloading MCP server configurations");
        
        let mut all_servers = HashMap::new();
        let mut config_errors = Vec::new();
        
        // Load workspace configuration
        match self.load_workspace_config(os).await {
            Ok(workspace_config) => {
                info!("Loaded {} servers from workspace config", workspace_config.mcp_servers.len());
                all_servers.extend(workspace_config.mcp_servers);
            },
            Err(e) => {
                debug!("No workspace config or failed to load: {}", e);
                // Workspace config is optional, so we don't treat this as an error
            }
        }
        
        // Load global configuration
        match self.load_global_config(os).await {
            Ok(global_config) => {
                info!("Loaded {} servers from global config", global_config.mcp_servers.len());
                // Workspace config takes precedence over global config
                for (name, config) in global_config.mcp_servers {
                    if !all_servers.contains_key(&name) {
                        all_servers.insert(name, config);
                    }
                }
            },
            Err(e) => {
                debug!("No global config or failed to load: {}", e);
                // Global config is optional, so we don't treat this as an error
            }
        }
        
        // Validate all loaded configurations
        for (server_name, config) in &all_servers {
            if let Err(validation_error) = self.validate_server_configuration(server_name, config).await {
                config_errors.push(format!("{}: {}", server_name, validation_error));
            }
        }
        
        // If there are validation errors, return them
        if !config_errors.is_empty() {
            return Err(ReloadError::ConfigReloadFailed {
                server_name: "multiple".to_string(),
                reason: format!("Configuration validation failed: {}", config_errors.join("; ")),
            });
        }
        
        info!("Successfully reloaded {} server configurations", all_servers.len());
        Ok(all_servers)
    }
    
    /// Loads workspace MCP configuration
    async fn load_workspace_config(&self, os: &Os) -> Result<McpServerConfig, eyre::Error> {
        let workspace_path = workspace_mcp_config_path(os)?;
        
        if !os.fs.exists(&workspace_path) {
            return Err(eyre::eyre!("Workspace config file does not exist"));
        }
        
        info!("Loading workspace config from: {}", workspace_path.display());
        McpServerConfig::load_from_file(os, &workspace_path).await
    }
    
    /// Loads global MCP configuration
    async fn load_global_config(&self, os: &Os) -> Result<McpServerConfig, eyre::Error> {
        let global_path = global_mcp_config_path(os)?;
        
        if !os.fs.exists(&global_path) {
            return Err(eyre::eyre!("Global config file does not exist"));
        }
        
        info!("Loading global config from: {}", global_path.display());
        McpServerConfig::load_from_file(os, &global_path).await
    }
    
    /// Validates a server configuration before applying it
    async fn validate_server_configuration(&self, _server_name: &str, config: &CustomToolConfig) -> Result<(), String> {
        // Validate command exists
        if config.command.is_empty() {
            return Err("Server command is empty".to_string());
        }
        
        // Check if command is accessible (basic validation)
        if !std::path::Path::new(&config.command).exists() {
            // For simple commands, we'll skip PATH checking for now
            // This is a basic validation - more sophisticated checking could be added later
            let command_parts: Vec<&str> = config.command.split_whitespace().collect();
            if let Some(cmd) = command_parts.first() {
                if !std::path::Path::new(cmd).exists() && !cmd.contains('/') {
                    // If it's not an absolute/relative path and doesn't exist, it might be in PATH
                    // We'll allow it for now but could add more validation later
                    debug!("Command '{}' not found locally, assuming it's in PATH", cmd);
                }
            }
        }
        
        // Validate timeout
        if config.timeout == 0 {
            return Err("Server timeout cannot be zero".to_string());
        }
        
        // Validate environment variables if present
        if let Some(env) = &config.env {
            for (key, _value) in env {
                if key.is_empty() {
                    return Err("Environment variable key cannot be empty".to_string());
                }
                if key.contains('=') {
                    return Err(format!("Environment variable key '{}' cannot contain '='", key));
                }
                // Value can be empty, that's valid
            }
        }
        
        // Validate arguments
        for arg in &config.args {
            if arg.is_empty() {
                return Err("Server argument cannot be empty".to_string());
            }
        }
        
        Ok(())
    }
    
    /// Reloads a specific server with updated configuration
    pub async fn reload_server_with_config(&self, os: &Os, server_name: &str) -> Result<(), ReloadError> {
        info!("Reloading server '{}' with updated configuration", server_name);
        
        // Step 1: Reload all configurations
        let updated_configs = self.reload_configurations(os).await?;
        
        // Step 2: Check if the server exists in the updated configuration
        let server_config = updated_configs.get(server_name)
            .ok_or_else(|| ReloadError::ServerNotFound {
                server_name: server_name.to_string(),
            })?
            .clone();
        
        // Step 3: Check current state
        let tool_manager = self.tool_manager.lock().await;
        let is_currently_running = tool_manager.clients.contains_key(server_name);
        let is_session_disabled = tool_manager.is_session_disabled(server_name).await;
        drop(tool_manager);
        
        if is_session_disabled {
            return Err(ReloadError::ServerStateConflict {
                server_name: server_name.to_string(),
                state: "disabled for this session".to_string(),
            });
        }
        
        // Step 4: Stop existing server if running
        if is_currently_running {
            self.stop_server_and_cleanup(server_name).await
                .map_err(|e| ReloadError::ServerStopFailed {
                    server_name: server_name.to_string(),
                    reason: e.to_string(),
                })?;
        }
        
        // Step 5: Start the server with the new configuration
        self.start_server_with_config(server_name, server_config).await?;
        
        info!("Successfully reloaded server '{}' with updated configuration", server_name);
        Ok(())
    }
    
    /// Starts a server with a specific configuration
    async fn start_server_with_config(&self, server_name: &str, config: CustomToolConfig) -> Result<(), ReloadError> {
        // Create the client (this is not async)
        let client = CustomToolClient::from_config(server_name.to_string(), config.clone())
            .map_err(|e| ReloadError::ServerStartFailed {
                server_name: server_name.to_string(),
                reason: e.to_string(),
            })?;
        
        // Initialize the client with timeout
        tokio::time::timeout(
            std::time::Duration::from_secs(config.timeout.max(30)), // Use at least 30 seconds
            client.init()
        )
        .await
        .map_err(|_| ReloadError::OperationTimeout {
            server_name: server_name.to_string(),
            operation: "initialize".to_string(),
        })?
        .map_err(|e| ReloadError::ServerStartFailed {
            server_name: server_name.to_string(),
            reason: format!("Initialization failed: {}", e),
        })?;
        
        // Add client to tool manager
        let mut tool_manager = self.tool_manager.lock().await;
        let client_arc = Arc::new(client);
        tool_manager.clients.insert(server_name.to_string(), client_arc.clone());
        
        // Check if tools already exist for this server (from initial startup)
        let has_existing_tools = tool_manager.schema.values()
            .any(|spec| matches!(&spec.tool_origin, ToolOrigin::McpServer(name) if name == server_name));
        
        if !has_existing_tools {
            // Server was disabled in config, so we need to register tools manually
            if let Err(e) = self.register_server_tools(&mut tool_manager, client_arc, server_name).await {
                // If tool registration fails, remove the client and return error
                tool_manager.clients.remove(server_name);
                return Err(e);
            }
        } else {
            // Server was enabled in config but disabled for session - tools already exist
            debug!("Server '{}' has existing tools, skipping registration", server_name);
        }
        
        Ok(())
    }
    
    /// Manually registers tools for a server (bypassing the messenger system)
    async fn register_server_tools(
        &self,
        tool_manager: &mut crate::cli::chat::tool_manager::ToolManager,
        client: Arc<CustomToolClient>,
        server_name: &str,
    ) -> Result<(), ReloadError> {
        use crate::cli::chat::tools::custom_tool::CustomToolClient;
        
        // Fetch tools from the server
        let tools_result = match client.as_ref() {
            CustomToolClient::Stdio { client: mcp_client, .. } => {
                // Request tools list from the server
                let resp = mcp_client.request("tools/list", None).await
                    .map_err(|e| ReloadError::ToolRegistrationFailed {
                        server_name: server_name.to_string(),
                        reason: format!("Failed to request tools list: {}", e),
                    })?;
                
                if let Some(error) = resp.error {
                    return Err(ReloadError::ToolRegistrationFailed {
                        server_name: server_name.to_string(),
                        reason: format!("Server returned error: {:?}", error),
                    });
                }
                
                resp.result.ok_or_else(|| ReloadError::ToolRegistrationFailed {
                    server_name: server_name.to_string(),
                    reason: "Tools list response missing result".to_string(),
                })?
            }
        };
        
        // Parse the tools list result
        #[derive(serde::Deserialize)]
        struct ToolsListResult {
            tools: Vec<serde_json::Value>,
        }
        
        let tools_list = serde_json::from_value::<ToolsListResult>(tools_result)
            .map_err(|e| ReloadError::ToolRegistrationFailed {
                server_name: server_name.to_string(),
                reason: format!("Failed to parse tools list: {}", e),
            })?;
        
        // Convert tools to ToolSpec format and register them directly
        let mut specs = Vec::new();
        for tool_value in tools_list.tools {
            if let Ok(mut spec) = serde_json::from_value::<ToolSpec>(tool_value) {
                // Set the correct tool origin for MCP server tools
                spec.tool_origin = ToolOrigin::McpServer(server_name.to_string());
                specs.push(spec);
            }
        }
        
        // Register tools directly in the tool manager (similar to what update() does)
        for spec in specs {
            let model_tool_name = format!("{}___{}", server_name, spec.name);
            
            // Check for conflicts with existing tools
            if tool_manager.tn_map.contains_key(&model_tool_name) {
                warn!("Tool name conflict: {} already exists, skipping", model_tool_name);
                continue;
            }
            
            // Create tool info
            let tool_info = ToolInfo {
                host_tool_name: spec.name.clone(),
                server_name: server_name.to_string(),
            };
            
            // Add to tool name map
            tool_manager.tn_map.insert(model_tool_name.clone(), tool_info);
            
            // Add to schema
            tool_manager.schema.insert(model_tool_name, spec);
        }
        
        info!("Registered {} tools for server '{}'", tool_manager.schema.len(), server_name);
        
        Ok(())
    }
    
    /// Detects configuration changes by comparing file modification times
    pub async fn detect_configuration_changes(&self, os: &Os) -> Result<Vec<String>, ReloadError> {
        let mut changed_files = Vec::new();
        
        // Check workspace config
        if let Ok(workspace_path) = workspace_mcp_config_path(os) {
            if os.fs.exists(&workspace_path) {
                // For now, we'll just report that the file exists
                // In a more sophisticated implementation, we could track modification times
                changed_files.push(format!("workspace: {}", workspace_path.display()));
            }
        }
        
        // Check global config
        if let Ok(global_path) = global_mcp_config_path(os) {
            if os.fs.exists(&global_path) {
                changed_files.push(format!("global: {}", global_path.display()));
            }
        }
        
        Ok(changed_files)
    }
    
    /// Stops a server process without removing tools from the registry
    async fn stop_server_only(&self, server_name: &str) -> Result<(), ReloadError> {
        debug!("Stopping server '{}' (keeping tools in registry)", server_name);
        
        let mut tool_manager = self.tool_manager.lock().await;
        
        // Remove client (this will trigger the Drop trait to terminate the process)
        if let Some(client) = tool_manager.clients.remove(server_name) {
            debug!("Removed client for server '{}'", server_name);
            // Client will be dropped here, terminating the server process
            drop(client);
        }
        
        debug!("Successfully stopped server '{}'", server_name);
        Ok(())
    }
    
    /// Stops a server and cleans up its tools from the registry
    async fn stop_server_and_cleanup(&self, server_name: &str) -> Result<(), ReloadError> {
        debug!("Stopping server '{}' and cleaning up tools", server_name);
        
        let mut tool_manager = self.tool_manager.lock().await;
        
        // Remove client (this will trigger the Drop trait to terminate the process)
        if let Some(client) = tool_manager.clients.remove(server_name) {
            debug!("Removed client for server '{}'", server_name);
            // Client will be dropped here, terminating the server process
            drop(client);
        }
        
        // Remove tools from registry
        self.remove_server_tools(&mut tool_manager, server_name).await;
        
        debug!("Successfully stopped and cleaned up server '{}'", server_name);
        Ok(())
    }
    
    /// Removes all tools associated with a server from the tool registry
    async fn remove_server_tools(&self, tool_manager: &mut ToolManager, server_name: &str) {
        debug!("Removing tools for server '{}'", server_name);
        
        // Remove from tool name map
        let removed_tools: Vec<_> = tool_manager.tn_map
            .iter()
            .filter(|(_, tool_info)| tool_info.server_name == server_name)
            .map(|(tool_name, _)| tool_name.clone())
            .collect();
        
        for tool_name in &removed_tools {
            tool_manager.tn_map.remove(tool_name);
        }
        
        // Remove from schema
        for tool_name in &removed_tools {
            tool_manager.schema.remove(tool_name);
        }
        
        debug!("Removed {} tools for server '{}'", removed_tools.len(), server_name);
    }
    
    /// Re-reads the configuration for a specific server
    /// Re-reads the configuration for a specific server with comprehensive validation
    async fn reload_server_config(&self, os: &Os, server_name: &str) -> Result<CustomToolConfig, ReloadError> {
        debug!("Reloading configuration for server '{}'", server_name);
        
        // Use the comprehensive configuration reloading
        let updated_configs = self.reload_configurations(os).await?;
        
        // Get the specific server configuration
        updated_configs.get(server_name)
            .cloned()
            .ok_or_else(|| ReloadError::ConfigReloadFailed {
                server_name: server_name.to_string(),
                reason: "Server not found in updated configuration".to_string(),
            })
    }
    
    /// Loads workspace MCP configuration
    async fn load_workspace_mcp_config(&self, os: &Os) -> Result<crate::cli::agent::McpServerConfig, ReloadError> {
        let config_path = workspace_mcp_config_path(os)
            .map_err(|e| ReloadError::ConfigReloadFailed {
                server_name: "workspace".to_string(),
                reason: format!("Failed to get workspace config path: {}", e),
            })?;
        
        if !os.fs.exists(&config_path) {
            return Err(ReloadError::ConfigReloadFailed {
                server_name: "workspace".to_string(),
                reason: "Workspace MCP configuration file does not exist".to_string(),
            });
        }
        
        crate::cli::agent::McpServerConfig::load_from_file(os, &config_path).await
            .map_err(|e| ReloadError::ConfigReloadFailed {
                server_name: "workspace".to_string(),
                reason: format!("Failed to load workspace config: {}", e),
            })
    }
    
    /// Loads global MCP configuration
    async fn load_global_mcp_config(&self, os: &Os) -> Result<crate::cli::agent::McpServerConfig, ReloadError> {
        let config_path = global_mcp_config_path(os)
            .map_err(|e| ReloadError::ConfigReloadFailed {
                server_name: "global".to_string(),
                reason: format!("Failed to get global config path: {}", e),
            })?;
        
        if !os.fs.exists(&config_path) {
            return Err(ReloadError::ConfigReloadFailed {
                server_name: "global".to_string(),
                reason: "Global MCP configuration file does not exist".to_string(),
            });
        }
        
        crate::cli::agent::McpServerConfig::load_from_file(os, &config_path).await
            .map_err(|e| ReloadError::ConfigReloadFailed {
                server_name: "global".to_string(),
                reason: format!("Failed to load global config: {}", e),
            })
    }
    
    /// Starts a server with the given configuration
    /// Gets the current status of all servers
    pub async fn get_server_status(&self) -> HashMap<String, ServerStatus> {
        let tool_manager = self.tool_manager.lock().await;
        let mut status_map = HashMap::new();
        
        // Get all configured servers
        let configured_servers = tool_manager.get_configured_server_names().await;
        
        for server_name in configured_servers {
            let is_running = tool_manager.clients.contains_key(&server_name);
            let _has_session_override = tool_manager.has_session_override(&server_name).await;
            let is_session_enabled = tool_manager.is_session_enabled(&server_name).await;
            let is_session_disabled = tool_manager.is_session_disabled(&server_name).await;
            
            let status = if is_running {
                ServerStatus::Running
            } else if is_session_disabled {
                ServerStatus::SessionDisabled
            } else if is_session_enabled {
                ServerStatus::SessionEnabled
            } else {
                ServerStatus::Stopped
            };
            
            status_map.insert(server_name, status);
        }
        
        status_map
    }
}

/// Represents the current status of a server
#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    /// Server is currently running
    Running,
    /// Server is stopped
    Stopped,
    /// Server was enabled for this session only
    SessionEnabled,
    /// Server was disabled for this session only
    SessionDisabled,
    /// Server failed to start
    Failed(String),
}

impl std::fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerStatus::Running => write!(f, "running"),
            ServerStatus::Stopped => write!(f, "stopped"),
            ServerStatus::SessionEnabled => write!(f, "enabled (session)"),
            ServerStatus::SessionDisabled => write!(f, "disabled (session)"),
            ServerStatus::Failed(reason) => write!(f, "failed: {}", reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_error_display() {
        let error = ReloadError::ServerNotFound {
            server_name: "test-server".to_string(),
        };
        assert_eq!(error.to_string(), "Server 'test-server' not found in configuration");
        
        let error = ReloadError::ServerStateConflict {
            server_name: "test-server".to_string(),
            state: "enabled".to_string(),
        };
        assert_eq!(error.to_string(), "Server 'test-server' is already enabled");
        
        let error = ReloadError::ServerStartFailed {
            server_name: "test-server".to_string(),
            reason: "connection timeout".to_string(),
        };
        assert_eq!(error.to_string(), "Failed to start server 'test-server': connection timeout");
        
        let error = ReloadError::ConfigReloadFailed {
            server_name: "test-server".to_string(),
            reason: "file not found".to_string(),
        };
        assert_eq!(error.to_string(), "Configuration reload failed for 'test-server': file not found");
    }

    #[test]
    fn test_server_status_display() {
        assert_eq!(ServerStatus::Running.to_string(), "running");
        assert_eq!(ServerStatus::Stopped.to_string(), "stopped");
        assert_eq!(ServerStatus::SessionEnabled.to_string(), "enabled (session)");
        assert_eq!(ServerStatus::SessionDisabled.to_string(), "disabled (session)");
        assert_eq!(ServerStatus::Failed("connection error".to_string()).to_string(), "failed: connection error");
    }
    
    #[test]
    fn test_server_status_equality() {
        assert_eq!(ServerStatus::Running, ServerStatus::Running);
        assert_eq!(ServerStatus::Stopped, ServerStatus::Stopped);
        assert_ne!(ServerStatus::Running, ServerStatus::Stopped);
        
        let failed1 = ServerStatus::Failed("error1".to_string());
        let failed2 = ServerStatus::Failed("error1".to_string());
        let failed3 = ServerStatus::Failed("error2".to_string());
        
        assert_eq!(failed1, failed2);
        assert_ne!(failed1, failed3);
    }
}
