use serde::{
    Deserialize,
    Serialize,
};

use crate::agent::agent_config::parse::CanonicalToolName;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mcp {
    pub tool_name: String,
    pub server_name: String,
    /// Optional parameters to pass to the tool when invoking the method.
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Mcp {
    pub fn canonical_tool_name(&self) -> CanonicalToolName {
        CanonicalToolName::Mcp {
            server_name: self.server_name.clone(),
            tool_name: self.tool_name.clone(),
        }
    }
}
