use serde::{
    Deserialize,
    Serialize,
};

use super::Tool;
use crate::agent::agent_config::parse::CanonicalToolName;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallIdentity {
    pub tool_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_server_name: Option<String>,
}

impl ToolCallIdentity {
    pub fn from_tool(tool: &Tool) -> Self {
        Self::from_canonical_tool_name(&tool.canonical_tool_name())
    }

    pub fn from_canonical_tool_name(canonical_tool_name: &CanonicalToolName) -> Self {
        match canonical_tool_name {
            CanonicalToolName::BuiltIn(name) => Self {
                tool_name: name.as_ref().to_string(),
                mcp_server_name: None,
            },
            CanonicalToolName::Mcp { server_name, tool_name } => Self {
                tool_name: tool_name.clone(),
                mcp_server_name: Some(server_name.clone()),
            },
            CanonicalToolName::Agent { agent_name } => Self {
                tool_name: agent_name.clone(),
                mcp_server_name: None,
            },
        }
    }
}
