//! MCP server info types for the /mcp command

use serde::{
    Deserialize,
    Serialize,
};

/// Information about a configured MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub name: String,
    pub status: McpServerStatus,
    pub tool_count: usize,
}

/// Status of an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpServerStatus {
    Running,
    Loading,
    Failed,
    Disabled,
}
