use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use crate::util::command_frontmatter::CommandFrontmatter;
use crate::util::command_types::{
    CommandError,
    CustomCommand,
};

/// CRUD operations for custom commands
#[allow(dead_code)]
#[derive(Debug)]
pub struct CommandCrud {
    /// Local commands directory
    local_commands_dir: PathBuf,
    /// Global commands directory
    global_commands_dir: Option<PathBuf>,
}

/// Command creation options
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CreateCommandOptions {
    /// Command name (will be used as filename)
    pub name: String,
    /// Direct content
    pub content: Option<String>,
    /// Direct frontmatter
    pub frontmatter: Option<CommandFrontmatter>,
    /// Whether to create in global directory
    pub global: bool,
    /// Whether to overwrite existing command
    pub force: bool,
}

/// Command update options
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct UpdateCommandOptions {
    /// Command name to update
    pub name: String,
    /// New content (optional)
    pub content: Option<String>,
    /// New frontmatter (optional)
    pub frontmatter: Option<CommandFrontmatter>,
    /// Whether to update global command
    pub global: bool,
}

/// Command listing options
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ListCommandOptions {
    /// Search query
    pub search: Option<String>,
    /// Include global commands
    pub include_global: bool,
    /// Include local commands
    pub include_local: bool,
}

/// Command information for listing
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CommandInfo {
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub is_global: bool,
    pub file_path: PathBuf,
    pub has_frontmatter: bool,
}

#[allow(dead_code)]
impl CommandCrud {
    /// Create a new CommandCrud instance
    #[allow(dead_code)]
    pub fn new() -> Result<Self, CommandError> {
        let local_commands_dir = PathBuf::from(".amazonq/commands");
        let global_commands_dir = dirs::home_dir().map(|home| home.join(".amazonq").join("commands"));

        Ok(Self {
            local_commands_dir,
            global_commands_dir,
        })
    }

    /// Create a new command
    pub fn create_command(&self, options: CreateCommandOptions) -> Result<PathBuf, CommandError> {
        // Validate command name
        if options.name.is_empty() {
            return Err(CommandError::ValidationError(
                "Command name cannot be empty".to_string(),
            ));
        }

        if !Self::is_valid_command_name(&options.name) {
            return Err(CommandError::ValidationError(
                "Command name must contain only alphanumeric characters, hyphens, and underscores".to_string(),
            ));
        }

        // Determine target directory
        let target_dir = if options.global {
            self.global_commands_dir
                .as_ref()
                .ok_or_else(|| CommandError::FileError("Global commands directory not available".to_string()))?
        } else {
            &self.local_commands_dir
        };

        // Ensure directory exists
        fs::create_dir_all(target_dir)
            .map_err(|e| CommandError::FileError(format!("Failed to create commands directory: {}", e)))?;

        // Determine file path
        let file_path = target_dir.join(format!("{}.md", options.name));

        // Check if file exists
        if file_path.exists() && !options.force {
            return Err(CommandError::ValidationError(format!(
                "Command '{}' already exists. Use --force to overwrite.",
                options.name
            )));
        }

        // Generate command content
        let content = options
            .content
            .unwrap_or_else(|| format!("# {}\n\nCommand implementation goes here.", options.name));
        let frontmatter = options.frontmatter;

        // Write command file
        let file_content = if let Some(frontmatter) = frontmatter {
            let frontmatter_yaml = serde_yaml::to_string(&frontmatter)
                .map_err(|e| CommandError::SerializationError(format!("Failed to serialize frontmatter: {}", e)))?;
            format!("---\n{}---\n\n{}", frontmatter_yaml, content)
        } else {
            content
        };

        fs::write(&file_path, file_content)
            .map_err(|e| CommandError::FileError(format!("Failed to write command file: {}", e)))?;

        Ok(file_path)
    }

    /// Update an existing command
    pub fn update_command(&self, options: UpdateCommandOptions) -> Result<PathBuf, CommandError> {
        // Find existing command
        let file_path = self.find_command_file(&options.name, options.global)?;

        // Load existing command
        let existing_command = CustomCommand::from_file(file_path.clone())?;

        // Update content if provided
        if let Some(new_content) = options.content {
            // If the command has frontmatter, preserve it
            let frontmatter = &existing_command.frontmatter;
            let frontmatter_yaml = serde_yaml::to_string(frontmatter)
                .map_err(|e| CommandError::SerializationError(format!("Failed to serialize frontmatter: {}", e)))?;
            let file_content = format!("---\n{}---\n\n{}", frontmatter_yaml, new_content);
            fs::write(&file_path, file_content)
                .map_err(|e| CommandError::FileError(format!("Failed to write command file: {}", e)))?;
        }

        // Update frontmatter if provided
        if let Some(new_frontmatter) = options.frontmatter {
            let content = existing_command.content.clone();
            let frontmatter_yaml = serde_yaml::to_string(&new_frontmatter)
                .map_err(|e| CommandError::SerializationError(format!("Failed to serialize frontmatter: {}", e)))?;
            let file_content = format!("---\n{}---\n\n{}", frontmatter_yaml, content);
            fs::write(&file_path, file_content)
                .map_err(|e| CommandError::FileError(format!("Failed to write command file: {}", e)))?;
        }

        Ok(file_path)
    }

    /// Delete a command
    pub fn delete_command(&self, name: &str, global: bool) -> Result<PathBuf, CommandError> {
        let file_path = self.find_command_file(name, global)?;

        fs::remove_file(&file_path)
            .map_err(|e| CommandError::FileError(format!("Failed to delete command file: {}", e)))?;

        Ok(file_path)
    }

    /// List available commands
    pub fn list_commands(&self, options: ListCommandOptions) -> Result<Vec<CommandInfo>, CommandError> {
        let mut commands = Vec::new();

        // List local commands
        if options.include_local {
            if let Ok(local_commands) = Self::list_commands_in_dir(&self.local_commands_dir, false) {
                commands.extend(local_commands);
            }
        }

        // List global commands
        if options.include_global {
            if let Some(global_dir) = &self.global_commands_dir {
                if let Ok(global_commands) = Self::list_commands_in_dir(global_dir, true) {
                    commands.extend(global_commands);
                }
            }
        }

        // Apply filters
        if let Some(search) = &options.search {
            let search_lower = search.to_lowercase();
            commands.retain(|cmd| {
                cmd.name.to_lowercase().contains(&search_lower)
                    || cmd
                        .description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&search_lower))
                    || cmd
                        .tags
                        .as_ref()
                        .is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(&search_lower)))
            });
        }

        // Sort by name
        commands.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(commands)
    }

    /// Get command details
    pub fn get_command(&self, name: &str, global: bool) -> Result<CustomCommand, CommandError> {
        let file_path = self.find_command_file(name, global)?;
        CustomCommand::from_file(file_path)
    }

    // Helper methods

    /// Find command file path
    fn find_command_file(&self, name: &str, global: bool) -> Result<PathBuf, CommandError> {
        let target_dir = if global {
            self.global_commands_dir
                .as_ref()
                .ok_or_else(|| CommandError::FileError("Global commands directory not available".to_string()))?
        } else {
            &self.local_commands_dir
        };

        let file_path = target_dir.join(format!("{}.md", name));

        if !file_path.exists() {
            return Err(CommandError::NotFound(format!(
                "Command '{}' not found in {} commands",
                name,
                if global { "global" } else { "local" }
            )));
        }

        Ok(file_path)
    }

    /// List commands in a directory
    fn list_commands_in_dir(dir: &Path, is_global: bool) -> Result<Vec<CommandInfo>, CommandError> {
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(dir)
            .map_err(|e| CommandError::FileError(format!("Failed to read commands directory: {}", e)))?;

        let mut commands = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| CommandError::FileError(format!("Failed to read directory entry: {}", e)))?;

            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    match Self::create_command_info(name, &path, is_global) {
                        Ok(info) => commands.push(info),
                        Err(e) => {
                            eprintln!("Warning: Failed to read command {}: {}", path.display(), e);
                        },
                    }
                }
            }
        }

        Ok(commands)
    }

    /// Create CommandInfo from file
    fn create_command_info(name: &str, path: &Path, is_global: bool) -> Result<CommandInfo, CommandError> {
        let command = CustomCommand::from_file(path.to_path_buf())?;

        let description = command.frontmatter.description.clone();
        let tags = if command.frontmatter.tags.is_empty() {
            None
        } else {
            Some(command.frontmatter.tags.clone())
        };

        Ok(CommandInfo {
            name: name.to_string(),
            description,
            tags,
            is_global,
            file_path: path.to_path_buf(),
            has_frontmatter: true, // Always true since frontmatter is not optional
        })
    }

    /// Validate command name
    fn is_valid_command_name(name: &str) -> bool {
        !name.is_empty()
            && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && !name.starts_with('-')
            && !name.ends_with('-')
    }
}

impl Default for CommandCrud {
    fn default() -> Self {
        Self::new().expect("Failed to create CommandCrud")
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn create_test_crud() -> (CommandCrud, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let local_commands_dir = temp_dir.path().join("local");
        let global_commands_dir = temp_dir.path().join("global");

        fs::create_dir_all(&local_commands_dir).unwrap();
        fs::create_dir_all(&global_commands_dir).unwrap();

        let mut crud = CommandCrud::new().unwrap();
        crud.local_commands_dir = local_commands_dir;
        crud.global_commands_dir = Some(global_commands_dir);

        (crud, temp_dir)
    }

    #[test]
    fn test_command_name_validation() {
        assert!(CommandCrud::is_valid_command_name("valid-name"));
        assert!(CommandCrud::is_valid_command_name("valid_name"));
        assert!(CommandCrud::is_valid_command_name("validname123"));

        assert!(!CommandCrud::is_valid_command_name(""));
        assert!(!CommandCrud::is_valid_command_name("-invalid"));
        assert!(!CommandCrud::is_valid_command_name("invalid-"));
        assert!(!CommandCrud::is_valid_command_name("invalid name"));
        assert!(!CommandCrud::is_valid_command_name("invalid@name"));
    }

    #[test]
    fn test_create_command_basic() {
        let (crud, _temp_dir) = create_test_crud();

        let options = CreateCommandOptions {
            name: "test-command".to_string(),
            content: Some("# Test Command\n\nThis is a test.".to_string()),
            frontmatter: None,
            global: false,
            force: false,
        };

        let result = crud.create_command(options);
        assert!(result.is_ok());

        let file_path = result.unwrap();
        assert!(file_path.exists());
        assert!(file_path.file_name().unwrap().to_str().unwrap() == "test-command.md");
    }

    #[test]
    fn test_list_commands() {
        let (crud, _temp_dir) = create_test_crud();

        // Create a test command
        let options = CreateCommandOptions {
            name: "list-test".to_string(),
            content: Some("# List Test\n\nTest command for listing.".to_string()),
            frontmatter: None,
            global: false,
            force: false,
        };
        crud.create_command(options).unwrap();

        // List commands
        let list_options = ListCommandOptions {
            search: None,
            include_global: true,
            include_local: true,
        };

        let commands = crud.list_commands(list_options).unwrap();
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.name == "list-test"));
    }

    #[test]
    fn test_delete_command() {
        let (crud, _temp_dir) = create_test_crud();

        // Create a command to delete
        let options = CreateCommandOptions {
            name: "delete-test".to_string(),
            content: Some("# Delete Test\n\nTest command for deletion.".to_string()),
            frontmatter: None,
            global: false,
            force: false,
        };
        let file_path = crud.create_command(options).unwrap();
        assert!(file_path.exists());

        // Delete the command
        let result = crud.delete_command("delete-test", false);
        assert!(result.is_ok());
        assert!(!file_path.exists());
    }

    #[test]
    fn test_create_command_duplicate_without_force() {
        let (crud, _temp_dir) = create_test_crud();

        let options = CreateCommandOptions {
            name: "duplicate-test".to_string(),
            content: Some("# First Command".to_string()),
            frontmatter: None,
            global: false,
            force: false,
        };

        // First creation should succeed
        assert!(crud.create_command(options.clone()).is_ok());

        // Second creation should fail without force
        let result = crud.create_command(options);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
}
