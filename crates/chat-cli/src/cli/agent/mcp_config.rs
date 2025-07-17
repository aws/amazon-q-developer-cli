use std::collections::HashMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

use crate::cli::chat::tools::custom_tool::CustomToolConfig;
use crate::os::Os;

// This is to mirror claude's config set up
#[derive(Clone, Serialize, Deserialize, Debug, Default, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase", transparent)]
pub struct McpServerConfig {
    pub mcp_servers: HashMap<String, CustomToolConfig>,
}

impl McpServerConfig {
    pub async fn load_from_file(os: &Os, path: impl AsRef<Path>) -> eyre::Result<Self> {
        let contents = os.fs.read(path.as_ref()).await?;
        let value = serde_json::from_slice::<serde_json::Value>(&contents)?;
        // We need to extract mcp_servers field from the value because we have annotated
        // [McpServerConfig] with transparent. Transparent was added because we want to preserve
        // the type in agent.
        let config = value
            .get("mcpServers")
            .cloned()
            .ok_or(eyre::eyre!("No mcp servers found in config"))?;
        Ok(serde_json::from_value(config)?)
    }

    pub async fn save_to_file(&self, os: &Os, path: impl AsRef<Path>) -> eyre::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        os.fs.write(path.as_ref(), json).await?;
        Ok(())
    }
}
