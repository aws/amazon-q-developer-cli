use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

pub mod client;
pub mod error;
pub mod server;
pub mod transport;

pub use client::*;
pub use transport::*;

/// https://spec.modelcontextprotocol.io/specification/2024-11-05/server/utilities/pagination/#operations-supporting-pagination
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaginationSupportedOps {
    ResourcesList,
    ResourceTemplatesList,
    PromptsList,
    ToolsList,
}

impl PaginationSupportedOps {
    fn as_key(&self) -> &str {
        match self {
            PaginationSupportedOps::ResourcesList => "resources",
            PaginationSupportedOps::ResourceTemplatesList => "resourceTemplates",
            PaginationSupportedOps::PromptsList => "prompts",
            PaginationSupportedOps::ToolsList => "tools",
        }
    }
}

impl TryFrom<&str> for PaginationSupportedOps {
    type Error = OpsConversionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "resources/list" => Ok(PaginationSupportedOps::ResourcesList),
            "resources/templates/list" => Ok(PaginationSupportedOps::ResourceTemplatesList),
            "prompts/list" => Ok(PaginationSupportedOps::PromptsList),
            "tools/list" => Ok(PaginationSupportedOps::ToolsList),
            _ => Err(OpsConversionError::InvalidMethod),
        }
    }
}

#[derive(Error, Debug)]
pub enum OpsConversionError {
    #[error("Invalid method encountered")]
    InvalidMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesListResult {
    resources: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplatesListResult {
    resource_templates: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsListResult {
    prompts: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsListResult {
    tools: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}
