use std::path::PathBuf;

use chrono::{
    DateTime,
    Utc,
};

/// A custom command definition
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Name of the command (derived from filename)
    #[allow(dead_code)]
    pub name: String,
    /// Command content (Markdown format)
    pub content: String,
    /// Path to the command file
    #[allow(dead_code)]
    pub file_path: PathBuf,
    /// Creation timestamp
    #[allow(dead_code)]
    pub created_at: DateTime<Utc>,
}

/// Errors that can occur during command operations
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
    #[allow(dead_code)]
    FeatureDisabled,

    #[error("Editor error: {0}")]
    EditorError(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Other error: {0}")]
    Other(String),
}

impl CustomCommand {
    /// Create a new custom command
    pub fn new(name: String, content: String, file_path: PathBuf) -> Self {
        Self {
            name,
            content,
            file_path,
            created_at: Utc::now(),
        }
    }

    /// Load a command from a file
    pub fn from_file(file_path: PathBuf) -> Result<Self, CommandError> {
        let content = std::fs::read_to_string(&file_path)?;
        let name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| CommandError::InvalidFormat("Invalid filename".to_string()))?
            .to_string();

        Ok(Self::new(name, content, file_path))
    }

    /// Validate command name
    pub fn validate_name(name: &str) -> Result<(), CommandError> {
        if name.is_empty() {
            return Err(CommandError::InvalidName("Command name cannot be empty".to_string()));
        }

        if name.contains('/') || name.contains('\\') {
            return Err(CommandError::InvalidName(
                "Command name cannot contain path separators".to_string(),
            ));
        }

        if name.contains(' ') {
            return Err(CommandError::InvalidName(
                "Command name cannot contain spaces".to_string(),
            ));
        }

        // Check for reserved names
        if matches!(name, "help" | "add" | "remove" | "show" | "list") {
            return Err(CommandError::InvalidName("Command name is reserved".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_validate_name_valid() {
        assert!(CustomCommand::validate_name("git-helper").is_ok());
        assert!(CustomCommand::validate_name("code_review").is_ok());
        assert!(CustomCommand::validate_name("test123").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(CustomCommand::validate_name("").is_err());
        assert!(CustomCommand::validate_name("git/helper").is_err());
        assert!(CustomCommand::validate_name("git helper").is_err());
        assert!(CustomCommand::validate_name("help").is_err());
    }

    #[test]
    fn test_from_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# Test Command\n\nThis is a test command.").unwrap();

        let command = CustomCommand::from_file(temp_file.path().to_path_buf()).unwrap();
        assert!(command.content.contains("Test Command"));
        assert!(!command.name.is_empty());
    }
}
