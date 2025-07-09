use std::borrow::Borrow;
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

pub fn prompt_hooks_schema(_generator: &mut SchemaGenerator) -> Schema {
    json_schema!({
      "oneOf": [
        {
          "type": "array",
          "description": "Array of command to execute before each prompt",
          "items": {
            "type": "string"
          }
        },
        {
          "type": "object",
          "description": "Object mapping hook names to command strings for prompt processing",
          "additionalProperties": {
            "type": "string",
            "description": "The command string to execute for this prompt hook"
          },
          "propertyNames": {
            "type": "string",
            "description": "The name/identifier for this prompt hook"
          }
        }
      ]
    })
}

pub fn create_hooks_schema(_generator: &mut SchemaGenerator) -> Schema {
    json_schema!({
      "oneOf": [
        {
          "type": "array",
          "description": "Array of command to execute",
          "items": {
            "type": "string"
          }
        },
        {
          "type": "object",
          "description": "Object mapping hook names to command strings",
          "additionalProperties": {
            "type": "string",
            "description": "The command string to execute for this hook"
          },
          "propertyNames": {
            "type": "string",
            "description": "The name/identifier for this hook"
          }
        }
      ]
    })
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
