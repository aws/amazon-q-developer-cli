use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};

use crate::util::command_types::{
    CommandError,
    CustomCommand,
};

/// Supported export formats
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    /// Native Markdown format
    Markdown,
    /// JSON format with metadata
    Json,
    /// YAML format with metadata
    Yaml,
}

#[allow(dead_code)]
impl ExportFormat {
    pub fn parse_format(s: &str) -> Result<Self, CommandError> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Markdown),
            "json" => Ok(Self::Json),
            "yaml" | "yml" => Ok(Self::Yaml),
            _ => Err(CommandError::Other(format!("Unsupported export format: {}", s))),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Yaml => "yaml",
        }
    }
}

/// Command export/import manager
#[allow(dead_code)]
pub struct CommandImportExport;

#[allow(dead_code)]
impl CommandImportExport {
    /// Export a single command to a file
    pub fn export_command(
        command: &CustomCommand,
        output_path: &Path,
        format: ExportFormat,
    ) -> Result<(), CommandError> {
        let content = match format {
            ExportFormat::Markdown => Self::export_as_markdown(command),
            ExportFormat::Json => Self::export_as_json(command)?,
            ExportFormat::Yaml => Self::export_as_yaml(command)?,
        };

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(output_path, content)?;
        Ok(())
    }

    /// Export multiple commands to a directory or archive
    pub fn export_commands(
        commands: &HashMap<String, CustomCommand>,
        output_path: &Path,
        format: ExportFormat,
    ) -> Result<(), CommandError> {
        // Create output directory
        fs::create_dir_all(output_path)?;

        for (name, command) in commands {
            let filename = format!("{}.{}", name, format.extension());
            let file_path = output_path.join(filename);
            Self::export_command(command, &file_path, format)?;
        }

        Ok(())
    }

    /// Import a single command from a file
    pub fn import_command(file_path: &Path) -> Result<(String, CustomCommand), CommandError> {
        let content = fs::read_to_string(file_path)?;
        let extension = file_path.extension().and_then(|ext| ext.to_str()).unwrap_or("md");

        let command = match extension {
            "md" => Self::import_from_markdown(&content)?,
            "json" => Self::import_from_json(&content)?,
            "yaml" | "yml" => Self::import_from_yaml(&content)?,
            _ => return Err(CommandError::Other(format!("Unsupported file format: {}", extension))),
        };

        // Extract command name from filename
        let name = file_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| CommandError::Other("Could not extract command name from filename".to_string()))?
            .to_string();

        Ok((name, command))
    }

    /// Import multiple commands from a directory
    pub fn import_commands(directory_path: &Path) -> Result<HashMap<String, CustomCommand>, CommandError> {
        let mut commands = HashMap::new();

        if !directory_path.is_dir() {
            return Err(CommandError::Other("Import path must be a directory".to_string()));
        }

        for entry in fs::read_dir(directory_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                match Self::import_command(&path) {
                    Ok((name, command)) => {
                        commands.insert(name, command);
                    },
                    Err(e) => {
                        // Log error but continue with other files
                        eprintln!("Warning: Failed to import {}: {}", path.display(), e);
                    },
                }
            }
        }

        Ok(commands)
    }

    /// Export command as Markdown (native format)
    fn export_as_markdown(command: &CustomCommand) -> String {
        let mut content = String::new();

        // Add frontmatter
        let frontmatter = &command.frontmatter;
        content.push_str("---\n");
        if let Some(description) = &frontmatter.description {
            content.push_str(&format!("description: {}\n", description));
        }
        if !frontmatter.allowed_tools.is_empty() {
            content.push_str("allowed_tools: [");
            for (i, tool) in frontmatter.allowed_tools.iter().enumerate() {
                if i > 0 {
                    content.push_str(", ");
                }
                content.push_str(&format!("\"{}\"", tool));
            }
            content.push_str("]\n");
        }
        content.push_str("---\n\n");

        // Add command content
        content.push_str(&command.content);

        content
    }

    /// Export command as JSON
    fn export_as_json(command: &CustomCommand) -> Result<String, CommandError> {
        let export_data = CommandExportData::from_command(command);
        serde_json::to_string_pretty(&export_data)
            .map_err(|e| CommandError::Other(format!("JSON serialization error: {}", e)))
    }

    /// Export command as YAML
    fn export_as_yaml(command: &CustomCommand) -> Result<String, CommandError> {
        let export_data = CommandExportData::from_command(command);
        serde_yaml::to_string(&export_data).map_err(|e| CommandError::Other(format!("YAML serialization error: {}", e)))
    }

    /// Import command from Markdown
    fn import_from_markdown(content: &str) -> Result<CustomCommand, CommandError> {
        // Parse YAML frontmatter if present
        let (frontmatter, markdown_content) =
            crate::util::command_frontmatter::CommandFrontmatter::parse_from_content(content)?;

        // Validate frontmatter
        frontmatter.validate()?;

        Ok(CustomCommand {
            name: "imported".to_string(), // Will be overridden by import logic
            content: markdown_content,
            file_path: std::path::PathBuf::new(), // Will be set by import logic
            created_at: chrono::Utc::now(),
            frontmatter,
            scope: crate::util::command_types::CommandScope::Project, // Default scope
        })
    }

    /// Import command from JSON
    fn import_from_json(content: &str) -> Result<CustomCommand, CommandError> {
        let export_data: CommandExportData = serde_json::from_str(content)
            .map_err(|e| CommandError::Other(format!("JSON deserialization error: {}", e)))?;
        Ok(export_data.to_command())
    }

    /// Import command from YAML
    fn import_from_yaml(content: &str) -> Result<CustomCommand, CommandError> {
        let export_data: CommandExportData = serde_yaml::from_str(content)
            .map_err(|e| CommandError::Other(format!("YAML deserialization error: {}", e)))?;
        Ok(export_data.to_command())
    }

    /// Validate imported command
    pub fn validate_command(command: &CustomCommand) -> Result<(), CommandError> {
        // Basic validation
        if command.content.trim().is_empty() {
            return Err(CommandError::Other("Command content cannot be empty".to_string()));
        }

        // Validate frontmatter
        let frontmatter = &command.frontmatter;
        // Validate allowed tools
        for tool in &frontmatter.allowed_tools {
            if tool.trim().is_empty() {
                return Err(CommandError::Other("Empty tool name in allowed-tools".to_string()));
            }
        }

        Ok(())
    }
}

/// Serializable representation of a command for export/import
#[derive(Debug, Serialize, Deserialize)]
struct CommandExportData {
    /// Command content
    content: String,
    /// Optional description
    description: Option<String>,
    /// Allowed tools
    allowed_tools: Vec<String>,
    /// Export metadata
    metadata: ExportMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportMetadata {
    /// Export format version
    version: String,
    /// Export timestamp
    exported_at: String,
    /// Exporter information
    exported_by: String,
}

#[allow(dead_code)]
impl CommandExportData {
    fn from_command(command: &CustomCommand) -> Self {
        let frontmatter = &command.frontmatter;
        let description = frontmatter.description.clone();
        let allowed_tools = frontmatter.allowed_tools.clone();

        Self {
            content: command.content.clone(),
            description,
            allowed_tools,
            metadata: ExportMetadata {
                version: "1.0".to_string(),
                exported_at: chrono::Utc::now().to_rfc3339(),
                exported_by: "Amazon Q CLI".to_string(),
            },
        }
    }

    fn to_command(&self) -> CustomCommand {
        let frontmatter = crate::util::command_frontmatter::CommandFrontmatter {
            description: self.description.clone(),
            allowed_tools: self.allowed_tools.clone(),
            ..Default::default()
        };

        CustomCommand {
            name: "imported".to_string(), // Will be overridden by import logic
            content: self.content.clone(),
            file_path: std::path::PathBuf::new(), // Will be set by import logic
            created_at: chrono::Utc::now(),
            frontmatter,
            scope: crate::util::command_types::CommandScope::Project, // Default scope
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::util::command_frontmatter::CommandFrontmatter;

    fn create_test_command() -> CustomCommand {
        let frontmatter = CommandFrontmatter {
            description: Some("A test command".to_string()),
            allowed_tools: vec!["execute_bash".to_string(), "fs_read".to_string()],
            ..Default::default()
        };

        CustomCommand {
            name: "test".to_string(),
            content: "# Test Command\n\nThis is a test command with !`echo hello` bash execution.".to_string(),
            file_path: std::path::PathBuf::from("test.md"),
            created_at: chrono::Utc::now(),
            frontmatter,
            scope: crate::util::command_types::CommandScope::Project,
        }
    }

    #[test]
    fn test_export_import_markdown() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let command = create_test_command();

        // Export
        CommandImportExport::export_command(&command, &file_path, ExportFormat::Markdown).unwrap();

        // Import
        let (name, imported_command) = CommandImportExport::import_command(&file_path).unwrap();

        assert_eq!(name, "test");
        assert_eq!(imported_command.content, command.content);
        assert_eq!(
            imported_command.frontmatter.description,
            command.frontmatter.description
        );
        assert_eq!(
            imported_command.frontmatter.allowed_tools,
            command.frontmatter.allowed_tools
        );
    }

    #[test]
    fn test_export_import_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");
        let command = create_test_command();

        // Export
        CommandImportExport::export_command(&command, &file_path, ExportFormat::Json).unwrap();

        // Import
        let (name, imported_command) = CommandImportExport::import_command(&file_path).unwrap();

        assert_eq!(name, "test");
        assert_eq!(imported_command.content, command.content);
        assert_eq!(
            imported_command.frontmatter.description,
            command.frontmatter.description
        );
        assert_eq!(
            imported_command.frontmatter.allowed_tools,
            command.frontmatter.allowed_tools
        );
    }

    #[test]
    fn test_export_import_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.yaml");
        let command = create_test_command();

        // Export
        CommandImportExport::export_command(&command, &file_path, ExportFormat::Yaml).unwrap();

        // Import
        let (name, imported_command) = CommandImportExport::import_command(&file_path).unwrap();

        assert_eq!(name, "test");
        assert_eq!(imported_command.content, command.content);
        assert_eq!(
            imported_command.frontmatter.description,
            command.frontmatter.description
        );
        assert_eq!(
            imported_command.frontmatter.allowed_tools,
            command.frontmatter.allowed_tools
        );
    }

    #[test]
    fn test_export_import_multiple_commands() {
        let temp_dir = TempDir::new().unwrap();
        let export_dir = temp_dir.path().join("export");
        let import_dir = temp_dir.path().join("import");

        // Create test commands
        let mut commands = HashMap::new();
        commands.insert("test1".to_string(), create_test_command());
        commands.insert("test2".to_string(), create_test_command());

        // Export
        CommandImportExport::export_commands(&commands, &export_dir, ExportFormat::Json).unwrap();

        // Copy exported files to import directory
        std::fs::create_dir_all(&import_dir).unwrap();
        for entry in std::fs::read_dir(&export_dir).unwrap() {
            let entry = entry.unwrap();
            let dest = import_dir.join(entry.file_name());
            std::fs::copy(entry.path(), dest).unwrap();
        }

        // Import
        let imported_commands = CommandImportExport::import_commands(&import_dir).unwrap();

        assert_eq!(imported_commands.len(), 2);
        assert!(imported_commands.contains_key("test1"));
        assert!(imported_commands.contains_key("test2"));
    }

    #[test]
    fn test_validate_command() {
        let valid_command = create_test_command();
        assert!(CommandImportExport::validate_command(&valid_command).is_ok());

        let empty_command = CustomCommand {
            name: "empty".to_string(),
            content: "".to_string(),
            file_path: std::path::PathBuf::from("empty.md"),
            created_at: chrono::Utc::now(),
            frontmatter: CommandFrontmatter::default(),
            scope: crate::util::command_types::CommandScope::Project,
        };
        assert!(CommandImportExport::validate_command(&empty_command).is_err());

        let invalid_frontmatter = CommandFrontmatter {
            description: Some("Test".to_string()),
            allowed_tools: vec!["".to_string()], // Empty tool name
            ..Default::default()
        };

        let invalid_frontmatter_command = CustomCommand {
            name: "invalid".to_string(),
            content: "Test content".to_string(),
            file_path: std::path::PathBuf::from("invalid.md"),
            created_at: chrono::Utc::now(),
            frontmatter: invalid_frontmatter,
            scope: crate::util::command_types::CommandScope::Project,
        };
        assert!(CommandImportExport::validate_command(&invalid_frontmatter_command).is_err());
    }

    #[test]
    fn test_export_format_from_str() {
        assert!(matches!(ExportFormat::parse_format("md"), Ok(ExportFormat::Markdown)));
        assert!(matches!(
            ExportFormat::parse_format("markdown"),
            Ok(ExportFormat::Markdown)
        ));
        assert!(matches!(ExportFormat::parse_format("json"), Ok(ExportFormat::Json)));
        assert!(matches!(ExportFormat::parse_format("yaml"), Ok(ExportFormat::Yaml)));
        assert!(matches!(ExportFormat::parse_format("yml"), Ok(ExportFormat::Yaml)));
        assert!(ExportFormat::parse_format("invalid").is_err());
    }
}
