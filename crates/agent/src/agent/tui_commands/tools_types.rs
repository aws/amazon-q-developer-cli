//! Tool info types for the /tools command

use serde::{
    Deserialize,
    Serialize,
};

/// Permission status of a tool in the current session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolStatus {
    /// Tool is auto-allowed (config or session trust)
    Allowed,
    /// Tool requires approval before each use
    RequiresApproval,
    /// Tool has been denied for this session
    Denied,
}

/// Information about an available tool
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub source: String,
    pub description: String,
    pub status: ToolStatus,
}
