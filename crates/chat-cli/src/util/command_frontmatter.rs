use serde::{
    Deserialize,
    Serialize,
};

use crate::util::command_types::CommandError;

/// YAML frontmatter configuration for custom commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CommandFrontmatter {
    /// Human-readable description of the command
    pub description: Option<String>,

    /// List of allowed tools this command can use
    pub allowed_tools: Vec<String>,

    /// Timeout in seconds for command execution
    pub timeout_seconds: Option<u64>,

    /// Maximum output size in bytes
    pub max_output_size: Option<usize>,

    /// Tags for organizing commands
    pub tags: Vec<String>,
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
        // Validate timeout if specified
        if let Some(timeout) = self.timeout_seconds {
            if timeout == 0 || timeout > 300 {
                return Err(CommandError::InvalidFormat(
                    "Timeout must be between 1 and 300 seconds".to_string(),
                ));
            }
        }

        // Validate max_output_size if specified
        if let Some(max_size) = self.max_output_size {
            if max_size == 0 || max_size > 1024 * 1024 {
                return Err(CommandError::InvalidFormat(
                    "Max output size must be between 1 byte and 1MB".to_string(),
                ));
            }
        }

        // Validate allowed_tools
        for tool in &self.allowed_tools {
            if tool.trim().is_empty() {
                return Err(CommandError::InvalidFormat("Tool names cannot be empty".to_string()));
            }
        }

        Ok(())
    }

    /// Convert frontmatter to YAML string
    #[allow(dead_code)]
    pub fn to_yaml(&self) -> Result<String, CommandError> {
        serde_yaml::to_string(self).map_err(|e| CommandError::InvalidFormat(format!("YAML serialization error: {}", e)))
    }

    /// Check if a specific tool is allowed
    #[allow(dead_code)]
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        self.allowed_tools.contains(&tool_name.to_string())
    }

    /// Get timeout with default fallback
    #[allow(dead_code)]
    pub fn get_timeout_seconds(&self) -> u64 {
        self.timeout_seconds.unwrap_or(30)
    }

    /// Get max output size with default fallback
    #[allow(dead_code)]
    pub fn get_max_output_size(&self) -> usize {
        self.max_output_size.unwrap_or(4096)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_with_yaml() {
        let content = r#"---
description: "Test command"
allowed_tools: ["execute_bash"]
timeout_seconds: 60
tags: ["test"]
---

# Test Command
This is test content."#;

        let (frontmatter, markdown) = CommandFrontmatter::parse_from_content(content).unwrap();

        assert_eq!(frontmatter.description, Some("Test command".to_string()));
        assert_eq!(frontmatter.allowed_tools, vec!["execute_bash"]);
        assert_eq!(frontmatter.timeout_seconds, Some(60));
        assert_eq!(frontmatter.tags, vec!["test"]);
        assert!(markdown.starts_with("# Test Command"));
    }

    #[test]
    fn test_parse_frontmatter_without_yaml() {
        let content = "# Simple Command\nThis has no frontmatter.";

        let (frontmatter, markdown) = CommandFrontmatter::parse_from_content(content).unwrap();

        assert_eq!(frontmatter.description, None);
        assert!(frontmatter.allowed_tools.is_empty());
        assert_eq!(markdown, content);
    }

    #[test]
    fn test_frontmatter_validation() {
        let mut frontmatter = CommandFrontmatter::default();

        // Valid frontmatter should pass
        assert!(frontmatter.validate().is_ok());

        // Invalid timeout should fail
        frontmatter.timeout_seconds = Some(0);
        assert!(frontmatter.validate().is_err());

        frontmatter.timeout_seconds = Some(500);
        assert!(frontmatter.validate().is_err());

        // Valid timeout should pass
        frontmatter.timeout_seconds = Some(30);
        assert!(frontmatter.validate().is_ok());
    }

    #[test]
    fn test_tool_permission_checking() {
        let mut frontmatter = CommandFrontmatter::default();
        frontmatter.allowed_tools = vec!["execute_bash".to_string(), "fs_read".to_string()];

        assert!(frontmatter.is_tool_allowed("execute_bash"));
        assert!(frontmatter.is_tool_allowed("fs_read"));
        assert!(!frontmatter.is_tool_allowed("fs_write"));
    }

    #[test]
    fn test_default_values() {
        let frontmatter = CommandFrontmatter::default();

        assert_eq!(frontmatter.get_timeout_seconds(), 30);
        assert_eq!(frontmatter.get_max_output_size(), 4096);
    }

    #[test]
    fn test_yaml_serialization() {
        let mut frontmatter = CommandFrontmatter::default();
        frontmatter.description = Some("Test command".to_string());
        frontmatter.allowed_tools = vec!["execute_bash".to_string()];

        let yaml = frontmatter.to_yaml().unwrap();
        assert!(yaml.contains("description: Test command"));
        assert!(yaml.contains("allowed_tools:"));
        assert!(yaml.contains("- execute_bash"));
    }
}
