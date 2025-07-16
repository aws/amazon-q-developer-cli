use std::borrow::Borrow;
use std::collections::HashMap;
use std::ops::Deref;

use schemars::{
    JsonSchema,
    Schema,
    SchemaGenerator,
    json_schema,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::McpServerConfig;
use crate::cli::chat::cli::hooks::Hook;

/// Subject of the tool name change. For tools in mcp servers, you would need to prefix them with
/// their server names
#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, JsonSchema)]
pub struct OriginalToolName(String);

impl Deref for OriginalToolName {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for OriginalToolName {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

pub fn alias_schema(generator: &mut SchemaGenerator) -> Schema {
    let key_schema = generator.subschema_for::<OriginalToolName>();
    let key_description = key_schema
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("Subject of the tool name change. For tools in mcp servers, you would need to prefix them with their server names");

    json_schema!({
        "type": "object",
        "additionalProperties": {
            "type": "string",
            "description": "The name to change to. For tools in mcp servers, you would need to exclude their server prefix"
        },
        "propertyNames": {
            "type": "string",
            "description": key_description
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(untagged)]
pub enum CreateHooks {
    /// Array of command to execute before the start of the conversation
    List(Vec<String>),
    /// Object mapping hook names to command strings for command to run at the start of the
    /// conversation
    Map(HashMap<String, Hook>),
}

impl Default for CreateHooks {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(untagged)]
pub enum PromptHooks {
    /// Array of command to execute before exchange
    List(Vec<String>),
    /// Object mapping hook names to command strings for command to be ran before each exchange
    Map(HashMap<String, Hook>),
}

impl Default for PromptHooks {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

/// The name of the tool to be configured
#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, JsonSchema)]
pub struct ToolSettingTarget(String);

impl Deref for ToolSettingTarget {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for ToolSettingTarget {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

pub fn tool_settings_schema(generator: &mut SchemaGenerator) -> Schema {
    let key_schema = generator.subschema_for::<ToolSettingTarget>();
    let key_description = key_schema
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("The name of the tool to be configured");

    json_schema!({
        "type": "object",
        "additionalProperties": {
            "type": "string",
            "description": "Settings for tools. Refer to our documentations to see how to configure them"
        },
        "propertyNames": {
            "type": "string",
            "description": key_description
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(untagged)]
pub enum McpServerConfigWrapper {
    /// Array of servers to instantiate in accordance to what is in the global mcp.json
    List(Vec<String>),
    /// A map of mcp server configurations, identical in format to the value of mcpServers in
    /// the global mcp.json
    Map(McpServerConfig),
}

impl Default for McpServerConfigWrapper {
    fn default() -> Self {
        Self::List(vec!["*".to_string()])
    }
}

impl McpServerConfigWrapper {
    pub fn get_server_names(&self) -> Vec<&str> {
        match self {
            Self::List(list) => list.iter().map(|name| name.as_str()).collect::<Vec<_>>(),
            Self::Map(config) => config.mcp_servers.keys().map(|name| name.as_str()).collect::<Vec<_>>(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::List(list) => list.is_empty(),
            Self::Map(config) => config.mcp_servers.is_empty(),
        }
    }
}

/// This is a marker trait to be used in conjunction with [Cold]
pub trait SerdeReady: for<'de> Deserialize<'de> + Serialize {}

#[derive(Deserialize, Serialize)]
pub struct Cold;

impl SerdeReady for Cold {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Warm;
