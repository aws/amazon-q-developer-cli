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
    /// True while a forced (re-)authentication is in progress for this server.
    ///
    /// During forced auth the original server keeps running (a hidden "shadow"
    /// server runs the OAuth flow), so `status` stays `Running`/`Loading` and
    /// `tool_count` reflects the still-available tools. The UI uses this flag to
    /// denote that an auth attempt is happening on the server without listing the
    /// shadow as a separate entry.
    #[serde(default)]
    pub authenticating: bool,
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
