use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};

use crate::database::settings::Setting;
use crate::os::Os;
use crate::util::bash_preprocessor::BashPreprocessor;
use crate::util::command_import_export::{
    CommandImportExport,
    ExportFormat,
};
use crate::util::command_types::{
    CommandError,
    CommandScope,
    CustomCommand,
};

/// Manages custom commands for the Amazon Q CLI
pub struct CommandManager {
    /// Path to project commands directory (.amazonq/commands/)
    project_commands_dir: PathBuf,
    /// Path to user commands directory (~/.amazonq/commands/)
    user_commands_dir: PathBuf,
    /// Cache of loaded commands
    cache: HashMap<String, CustomCommand>,
    /// Bash command preprocessor
    bash_preprocessor: BashPreprocessor,
}

impl CommandManager {
    /// Create a new command manager
    pub fn new(os: &Os) -> Result<Self, CommandError> {
        let project_commands_dir = os
            .env
            .current_dir()
            .map_err(CommandError::Io)?
            .join(".amazonq")
            .join("commands");

        let user_commands_dir = os
            .env
            .home()
            .ok_or_else(|| CommandError::Other("Could not determine home directory".to_string()))?
            .join(".amazonq")
            .join("commands");

        Ok(Self {
            project_commands_dir,
            user_commands_dir,
            cache: HashMap::new(),
            bash_preprocessor: BashPreprocessor::default(),
        })
    }

    /// Check if the commands feature is enabled in settings
    pub fn is_enabled(os: &Os) -> bool {
        os.database.settings.get_bool(Setting::EnabledCommands).unwrap_or(false)
    }

    /// Add a new command
    pub fn add_command(&mut self, name: &str, _os: &Os) -> Result<String, CommandError> {
        // Validate command name
        CustomCommand::validate_name(name)?;

        // Create commands directory if it doesn't exist
        std::fs::create_dir_all(&self.project_commands_dir)?;

        let file_path = self.project_commands_dir.join(format!("{}.md", name));

        // Check if command already exists
        if file_path.exists() {
            return Err(CommandError::AlreadyExists(name.to_string()));
        }

        // Create template content
        let template = Self::create_command_template(name);

        // Write template to file
        std::fs::write(&file_path, template)?;

        // Open editor
        Self::open_editor(&file_path)?;

        // Load the command into cache
        let command = CustomCommand::from_file(file_path)?;
        self.cache.insert(name.to_string(), command);

        Ok(format!(
            "✅ Command '{}' created successfully!\n   Use '/project:{}' to execute it.\n\n💡 Tip: Use '/commands show {}' to see command details.",
            name, name, name
        ))
    }

    /// Get a command by name
    pub fn get_command(&mut self, name: &str) -> Result<&CustomCommand, CommandError> {
        // Check cache first
        if !self.cache.contains_key(name) {
            // Try to load from file
            let file_path = self.project_commands_dir.join(format!("{}.md", name));
            if file_path.exists() {
                let command = CustomCommand::from_file(file_path)?;
                self.cache.insert(name.to_string(), command);
            } else {
                return Err(CommandError::NotFound(name.to_string()));
            }
        }

        // This should never fail since we just inserted the key above
        self.cache
            .get(name)
            .ok_or_else(|| CommandError::Other("Failed to retrieve cached command".to_string()))
    }

    /// Execute a command by name
    pub fn execute_command(&mut self, name: &str) -> Result<String, CommandError> {
        let command = self.get_command(name)?;
        Ok(command.content.clone())
    }

    /// Execute command with argument processing and file references
    pub fn execute_command_with_args(
        &mut self,
        name: &str,
        args: Option<&str>,
        os: &Os,
    ) -> Result<String, CommandError> {
        let command = self.get_command(name)?;
        let mut content = command.content.clone();
        let frontmatter = command.frontmatter.clone();

        // Process argument substitution
        if let Some(args) = args {
            content = content.replace("$ARGUMENTS", args);
        } else {
            content = content.replace("$ARGUMENTS", "");
        }

        // Process file references
        content = Self::process_file_references(content, os)?;

        // Process bash commands (NEW)
        content = self
            .bash_preprocessor
            .process_bash_commands(&content, Some(&frontmatter))?;

        // Basic security validation
        Self::validate_command_security(&content)?;

        Ok(content)
    }

    /// Execute user command with argument processing and file references
    pub fn execute_user_command_with_args(
        &mut self,
        name: &str,
        args: Option<&str>,
        os: &Os,
    ) -> Result<String, CommandError> {
        let command = self.get_user_command(name)?;
        let mut content = command.content.clone();
        let frontmatter = command.frontmatter.clone();

        // Process argument substitution
        if let Some(args) = args {
            content = content.replace("$ARGUMENTS", args);
        } else {
            content = content.replace("$ARGUMENTS", "");
        }

        // Process file references
        content = Self::process_file_references(content, os)?;

        // Process bash commands (NEW)
        content = self
            .bash_preprocessor
            .process_bash_commands(&content, Some(&frontmatter))?;

        // Basic security validation
        Self::validate_command_security(&content)?;

        Ok(content)
    }

    /// Get a user command by name
    fn get_user_command(&mut self, name: &str) -> Result<&CustomCommand, CommandError> {
        let cache_key = format!("user:{}", name);

        if !self.cache.contains_key(&cache_key) {
            let command = self.load_user_command(name)?;
            self.cache.insert(cache_key.clone(), command);
        }

        self.cache
            .get(&cache_key)
            .ok_or_else(|| CommandError::NotFound(name.to_string()))
    }

    /// Load a user command from the file system
    fn load_user_command(&self, name: &str) -> Result<CustomCommand, CommandError> {
        // Handle both namespace/name format and simple name format
        let file_path = self.user_commands_dir.join(format!("{}.md", name));

        if !file_path.exists() {
            return Err(CommandError::NotFound(name.to_string()));
        }

        CustomCommand::from_file(file_path)
    }

    /// Process file references in command content
    fn process_file_references(content: String, _os: &Os) -> Result<String, CommandError> {
        use regex::Regex;

        let file_ref_regex =
            Regex::new(r"@([^\s]+)").map_err(|e| CommandError::Other(format!("Regex error: {}", e)))?;
        let mut result = content.clone();

        for captures in file_ref_regex.captures_iter(&content) {
            let file_path = &captures[1];
            let full_match = &captures[0];

            // Try to read the file
            if let Ok(file_content) = std::fs::read_to_string(file_path) {
                result = result.replace(full_match, &file_content);
            }
            // Leave the reference as-is if file can't be read
            // This allows for graceful degradation
        }

        Ok(result)
    }

    /// Basic security validation for command content
    pub fn validate_command_security(content: &str) -> Result<(), CommandError> {
        // List of potentially dangerous patterns
        let dangerous_patterns = [
            "rm -rf",
            "sudo ",
            "chmod 777",
            "chmod +x",
            "../../../", // Path traversal
            "curl -s",   // Potential network access
            "wget ",
            "nc ", // netcat
        ];

        for pattern in &dangerous_patterns {
            if content.contains(pattern) {
                return Err(CommandError::SecurityViolation(format!(
                    "Command contains potentially dangerous pattern: '{}'",
                    pattern
                )));
            }
        }

        Ok(())
    }

    /// List all available commands with full details
    pub fn list_commands_detailed(&mut self, scope: Option<&CommandScope>) -> Result<Vec<CustomCommand>, CommandError> {
        let mut commands = Vec::new();

        // Load project commands if requested or no scope specified
        if (scope.is_none() || scope == Some(&CommandScope::Project)) && self.project_commands_dir.exists() {
            for entry in std::fs::read_dir(&self.project_commands_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(mut command) = CustomCommand::from_file(path) {
                        command.scope = CommandScope::Project;
                        commands.push(command);
                    }
                }
            }
        }

        // Load user/global commands if requested or no scope specified
        if (scope.is_none() || scope == Some(&CommandScope::Global)) && self.user_commands_dir.exists() {
            for entry in std::fs::read_dir(&self.user_commands_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(mut command) = CustomCommand::from_file(path) {
                        command.scope = CommandScope::Global;
                        commands.push(command);
                    }
                }
            }
        }

        commands.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(commands)
    }

    /// Clear the command cache
    #[allow(dead_code)]
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// List all available commands
    #[allow(dead_code)]
    pub fn list_commands(&mut self) -> Result<Vec<String>, CommandError> {
        let mut commands = Vec::new();

        if !self.project_commands_dir.exists() {
            return Ok(commands);
        }

        for entry in std::fs::read_dir(&self.project_commands_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    commands.push(name.to_string());
                }
            }
        }

        commands.sort();
        Ok(commands)
    }

    /// Create a command template
    fn create_command_template(name: &str) -> String {
        let display_name = name.replace(['-', '_'], " ");
        format!(
            "# {}\n\n\
            Brief description of what this command does.\n\n\
            ## Instructions\n\n\
            Provide detailed instructions for Amazon Q:\n\n\
            1. Step 1: What to analyze first\n\
            2. Step 2: What to look for\n\
            3. Step 3: How to format the response\n\n\
            ## Context\n\n\
            Any additional context or requirements for this command.\n\n\
            ## Examples\n\n\
            Provide examples of how this command should be used or what output is expected.\n",
            display_name
        )
    }

    /// Open editor for command file
    pub fn open_editor(file_path: &PathBuf) -> Result<(), CommandError> {
        // Get editor from environment, fallback to sensible defaults
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| {
                // Platform-specific defaults
                if cfg!(target_os = "windows") {
                    "notepad".to_string()
                } else if cfg!(target_os = "macos") {
                    "open".to_string()
                } else {
                    "nano".to_string()
                }
            });

        let mut cmd = std::process::Command::new(&editor);

        // Special handling for macOS 'open' command
        if editor == "open" {
            cmd.arg("-t"); // Open in text editor
        }

        cmd.arg(file_path);

        let status = cmd
            .status()
            .map_err(|e| CommandError::EditorError(format!("Failed to start editor '{}': {}", editor, e)))?;

        if !status.success() {
            return Err(CommandError::EditorError(format!(
                "Editor '{}' exited with error",
                editor
            )));
        }

        Ok(())
    }

    /// Export a command to a file
    #[allow(dead_code)]
    pub fn export_command(
        &mut self,
        name: &str,
        output_path: &Path,
        format: ExportFormat,
    ) -> Result<String, CommandError> {
        let command = self.get_command(name)?;
        CommandImportExport::export_command(command, output_path, format)?;

        Ok(format!(
            "✅ Command '{}' exported to {} (format: {:?})",
            name,
            output_path.display(),
            format
        ))
    }

    /// Export all project commands to a directory
    #[allow(dead_code)]
    pub fn export_all_commands(&mut self, output_dir: &Path, format: ExportFormat) -> Result<String, CommandError> {
        let command_names = self.list_commands()?;
        let mut commands = HashMap::new();

        for name in &command_names {
            let command = self.get_command(name)?;
            commands.insert(name.clone(), command.clone());
        }

        CommandImportExport::export_commands(&commands, output_dir, format)?;

        Ok(format!(
            "✅ Exported {} commands to {} (format: {:?})",
            commands.len(),
            output_dir.display(),
            format
        ))
    }

    /// Import a command from a file
    #[allow(dead_code)]
    pub fn import_command(&mut self, file_path: &Path, force: bool) -> Result<String, CommandError> {
        let (name, command) = CommandImportExport::import_command(file_path)?;

        // Validate the imported command
        CommandImportExport::validate_command(&command)?;

        // Check if command already exists
        let command_file = self.project_commands_dir.join(format!("{}.md", name));
        if command_file.exists() && !force {
            return Err(CommandError::AlreadyExists(format!(
                "Command '{}' already exists. Use --force to overwrite.",
                name
            )));
        }

        // Ensure commands directory exists
        std::fs::create_dir_all(&self.project_commands_dir)?;

        // Write the command as markdown (native format)
        CommandImportExport::export_command(&command, &command_file, ExportFormat::Markdown)?;

        // Update cache
        self.cache.insert(name.clone(), command);

        Ok(format!(
            "✅ Command '{}' imported successfully!\n   Use '/project:{}' to execute it.",
            name, name
        ))
    }

    /// Import multiple commands from a directory
    #[allow(dead_code)]
    pub fn import_commands(&mut self, import_dir: &Path, force: bool) -> Result<String, CommandError> {
        let commands = CommandImportExport::import_commands(import_dir)?;

        if commands.is_empty() {
            return Ok("No valid commands found to import.".to_string());
        }

        let mut imported_count = 0;
        let mut skipped_count = 0;
        let mut errors = Vec::new();

        // Ensure commands directory exists
        std::fs::create_dir_all(&self.project_commands_dir)?;

        for (name, command) in commands {
            // Validate the command
            if let Err(e) = CommandImportExport::validate_command(&command) {
                errors.push(format!("Command '{}': {}", name, e));
                continue;
            }

            // Check if command already exists
            let command_file = self.project_commands_dir.join(format!("{}.md", name));
            if command_file.exists() && !force {
                skipped_count += 1;
                continue;
            }

            // Import the command
            match CommandImportExport::export_command(&command, &command_file, ExportFormat::Markdown) {
                Ok(_) => {
                    self.cache.insert(name, command);
                    imported_count += 1;
                },
                Err(e) => {
                    errors.push(format!("Failed to import '{}': {}", name, e));
                },
            }
        }

        let mut result = format!("✅ Import completed: {} commands imported", imported_count);

        if skipped_count > 0 {
            result.push_str(&format!(", {} skipped (already exist)", skipped_count));
        }

        if !errors.is_empty() {
            result.push_str(&format!("\n⚠️  Errors:\n{}", errors.join("\n")));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_command_name() {
        assert!(CustomCommand::validate_name("valid-name").is_ok());
        assert!(CustomCommand::validate_name("invalid name").is_err());
        assert!(CustomCommand::validate_name("").is_err());
        assert!(CustomCommand::validate_name("help").is_err());
    }

    // Phase 2 functionality tests
    #[test]
    fn test_argument_substitution() {
        let content = "Help with: $ARGUMENTS and more $ARGUMENTS";
        let args = "git commits";

        let processed = content.replace("$ARGUMENTS", args);
        assert_eq!(processed, "Help with: git commits and more git commits");

        // Test with no arguments
        let processed_empty = content.replace("$ARGUMENTS", "");
        assert_eq!(processed_empty, "Help with:  and more ");
    }

    #[test]
    fn test_security_validation() {
        let _manager = CommandManager {
            project_commands_dir: PathBuf::new(),
            user_commands_dir: PathBuf::new(),
            cache: HashMap::new(),
            bash_preprocessor: BashPreprocessor::default(),
        };

        // Test dangerous patterns
        let dangerous_patterns = [
            "rm -rf /tmp",
            "sudo apt install",
            "chmod 777 file",
            "chmod +x script",
            "../../../etc/passwd",
            "curl -s malicious.com",
            "wget http://bad.com",
            "nc -l 1234",
        ];

        for pattern in &dangerous_patterns {
            let content = format!("Execute: {}", pattern);
            assert!(
                CommandManager::validate_command_security(&content).is_err(),
                "Pattern '{}' should be detected as dangerous",
                pattern
            );
        }

        // Test safe content
        let safe_content = "Please help me with git commands and best practices";
        assert!(CommandManager::validate_command_security(safe_content).is_ok());
    }

    #[test]
    fn test_file_reference_regex() {
        use regex::Regex;

        let file_ref_regex = Regex::new(r"@([^\s]+)").unwrap();
        let content = "Read @file1.txt and @file2.md for context";

        let matches: Vec<&str> = file_ref_regex
            .captures_iter(content)
            .map(|cap| cap.get(1).unwrap().as_str())
            .collect();

        assert_eq!(matches, vec!["file1.txt", "file2.md"]);

        // Test single file reference
        let single_content = "Analyze @README.md";
        let single_matches: Vec<&str> = file_ref_regex
            .captures_iter(single_content)
            .map(|cap| cap.get(1).unwrap().as_str())
            .collect();

        assert_eq!(single_matches, vec!["README.md"]);
    }

    #[test]
    fn test_command_name_parsing() {
        // Test namespace parsing for user commands
        let name_with_namespace = "frontend/component";
        assert!(name_with_namespace.contains('/'));

        let simple_name = "git-helper";
        assert!(!simple_name.contains('/'));

        // Test cache key generation
        let cache_key = format!("user:{}", name_with_namespace);
        assert_eq!(cache_key, "user:frontend/component");
    }

    #[test]
    fn test_user_command_cache_key() {
        let name = "test-command";
        let cache_key = format!("user:{}", name);
        assert_eq!(cache_key, "user:test-command");

        let namespaced_name = "frontend/component";
        let namespaced_cache_key = format!("user:{}", namespaced_name);
        assert_eq!(namespaced_cache_key, "user:frontend/component");
    }
}
