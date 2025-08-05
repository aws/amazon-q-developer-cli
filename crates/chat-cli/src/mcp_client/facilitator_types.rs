use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

/// https://spec.modelcontextprotocol.io/specification/2024-11-05/server/utilities/pagination/#operations-supporting-pagination
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaginationSupportedOps {
    ResourcesList,
    ResourceTemplatesList,
    PromptsList,
    ToolsList,
}

impl PaginationSupportedOps {
    pub fn as_key(&self) -> &str {
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

/// Represents the capabilities supported by a Model Context Protocol server
/// This is the "capabilities" field in the result of a response for init
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Configuration for server logging capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<serde_json::Value>,
    /// Configuration for prompt-related capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<serde_json::Value>,
    /// Configuration for resource management capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
    /// Configuration for tool integration capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    /// Configuration for sampling capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<serde_json::Value>,
}

// Sampling-related types for MCP sampling specification

/// Model preferences for sampling requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferences {
    /// Model hints in order of preference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,
    /// Priority for cost optimization (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,
    /// Priority for speed optimization (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,
    /// Priority for intelligence/capability (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,
}

/// Model hint for sampling requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    /// Model name or substring to match
    pub name: String,
}

/// Message for sampling requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    /// Role of the message sender
    pub role: Role,
    /// Content of the message
    pub content: SamplingContent,
}

/// Content types for sampling messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SamplingContent {
    /// Text content
    Text {
        /// The text content
        text: String,
    },
    /// Image content
    #[serde(rename_all = "camelCase")]
    Image {
        /// base64-encoded image data
        data: String,
        /// MIME type of the image
        mime_type: String,
    },
    /// Audio content
    #[serde(rename_all = "camelCase")]
    Audio {
        /// base64-encoded audio data
        data: String,
        /// MIME type of the audio
        mime_type: String,
    },
}

/// Request parameters for sampling/createMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingCreateMessageRequest {
    /// Messages to send to the model
    pub messages: Vec<SamplingMessage>,
    /// Model preferences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,
    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Response from sampling/createMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingCreateMessageResponse {
    /// Role of the response (typically "assistant")
    pub role: Role,
    /// Content of the response
    pub content: SamplingContent,
    /// Model that generated the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Reason for stopping generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_supported_ops_as_key() {
        assert_eq!(PaginationSupportedOps::ResourcesList.as_key(), "resources");
        assert_eq!(
            PaginationSupportedOps::ResourceTemplatesList.as_key(),
            "resourceTemplates"
        );
        assert_eq!(PaginationSupportedOps::PromptsList.as_key(), "prompts");
        assert_eq!(PaginationSupportedOps::ToolsList.as_key(), "tools");
    }

    #[test]
    fn test_pagination_supported_ops_try_from() {
        assert_eq!(
            PaginationSupportedOps::try_from("resources/list").unwrap(),
            PaginationSupportedOps::ResourcesList
        );
        assert_eq!(
            PaginationSupportedOps::try_from("resources/templates/list").unwrap(),
            PaginationSupportedOps::ResourceTemplatesList
        );
        assert_eq!(
            PaginationSupportedOps::try_from("prompts/list").unwrap(),
            PaginationSupportedOps::PromptsList
        );
        assert_eq!(
            PaginationSupportedOps::try_from("tools/list").unwrap(),
            PaginationSupportedOps::ToolsList
        );

        // Test invalid method
        assert!(PaginationSupportedOps::try_from("invalid/method").is_err());
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
    }

    #[test]
    fn test_role_serialization() {
        let user_json = serde_json::to_value(Role::User).unwrap();
        let assistant_json = serde_json::to_value(Role::Assistant).unwrap();

        assert_eq!(user_json, serde_json::Value::String("user".to_string()));
        assert_eq!(assistant_json, serde_json::Value::String("assistant".to_string()));

        // Test deserialization
        let user_role: Role = serde_json::from_value(user_json).unwrap();
        let assistant_role: Role = serde_json::from_value(assistant_json).unwrap();

        assert_eq!(user_role, Role::User);
        assert_eq!(assistant_role, Role::Assistant);
    }

    #[test]
    fn test_message_content_display() {
        let text_content = MessageContent::Text {
            text: "Hello world".to_string(),
        };
        assert_eq!(text_content.to_string(), "Hello world");

        let image_content = MessageContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/jpeg".to_string(),
        };
        assert_eq!(image_content.to_string(), "Image [base64-encoded-string] (image/jpeg)");

        let resource_content = MessageContent::Resource {
            resource: Resource {
                uri: "file://test.txt".to_string(),
                title: "Test File".to_string(),
                description: None,
                contents: ResourceContents::Text {
                    text: "content".to_string(),
                },
            },
        };
        assert_eq!(resource_content.to_string(), "Resource: Test File (file://test.txt)");
    }

    #[test]
    fn test_message_content_from_string() {
        let text_content = MessageContent::Text {
            text: "Hello world".to_string(),
        };
        let result: String = text_content.into();
        assert_eq!(result, "Hello world");

        let image_content = MessageContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/jpeg".to_string(),
        };
        let result: String = image_content.into();
        assert!(result.contains("base64data"));
        assert!(result.contains("image/jpeg"));
    }

    #[test]
    fn test_sampling_message_serialization() {
        let message = SamplingMessage {
            role: Role::User,
            content: SamplingContent::Text {
                text: "Hello".to_string(),
            },
        };

        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"]["type"], "text");
        assert_eq!(json["content"]["text"], "Hello");

        // Test deserialization
        let deserialized: SamplingMessage = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.role, Role::User);
        match deserialized.content {
            SamplingContent::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_sampling_content_serialization() {
        // Test Text content
        let text_content = SamplingContent::Text {
            text: "Hello world".to_string(),
        };
        let text_json = serde_json::to_value(&text_content).unwrap();
        assert_eq!(text_json["type"], "text");
        assert_eq!(text_json["text"], "Hello world");

        // Test Image content
        let image_content = SamplingContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };
        let image_json = serde_json::to_value(&image_content).unwrap();
        assert_eq!(image_json["type"], "image");
        assert_eq!(image_json["data"], "base64data");
        assert_eq!(image_json["mimeType"], "image/png");

        // Test Audio content
        let audio_content = SamplingContent::Audio {
            data: "audiodata".to_string(),
            mime_type: "audio/wav".to_string(),
        };
        let audio_json = serde_json::to_value(&audio_content).unwrap();
        assert_eq!(audio_json["type"], "audio");
        assert_eq!(audio_json["data"], "audiodata");
        assert_eq!(audio_json["mimeType"], "audio/wav");

        // Test deserialization
        let text_deserialized: SamplingContent = serde_json::from_value(text_json).unwrap();
        let image_deserialized: SamplingContent = serde_json::from_value(image_json).unwrap();
        let audio_deserialized: SamplingContent = serde_json::from_value(audio_json).unwrap();

        match text_deserialized {
            SamplingContent::Text { text } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected text content"),
        }

        match image_deserialized {
            SamplingContent::Image { data, mime_type } => {
                assert_eq!(data, "base64data");
                assert_eq!(mime_type, "image/png");
            },
            _ => panic!("Expected image content"),
        }

        match audio_deserialized {
            SamplingContent::Audio { data, mime_type } => {
                assert_eq!(data, "audiodata");
                assert_eq!(mime_type, "audio/wav");
            },
            _ => panic!("Expected audio content"),
        }
    }

    #[test]
    fn test_model_preferences_serialization() {
        let preferences = ModelPreferences {
            hints: Some(vec![
                ModelHint {
                    name: "claude-3-sonnet".to_string(),
                },
                ModelHint {
                    name: "gpt-4".to_string(),
                },
            ]),
            cost_priority: Some(0.3),
            speed_priority: Some(0.8),
            intelligence_priority: Some(0.9),
        };

        let json = serde_json::to_value(&preferences).unwrap();
        assert!(json.get("hints").is_some());
        assert_eq!(json["costPriority"], 0.3);
        assert_eq!(json["speedPriority"], 0.8);
        assert_eq!(json["intelligencePriority"], 0.9);

        // Test deserialization
        let deserialized: ModelPreferences = serde_json::from_value(json).unwrap();
        assert!(deserialized.hints.is_some());
        assert_eq!(deserialized.hints.as_ref().unwrap().len(), 2);
        assert_eq!(deserialized.cost_priority, Some(0.3));
        assert_eq!(deserialized.speed_priority, Some(0.8));
        assert_eq!(deserialized.intelligence_priority, Some(0.9));
    }

    #[test]
    fn test_model_preferences_optional_fields() {
        // Test with no optional fields
        let minimal_preferences = ModelPreferences {
            hints: None,
            cost_priority: None,
            speed_priority: None,
            intelligence_priority: None,
        };

        let json = serde_json::to_value(&minimal_preferences).unwrap();
        // Optional fields should not be present when None
        assert!(json.get("hints").is_none());
        assert!(json.get("costPriority").is_none());
        assert!(json.get("speedPriority").is_none());
        assert!(json.get("intelligencePriority").is_none());

        // Test deserialization of empty object
        let empty_json = serde_json::json!({});
        let deserialized: ModelPreferences = serde_json::from_value(empty_json).unwrap();
        assert!(deserialized.hints.is_none());
        assert!(deserialized.cost_priority.is_none());
        assert!(deserialized.speed_priority.is_none());
        assert!(deserialized.intelligence_priority.is_none());
    }

    #[test]
    fn test_sampling_create_message_request_serialization() {
        let request = SamplingCreateMessageRequest {
            messages: vec![SamplingMessage {
                role: Role::User,
                content: SamplingContent::Text {
                    text: "What is the capital of France?".to_string(),
                },
            }],
            model_preferences: Some(ModelPreferences {
                hints: Some(vec![ModelHint {
                    name: "claude-3-sonnet".to_string(),
                }]),
                cost_priority: Some(0.3),
                speed_priority: Some(0.8),
                intelligence_priority: Some(0.5),
            }),
            system_prompt: Some("You are a helpful assistant.".to_string()),
            max_tokens: Some(100),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("messages").is_some());
        assert!(json.get("modelPreferences").is_some());
        assert_eq!(json["systemPrompt"], "You are a helpful assistant.");
        assert_eq!(json["maxTokens"], 100);

        // Test deserialization
        let deserialized: SamplingCreateMessageRequest = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.messages.len(), 1);
        assert!(deserialized.model_preferences.is_some());
        assert_eq!(
            deserialized.system_prompt,
            Some("You are a helpful assistant.".to_string())
        );
        assert_eq!(deserialized.max_tokens, Some(100));
    }

    #[test]
    fn test_sampling_create_message_response_serialization() {
        let response = SamplingCreateMessageResponse {
            role: Role::Assistant,
            content: SamplingContent::Text {
                text: "The capital of France is Paris.".to_string(),
            },
            model: Some("claude-3-sonnet-20240307".to_string()),
            stop_reason: Some("endTurn".to_string()),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"]["type"], "text");
        assert_eq!(json["content"]["text"], "The capital of France is Paris.");
        assert_eq!(json["model"], "claude-3-sonnet-20240307");
        assert_eq!(json["stopReason"], "endTurn");

        // Test deserialization
        let deserialized: SamplingCreateMessageResponse = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.role, Role::Assistant);
        match deserialized.content {
            SamplingContent::Text { text } => {
                assert_eq!(text, "The capital of France is Paris.");
            },
            _ => panic!("Expected text content"),
        }
        assert_eq!(deserialized.model, Some("claude-3-sonnet-20240307".to_string()));
        assert_eq!(deserialized.stop_reason, Some("endTurn".to_string()));
    }

    #[test]
    fn test_server_capabilities_with_sampling() {
        let capabilities_json = serde_json::json!({
            "logging": {},
            "prompts": { "listChanged": true },
            "resources": {},
            "tools": { "listChanged": true },
            "sampling": {}
        });

        let capabilities: ServerCapabilities = serde_json::from_value(capabilities_json).unwrap();
        assert!(capabilities.logging.is_some());
        assert!(capabilities.prompts.is_some());
        assert!(capabilities.resources.is_some());
        assert!(capabilities.tools.is_some());
        assert!(capabilities.sampling.is_some());

        // Test serialization back
        let serialized = serde_json::to_value(&capabilities).unwrap();
        assert!(serialized.get("sampling").is_some());
    }

    #[test]
    fn test_server_capabilities_without_sampling() {
        let capabilities_json = serde_json::json!({
            "logging": {},
            "prompts": { "listChanged": true },
            "resources": {},
            "tools": { "listChanged": true }
        });

        let capabilities: ServerCapabilities = serde_json::from_value(capabilities_json).unwrap();
        assert!(capabilities.logging.is_some());
        assert!(capabilities.prompts.is_some());
        assert!(capabilities.resources.is_some());
        assert!(capabilities.tools.is_some());
        assert!(capabilities.sampling.is_none());

        // Test serialization back - sampling field should not be present
        let serialized = serde_json::to_value(&capabilities).unwrap();
        assert!(serialized.get("sampling").is_none());
    }

    #[test]
    fn test_model_hint_serialization() {
        let hint = ModelHint {
            name: "claude-3-sonnet".to_string(),
        };

        let json = serde_json::to_value(&hint).unwrap();
        assert_eq!(json["name"], "claude-3-sonnet");

        // Test deserialization
        let deserialized: ModelHint = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.name, "claude-3-sonnet");
    }
}
