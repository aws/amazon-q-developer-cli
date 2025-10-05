use async_trait::async_trait;
use eyre::Result;

use crate::cli::chat::tools::QueuedTool;

pub mod console;

/// Context information for permission requests
#[derive(Debug)]
pub struct PermissionContext {
    pub trust_all_tools: bool,
}

/// Result of a permission request
#[derive(Debug, Clone)]
pub enum PermissionDecision {
    Approved,
    Rejected,
    Cancelled,
}

/// Abstraction for handling tool permission requests
#[async_trait]
pub trait PermissionInterface {
    /// Request permission for a tool that requires confirmation
    async fn request_permission(
        &mut self,
        tool: &QueuedTool,
        context: &PermissionContext,
    ) -> Result<PermissionDecision>;
    
    /// Show a denied tool with explanation
    async fn show_denied_tool(
        &mut self,
        tool_name: &str,
        rules: Vec<String>,
    ) -> Result<()>;
    
    /// Show tool execution status (for notifications, etc.)
    async fn show_tool_execution(
        &mut self,
        tool: &QueuedTool,
        allowed: bool,
    ) -> Result<()>;
}
