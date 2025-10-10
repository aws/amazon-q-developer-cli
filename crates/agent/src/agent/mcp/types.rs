use rmcp::model::{
    Prompt as RmcpPrompt,
    PromptArgument as RmcpPromptArgument,
    Tool as RmcpTool,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::agent::agent_loop::types::ToolSpec;

impl From<RmcpTool> for ToolSpec {
    fn from(value: RmcpTool) -> Self {
        Self {
            name: value.name.to_string(),
            description: value.description.map(String::from).unwrap_or_default(),
            input_schema: (*value.input_schema).clone(),
        }
    }
}

/// A prompt that can be used to generate text from a model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// The name of the prompt
    pub name: String,
    /// Optional description of what the prompt does
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional arguments that can be passed to customize the prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Represents a prompt argument that can be passed to customize the prompt
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    /// The name of the argument
    pub name: String,
    /// A description of what the argument is used for
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this argument is required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

impl From<RmcpPrompt> for Prompt {
    fn from(value: RmcpPrompt) -> Self {
        Self {
            name: value.name,
            description: value.description,
            arguments: value.arguments.map(|v| v.into_iter().map(Into::into).collect()),
        }
    }
}

impl From<RmcpPromptArgument> for PromptArgument {
    fn from(value: RmcpPromptArgument) -> Self {
        Self {
            name: value.name,
            description: value.description,
            required: value.required,
        }
    }
}
