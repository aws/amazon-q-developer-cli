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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::Map;

    use super::*;

    #[test]
    fn test_tool_spec_from_rmcp_tool_full() {
        let tool = RmcpTool {
            name: "my_tool".into(),
            description: Some("A useful tool".into()),
            input_schema: Arc::new(Map::new()),
            output_schema: None,
            annotations: None,
            title: None,
            icons: None,
            execution: None,
            meta: None,
        };
        let spec: ToolSpec = tool.into();
        assert_eq!(spec.name, "my_tool");
        assert_eq!(spec.description, "A useful tool");
    }

    #[test]
    fn test_tool_spec_from_rmcp_tool_no_description() {
        let tool = RmcpTool {
            name: "tool".into(),
            description: None,
            input_schema: Arc::new(Map::new()),
            output_schema: None,
            annotations: None,
            title: None,
            icons: None,
            execution: None,
            meta: None,
        };
        let spec: ToolSpec = tool.into();
        assert_eq!(spec.name, "tool");
        assert_eq!(spec.description, "");
    }

    #[test]
    fn test_prompt_serde_minimal() {
        let p = Prompt {
            name: "greet".to_string(),
            description: None,
            arguments: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""name":"greet""#));
        // Skip if None
        assert!(!json.contains("description"));
        assert!(!json.contains("arguments"));

        let parsed: Prompt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, p);
    }

    #[test]
    fn test_prompt_serde_full() {
        let p = Prompt {
            name: "summary".to_string(),
            description: Some("Summarize text".to_string()),
            arguments: Some(vec![PromptArgument {
                name: "text".to_string(),
                description: Some("Text to summarize".to_string()),
                required: Some(true),
            }]),
        };
        let json = serde_json::to_string(&p).unwrap();
        let parsed: Prompt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, p);
    }

    #[test]
    fn test_prompt_argument_serde() {
        let a = PromptArgument {
            name: "x".to_string(),
            description: None,
            required: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains(r#""name":"x""#));
        let parsed: PromptArgument = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, a);
    }

    #[test]
    fn test_prompt_argument_from_rmcp() {
        let arg = RmcpPromptArgument {
            name: "topic".to_string(),
            description: Some("Topic of summary".to_string()),
            required: Some(true),
            title: None,
        };
        let result: PromptArgument = arg.into();
        assert_eq!(result.name, "topic");
        assert_eq!(result.description, Some("Topic of summary".to_string()));
        assert_eq!(result.required, Some(true));
    }

    #[test]
    fn test_prompt_argument_from_rmcp_minimal() {
        let arg = RmcpPromptArgument {
            name: "x".to_string(),
            description: None,
            required: None,
            title: None,
        };
        let result: PromptArgument = arg.into();
        assert_eq!(result.name, "x");
        assert_eq!(result.description, None);
        assert_eq!(result.required, None);
    }

    #[test]
    fn test_prompt_from_rmcp_minimal() {
        let prompt = RmcpPrompt {
            name: "code_review".to_string(),
            description: None,
            arguments: None,
            title: None,
            icons: None,
            meta: None,
        };
        let result: Prompt = prompt.into();
        assert_eq!(result.name, "code_review");
        assert_eq!(result.description, None);
        assert!(result.arguments.is_none());
    }

    #[test]
    fn test_prompt_from_rmcp_full() {
        let prompt = RmcpPrompt {
            name: "explain".to_string(),
            description: Some("Explain code".to_string()),
            arguments: Some(vec![RmcpPromptArgument {
                name: "code".to_string(),
                description: Some("Code to explain".to_string()),
                required: Some(true),
                title: None,
            }]),
            title: None,
            icons: None,
            meta: None,
        };
        let result: Prompt = prompt.into();
        assert_eq!(result.name, "explain");
        assert_eq!(result.description, Some("Explain code".to_string()));
        let args = result.arguments.unwrap();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].name, "code");
    }

    #[test]
    fn test_prompt_serde_camel_case() {
        // Test that camelCase serialization is applied
        let p = Prompt {
            name: "test".to_string(),
            description: Some("d".to_string()),
            arguments: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        // verify we don't have snake_case - this struct only has simple names so just smoke check
        assert!(json.contains("name"));
    }
}
