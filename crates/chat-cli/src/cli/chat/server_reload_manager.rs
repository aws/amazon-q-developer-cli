use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use eyre::Result;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::cli::chat::tools::custom_tool::{CustomToolClient, CustomToolConfig};
use crate::cli::chat::tool_manager::{ToolManager, global_mcp_config_path, workspace_mcp_config_path};
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
}

/// Manages the lifecycle of MCP servers including reload, start, and stop operations
pub struct ServerReloadManager {
    tool_manager: Arc<Mutex<ToolManager>>,
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
        let start_time = Instant::now();
        info!("Starting reload of server '{}'", server_name);
        
        // 1. Validate server exists in configuration
        self.validate_server_exists(server_name).await?;
        
        // 2. Stop existing server and remove tools
        self.stop_server_and_cleanup(server_name).await?;
        
        // 3. Re-read configuration for this server
        let config = self.reload_server_config(os, server_name).await?;
        
        // 4. Start new server with updated configuration
        self.start_server_with_config(server_name, config).await?;
        
        let duration = start_time.elapsed();
        info!("Successfully reloaded server '{}' in {:.2}s", server_name, duration.as_secs_f64());
        
        Ok(())
    }
    
    /// Enables a server that was disabled in configuration or session
    /// 
    /// # Arguments
    /// * `os` - Operating system interface for file operations
    /// * `server_name` - Name of the server to enable
    pub async fn enable_server(&self, os: &Os, server_name: &str) -> Result<(), ReloadError> {
        info!("Enabling server '{}'", server_name);
        
        // Validate server exists
        self.validate_server_exists(server_name).await?;
        
        // Check current state
        let tool_manager = self.tool_manager.lock().await;
        let is_currently_enabled = tool_manager.clients.contains_key(server_name);
        let has_session_override = tool_manager.has_session_override(server_name).await;
        drop(tool_manager);
        
        if is_currently_enabled && !has_session_override {
            return Err(ReloadError::ServerStateConflict {
                server_name: server_name.to_string(),
                state: "enabled".to_string(),
            });
        }
        
        // Enable in session state
        let tool_manager = self.tool_manager.lock().await;
        tool_manager.enable_server_for_session(server_name.to_string()).await;
        drop(tool_manager);
        
        // If server is not currently running, start it
        if !is_currently_enabled {
            let config = self.reload_server_config(os, server_name).await?;
            self.start_server_with_config(server_name, config).await?;
        }
        
        info!("Successfully enabled server '{}'", server_name);
        Ok(())
    }
    
    /// Disables a server for the current session
    /// 
    /// # Arguments
    /// * `server_name` - Name of the server to disable
    pub async fn disable_server(&self, server_name: &str) -> Result<(), ReloadError> {
        info!("Disabling server '{}'", server_name);
        
        // Validate server exists
        self.validate_server_exists(server_name).await?;
        
        // Check current state
        let tool_manager = self.tool_manager.lock().await;
        let is_currently_enabled = tool_manager.clients.contains_key(server_name);
        let has_session_override = tool_manager.has_session_override(server_name).await;
        drop(tool_manager);
        
        if !is_currently_enabled && !has_session_override {
            return Err(ReloadError::ServerStateConflict {
                server_name: server_name.to_string(),
                state: "disabled".to_string(),
            });
        }
        
        // Disable in session state
        let tool_manager = self.tool_manager.lock().await;
        tool_manager.disable_server_for_session(server_name.to_string()).await;
        drop(tool_manager);
        
        // If server is currently running, stop it (but keep tools in registry)
        if is_currently_enabled {
            self.stop_server_only(server_name).await?;
        }
        
        info!("Successfully disabled server '{}'", server_name);
        Ok(())
    }
    
    /// Validates that a server exists in the configuration
    async fn validate_server_exists(&self, server_name: &str) -> Result<(), ReloadError> {
        let tool_manager = self.tool_manager.lock().await;
        let has_config = tool_manager.has_server_config(server_name).await;
        
        if !has_config {
            let available_servers = tool_manager.get_configured_server_names().await;
            return Err(ReloadError::ServerNotFound {
                server_name: server_name.to_string(),
            });
        }
        
        Ok(())
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
    async fn reload_server_config(&self, os: &Os, server_name: &str) -> Result<CustomToolConfig, ReloadError> {
        debug!("Reloading configuration for server '{}'", server_name);
        
        // Re-read both workspace and global MCP configurations
        let workspace_config = self.load_workspace_mcp_config(os).await;
        let global_config = self.load_global_mcp_config(os).await;
        
        // Try workspace config first, then global config
        let config = if let Ok(workspace_config) = workspace_config {
            workspace_config.mcp_servers.get(server_name).cloned()
        } else {
            None
        }.or_else(|| {
            if let Ok(global_config) = global_config {
                global_config.mcp_servers.get(server_name).cloned()
            } else {
                None
            }
        });
        
        config.ok_or_else(|| ReloadError::ConfigReloadFailed {
            server_name: server_name.to_string(),
            reason: "Server not found in workspace or global MCP configuration".to_string(),
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
    async fn start_server_with_config(&self, server_name: &str, config: CustomToolConfig) -> Result<(), ReloadError> {
        debug!("Starting server '{}' with new configuration", server_name);
        
        // Create new client
        let new_client = CustomToolClient::from_config(server_name.to_string(), config)
            .map_err(|e| ReloadError::ServerStartFailed {
                server_name: server_name.to_string(),
                reason: e.to_string(),
            })?;
        
        // Initialize the client (this establishes the connection)
        new_client.init().await
            .map_err(|e| ReloadError::ServerStartFailed {
                server_name: server_name.to_string(),
                reason: e.to_string(),
            })?;
        
        // Add client to tool manager
        let mut tool_manager = self.tool_manager.lock().await;
        tool_manager.clients.insert(server_name.to_string(), Arc::new(new_client));
        
        debug!("Successfully started server '{}'", server_name);
        
        // Note: Tool registration will happen automatically via the existing
        // async mechanism in ToolManager that listens for new tools
        
        Ok(())
    }
    
    /// Gets the current status of all servers
    pub async fn get_server_status(&self) -> HashMap<String, ServerStatus> {
        let tool_manager = self.tool_manager.lock().await;
        let mut status_map = HashMap::new();
        
        // Get all configured servers
        let configured_servers = tool_manager.get_configured_server_names().await;
        
        for server_name in configured_servers {
            let is_running = tool_manager.clients.contains_key(&server_name);
            let has_session_override = tool_manager.has_session_override(&server_name).await;
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
