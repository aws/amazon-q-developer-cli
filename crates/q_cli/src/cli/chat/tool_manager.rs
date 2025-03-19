use std::collections::HashMap;
use std::sync::Arc;

use mcp_client::{
    Client as McpClient,
    StdioTransport,
};

use super::tools::Tool;
use super::tools::custom_tool::CustomTool;

pub struct ToolManager {
    clients: HashMap<String, Arc<CustomTool>>,
}

impl ToolManager {
    pub async fn from_config(config: ()) -> Self {
        todo!()
    }

    pub fn get_tool_from_name(name: &str) -> Tool {
        todo!()
    }
}
