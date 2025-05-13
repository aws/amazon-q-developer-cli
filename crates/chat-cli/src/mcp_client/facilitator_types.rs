use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

/// https://spec.modelcontextprotocol.io/specification/2024-11-05/server/utilities/pagination/#operations-supporting-pagination
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaginationSupportedOps {
    Resources,
    ResourceTemplates,
    Prompts,
    Tools,
}

impl PaginationSupportedOps {
    pub fn as_key(&self) -> &str {
        match self {
            PaginationSupportedOps::Resources => "resources",
            PaginationSupportedOps::ResourceTemplates => "resourceTemplates",
            PaginationSupportedOps::Prompts => "prompts",
            PaginationSupportedOps::Tools => "tools",
        }
    }
}

impl TryFrom<&str> for PaginationSupportedOps {
    type Error = OpsConversionError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "resources/list" => Ok(PaginationSupportedOps::Resources),
            "resources/templates/list" => Ok(PaginationSupportedOps::ResourceTemplates),
            "prompts/list" => Ok(PaginationSupportedOps::Prompts),
            "tools/list" => Ok(PaginationSupportedOps::Tools),
            _ => Err(OpsConversionError::InvalidMethod),
        }
    }
}

#[derive(Error, Debug)]
pub enum OpsConversionError {
    #[error("Invalid method encountered")]
    InvalidMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// Role assumed for a particular message
pub enum Role {
    User,
    Assistant,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of listing resources operation
pub struct ResourcesListResult {
    /// List of resources
    pub resources: Vec<serde_json::Value>,
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Result of listing resource templates operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplatesListResult {
    /// List of resource templates
    pub resource_templates: Vec<serde_json::Value>,
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of prompt listing query
pub struct PromptsListResult {
    /// List of prompts
    pub prompts: Vec<serde_json::Value>,
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Represents an argument to be supplied to a [PromptGet]
pub struct PromptGetArg {
    /// The name identifier of the prompt
    pub name: String,
    /// Optional description providing context about the prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Indicates whether a response to this prompt is required
    /// If not specified, defaults to false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Represents a request to get a prompt from a mcp server
pub struct PromptGet {
    /// Unique identifier for the prompt
    pub name: String,
    /// Optional description providing context about the prompt's purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional list of arguments that define the structure of information to be collected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptGetArg>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// `result` field in [JsonRpcResponse] from a `prompts/get` request
pub struct PromptGetResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub messages: Vec<Prompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Completed prompt from `prompts/get` to be returned by a mcp server
pub struct Prompt {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Result of listing tools operation
pub struct ToolsListResult {
    /// List of tools
    pub tools: Vec<serde_json::Value>,
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MessageContent {
    /// Text content
    Text {
        /// The text content
        text: String,
    },
    /// Image content
    #[serde(rename_all = "camelCase")]
    Image {
        /// base64-encoded-data
        data: String,
        mime_type: String,
    },
    /// Resource content
    Resource {
        /// The resource
        resource: Resource,
    },
}

impl From<MessageContent> for String {
    fn from(val: MessageContent) -> Self {
        match val {
            MessageContent::Text { text } => text,
            MessageContent::Image { data, mime_type } => serde_json::json!({
                "data": data,
                "mime_type": mime_type
            })
            .to_string(),
            MessageContent::Resource { resource } => serde_json::json!(resource).to_string(),
        }
    }
}

impl std::fmt::Display for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageContent::Text { text } => write!(f, "{}", text),
            MessageContent::Image { data: _, mime_type } => write!(f, "Image [base64-encoded-string] ({})", mime_type),
            MessageContent::Resource { resource } => write!(f, "Resource: {} ({})", resource.title, resource.uri),
        }
    }
}

/// Resource contents
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ResourceContents {
    Text { text: String },
    Blob { data: Vec<u8> },
}

/// A resource in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Unique identifier for the resource
    pub uri: String,
    /// Human-readable title
    pub title: String,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Resource contents
    pub contents: ResourceContents,
}
