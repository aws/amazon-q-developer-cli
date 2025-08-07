use std::path::PathBuf;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::util::command_frontmatter::{
    CommandFrontmatter,
    Parameter,
};

/// Scope of a command (project-specific or global)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandScope {
    /// Command is specific to the current project
    Project,
    /// Command is available globally across all projects
    Global,
}

/// A custom command definition
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Name of the command (derived from filename)
    pub name: String,
    /// Command content (Markdown format, without frontmatter)
    pub content: String,
    /// Path to the command file
    pub file_path: PathBuf,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// YAML frontmatter configuration
    pub frontmatter: CommandFrontmatter,
}

#[allow(dead_code)]
impl CustomCommand {
    /// Create a new command from file path
    #[allow(dead_code)]
    pub fn from_file(file_path: PathBuf) -> Result<Self, CommandError> {
        let content = std::fs::read_to_string(&file_path)?;

        // Parse YAML frontmatter if present
        let (frontmatter, markdown_content) = CommandFrontmatter::parse_from_content(&content)?;

        // Validate frontmatter
        frontmatter.validate()?;

        // Extract command name from filename
        let name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CommandError::InvalidFormat("Invalid filename".to_string()))?
            .to_string();

        Ok(Self {
            name,
            content: markdown_content,
            file_path,
            created_at: Utc::now(),
            frontmatter,
        })
    }

    /// Create a new custom command with default frontmatter
    pub fn new(name: String, content: String, file_path: PathBuf) -> Self {
        Self {
            name,
            content,
            file_path,
            created_at: Utc::now(),
            frontmatter: CommandFrontmatter::default(),
        }
    }

    /// Get parameters defined in frontmatter
    pub fn parameters(&self) -> &[Parameter] {
        &self.frontmatter.parameters
    }

    /// Get allowed tools from frontmatter
    pub fn allowed_tools(&self) -> &[String] {
        &self.frontmatter.allowed_tools
    }

    /// Check if thinking mode is enabled
    pub fn thinking_mode_enabled(&self) -> bool {
        self.frontmatter.thinking_mode.unwrap_or(false)
    }

    /// Get command timeout in seconds
    pub fn timeout_seconds(&self) -> Option<u32> {
        self.frontmatter.timeout
    }

    /// Validate command name
    pub fn validate_name(name: &str) -> Result<(), CommandError> {
        if name.is_empty() {
            return Err(CommandError::InvalidName("Command name cannot be empty".to_string()));
        }

        // Check for reserved names
        const RESERVED_NAMES: &[&str] = &["help", "quit", "exit", "version"];
        if RESERVED_NAMES.contains(&name) {
            return Err(CommandError::InvalidName(format!(
                "Command name '{}' is reserved",
                name
            )));
        }

        // Check for invalid characters
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(CommandError::InvalidName(
                "Command name can only contain alphanumeric characters, hyphens, and underscores".to_string(),
            ));
        }

        // Check length
        if name.len() > 50 {
            return Err(CommandError::InvalidName(
                "Command name cannot exceed 50 characters".to_string(),
            ));
        }

        Ok(())
    }
}

/// Errors that can occur during command operations
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Command '{0}' not found")]
    NotFound(String),

    #[error("Command '{0}' already exists")]
    AlreadyExists(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid command name: {0}")]
    InvalidName(String),

    #[error("Invalid command format: {0}")]
    InvalidFormat(String),

    #[error("Feature disabled. Enable with: q settings chat.enableCommands true")]
    FeatureDisabled,

    #[error("Editor error: {0}")]
    EditorError(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Parameter error: {0}")]
    ParameterError(String),

    #[error("Tool permission error: {0}")]
    ToolPermissionError(String),

    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("File error: {0}")]
    FileError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<serde_json::Error> for CommandError {
    fn from(err: serde_json::Error) -> Self {
        CommandError::InvalidFormat(format!("JSON error: {}", err))
    }
}

impl From<serde_yaml::Error> for CommandError {
    fn from(err: serde_yaml::Error) -> Self {
        CommandError::InvalidFormat(format!("YAML error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_load_command_without_frontmatter() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("simple-command.md");

        fs::write(
            &file_path,
            "# Simple Command\n\nThis is a simple command without frontmatter.",
        )
        .unwrap();

        let command = CustomCommand::from_file(file_path).unwrap();

        assert_eq!(command.name, "simple-command");
        assert!(command.content.starts_with("# Simple Command"));
        assert_eq!(command.frontmatter.parameters.len(), 0);
        assert_eq!(command.frontmatter.allowed_tools.len(), 0);
        assert!(!command.thinking_mode_enabled());
        assert_eq!(command.timeout_seconds(), None);
    }

    #[test]
    fn test_load_command_with_frontmatter() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("advanced-command.md");

        let content = r#"---
name: "Advanced Command"
description: "A command with frontmatter"
allowed_tools: ["fs_read", "execute_bash"]
thinking_mode: true
timeout: 60
parameters:
  - name: "action"
    type: "string"
    required: true
    options: ["start", "stop", "restart"]
---

# Advanced Command

This command has frontmatter configuration.

Action: {{action}}"#;

        fs::write(&file_path, content).unwrap();

        let command = CustomCommand::from_file(file_path).unwrap();

        assert_eq!(command.name, "advanced-command");
        assert!(command.content.starts_with("# Advanced Command"));
        assert_eq!(command.frontmatter.name, Some("Advanced Command".to_string()));
        assert_eq!(command.frontmatter.allowed_tools, vec!["fs_read", "execute_bash"]);
        assert!(command.thinking_mode_enabled());
        assert_eq!(command.timeout_seconds(), Some(60));
        assert_eq!(command.parameters().len(), 1);
        assert_eq!(command.parameters()[0].name, "action");
    }

    #[test]
    fn test_command_name_validation() {
        assert!(CustomCommand::validate_name("valid-name").is_ok());
        assert!(CustomCommand::validate_name("valid_name").is_ok());
        assert!(CustomCommand::validate_name("validname123").is_ok());

        assert!(CustomCommand::validate_name("").is_err());
        assert!(CustomCommand::validate_name("invalid name").is_err());
        assert!(CustomCommand::validate_name("invalid@name").is_err());
        assert!(CustomCommand::validate_name(&"a".repeat(51)).is_err());
    }
}
