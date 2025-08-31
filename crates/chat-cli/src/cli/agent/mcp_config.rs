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

        let mut mcp_config: Self = serde_json::from_value(config)?;
        // Substitute environment variables in MCP server configurations
        for server_config in mcp_config.mcp_servers.values_mut() {
            if let Some(ref mut env_vars) = server_config.env {
                crate::cli::chat::tools::custom_tool::process_env_vars(env_vars, &os.env);
            }
        }
        Ok(mcp_config)
    }

    pub async fn save_to_file(&self, os: &Os, path: impl AsRef<Path>) -> eyre::Result<()> {
        let json = self.to_non_transparent_json_pretty()?;
        os.fs.write(path.as_ref(), json).await?;
        Ok(())
    }

    /// Because we had annotated [McpServerConfig] with transparent, when writing the config alone
    /// to its legacy location (as opposed to writing it along with its agent config), we would
    /// need to call this function to stringify it otherwise we would be writing only the inner
    /// hashmap.
    fn to_non_transparent_json_pretty(&self) -> eyre::Result<String> {
        let transparent_json = serde_json::to_value(self)?;
        let non_transparent_json = serde_json::json!({
            "mcpServers": transparent_json
        });
        Ok(serde_json::to_string_pretty(&non_transparent_json)?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::os::Os;

    #[tokio::test]
    async fn test_load_from_file_with_env_substitution() {
        let os = Os::new().await.unwrap();

        // Set test environment variables
        unsafe {
            os.env.set_var("TEST_DB_URL", "postgresql://localhost:5432/test");
            os.env.set_var("TEST_PORT", "3000");
        }

        // Create test MCP config with environment variables
        let config = json!({
            "mcpServers": {
                "database-server": {
                    "command": "node",
                    "args": ["server.js"],
                    "env": {
                        "DATABASE_URL": "${env:TEST_DB_URL}",
                        "PORT": "${env:TEST_PORT}",
                        "NODE_ENV": "test",
                        "MISSING": "${env:DOES_NOT_EXIST}"
                    },
                }
            }
        });

        // Create a temporary file using the Os filesystem
        let temp_path = "/tmp/test_config.json";
        os.fs.create_dir_all("/tmp").await.unwrap();
        os.fs
            .write(temp_path, serde_json::to_string_pretty(&config).unwrap())
            .await
            .unwrap();

        // Load and test
        let loaded_config = McpServerConfig::load_from_file(&os, temp_path).await.unwrap();

        let server = loaded_config.mcp_servers.get("database-server").unwrap();
        let env_vars = server.env.as_ref().unwrap();

        // Verify substitution worked
        assert_eq!(
            env_vars.get("DATABASE_URL").unwrap(),
            "postgresql://localhost:5432/test"
        );
        assert_eq!(env_vars.get("PORT").unwrap(), "3000");
        assert_eq!(env_vars.get("NODE_ENV").unwrap(), "test");
        // Non-existent variable should keep original format
        assert_eq!(env_vars.get("MISSING").unwrap(), "${env:DOES_NOT_EXIST}");

        // Verify other fields are preserved
        assert_eq!(server.command, "node");
        assert_eq!(server.args, vec!["server.js"]);

        // Clean up
        let _ = os.fs.remove_file(temp_path).await;
    }
}
