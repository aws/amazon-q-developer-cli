use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use crate::util::command_types::CommandError;

/// YAML frontmatter configuration for custom commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CommandFrontmatter {
    // Basic metadata
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,

    // Tool permissions (Phase 1 requirement)
    pub allowed_tools: Vec<String>,
    pub thinking_mode: Option<bool>,
    pub timeout: Option<u32>,

    // Parameter definitions (Phase 3 feature)
    pub parameters: Vec<Parameter>,

    // Extensibility for future features
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Parameter definition for command frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: ParameterType,
    #[serde(default)]
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub description: Option<String>,
    pub options: Option<Vec<serde_json::Value>>,
    pub validation: Option<ParameterValidation>,
}

/// Supported parameter types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Integer,
    Float,
    Boolean,
    Array,
    Object,
}

/// Parameter validation constraints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterValidation {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

impl CommandFrontmatter {
    /// Parse YAML frontmatter from markdown content
    pub fn parse_from_content(content: &str) -> Result<(Self, String), CommandError> {
        if let Some(stripped) = content.strip_prefix("---\n") {
            if let Some(end_pos) = stripped.find("\n---\n") {
                let yaml_content = &stripped[..end_pos];
                let markdown_content = &stripped[end_pos + 5..]; // Skip "\n---\n"

                let frontmatter: CommandFrontmatter = serde_yaml::from_str(yaml_content)
                    .map_err(|e| CommandError::InvalidFormat(format!("YAML parse error: {}", e)))?;

                return Ok((frontmatter, markdown_content.trim().to_string()));
            }
        }

        // No frontmatter found, return default
        Ok((CommandFrontmatter::default(), content.to_string()))
    }

    /// Validate frontmatter configuration
    pub fn validate(&self) -> Result<(), CommandError> {
        // Validate parameter definitions
        for param in &self.parameters {
            param.validate()?;
        }

        // Validate timeout if specified
        if let Some(timeout) = self.timeout {
            if timeout == 0 || timeout > 300 {
                return Err(CommandError::InvalidFormat(
                    "Timeout must be between 1 and 300 seconds".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Parameter {
    /// Validate parameter definition
    pub fn validate(&self) -> Result<(), CommandError> {
        // Validate parameter name
        if self.name.is_empty() {
            return Err(CommandError::InvalidFormat(
                "Parameter name cannot be empty".to_string(),
            ));
        }

        // Validate parameter name format (alphanumeric + underscore)
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(CommandError::InvalidFormat(format!(
                "Invalid parameter name '{}': only alphanumeric characters and underscores allowed",
                self.name
            )));
        }

        // Validate default value type matches parameter type
        if let Some(default_value) = &self.default {
            if !self.is_value_type_compatible(default_value) {
                return Err(CommandError::InvalidFormat(format!(
                    "Default value type doesn't match parameter type for '{}'",
                    self.name
                )));
            }
        }

        // Validate options if specified
        if let Some(options) = &self.options {
            if options.is_empty() {
                return Err(CommandError::InvalidFormat(format!(
                    "Options list cannot be empty for parameter '{}'",
                    self.name
                )));
            }

            for option in options {
                if !self.is_value_type_compatible(option) {
                    return Err(CommandError::InvalidFormat(format!(
                        "Option value type doesn't match parameter type for '{}'",
                        self.name
                    )));
                }
            }
        }

        Ok(())
    }

    /// Check if a value is compatible with the parameter type
    fn is_value_type_compatible(&self, value: &serde_json::Value) -> bool {
        match (&self.param_type, value) {
            (ParameterType::String, serde_json::Value::String(_)) => true,
            (ParameterType::Integer, serde_json::Value::Number(n)) => n.is_i64(),
            (ParameterType::Float, serde_json::Value::Number(_)) => true,
            (ParameterType::Boolean, serde_json::Value::Bool(_)) => true,
            (ParameterType::Array, serde_json::Value::Array(_)) => true,
            (ParameterType::Object, serde_json::Value::Object(_)) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_with_yaml() {
        let content = r#"---
name: "Test Command"
description: "A test command"
allowed_tools: ["execute_bash"]
parameters:
  - name: "action"
    type: "string"
    required: true
    options: ["start", "stop"]
---

# Test Command Content

This is the command content."#;

        let (frontmatter, markdown) = CommandFrontmatter::parse_from_content(content).unwrap();

        assert_eq!(frontmatter.name, Some("Test Command".to_string()));
        assert_eq!(frontmatter.description, Some("A test command".to_string()));
        assert_eq!(frontmatter.allowed_tools, vec!["execute_bash"]);
        assert_eq!(frontmatter.parameters.len(), 1);
        assert_eq!(frontmatter.parameters[0].name, "action");
        assert!(markdown.starts_with("# Test Command Content"));
    }

    #[test]
    fn test_parse_frontmatter_without_yaml() {
        let content = "# Simple Command\n\nThis is a simple command without frontmatter.";

        let (frontmatter, markdown) = CommandFrontmatter::parse_from_content(content).unwrap();

        assert_eq!(frontmatter.name, None);
        assert_eq!(frontmatter.allowed_tools.len(), 0);
        assert_eq!(frontmatter.parameters.len(), 0);
        assert_eq!(markdown, content);
    }

    #[test]
    fn test_parameter_validation() {
        let valid_param = Parameter {
            name: "test_param".to_string(),
            param_type: ParameterType::String,
            required: true,
            default: Some(serde_json::Value::String("default".to_string())),
            description: Some("Test parameter".to_string()),
            options: None,
            validation: None,
        };

        assert!(valid_param.validate().is_ok());

        let invalid_param = Parameter {
            name: "".to_string(),
            param_type: ParameterType::String,
            required: false,
            default: None,
            description: None,
            options: None,
            validation: None,
        };

        assert!(invalid_param.validate().is_err());
    }

    #[test]
    fn test_type_compatibility() {
        let string_param = Parameter {
            name: "test".to_string(),
            param_type: ParameterType::String,
            required: false,
            default: None,
            description: None,
            options: None,
            validation: None,
        };

        assert!(string_param.is_value_type_compatible(&serde_json::Value::String("test".to_string())));
        assert!(!string_param.is_value_type_compatible(&serde_json::Value::Number(42.into())));
    }
}
