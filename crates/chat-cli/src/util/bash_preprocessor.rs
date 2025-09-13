use std::process::Stdio;
use std::time::Duration;

use eyre::Result;
use regex::Regex;
use tokio::io::{
    AsyncBufReadExt,
    BufReader,
};
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;

use crate::util::command_frontmatter::CommandFrontmatter;
use crate::util::command_types::CommandError;

/// Preprocessor for bash commands in custom command content
pub struct BashPreprocessor {
    /// Maximum output size for bash commands
    max_output_size: usize,
    /// Timeout for bash command execution
    timeout_duration: Duration,
}

impl Default for BashPreprocessor {
    fn default() -> Self {
        Self {
            max_output_size: 4096,                     // 4KB max output per command
            timeout_duration: Duration::from_secs(30), // 30 second timeout
        }
    }
}

impl BashPreprocessor {
    #[allow(dead_code)]
    pub fn new(max_output_size: usize, timeout_seconds: u64) -> Self {
        Self {
            max_output_size,
            timeout_duration: Duration::from_secs(timeout_seconds),
        }
    }

    /// Process bash commands in content, replacing !`command` with command output
    pub fn process_bash_commands(
        &self,
        content: &str,
        frontmatter: Option<&CommandFrontmatter>,
    ) -> Result<String, CommandError> {
        // Parse bash commands from content
        let bash_commands = Self::parse_bash_commands(content)?;

        if bash_commands.is_empty() {
            return Ok(content.to_string());
        }

        // Validate permissions
        Self::validate_bash_permissions(&bash_commands, frontmatter)?;

        // Execute commands and replace in content
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| CommandError::Other(format!("Failed to create runtime: {}", e)))?;
        let mut result = content.to_string();

        for bash_cmd in bash_commands {
            let output = rt.block_on(async { self.execute_bash_command(&bash_cmd.command).await })?;

            result = result.replace(&bash_cmd.full_match, &output);
        }

        Ok(result)
    }

    /// Parse bash commands from content
    fn parse_bash_commands(content: &str) -> Result<Vec<BashCommand>, CommandError> {
        let bash_regex = Regex::new(r"!\`([^`]+)\`").map_err(|e| CommandError::Other(format!("Regex error: {}", e)))?;

        let mut commands = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            for captures in bash_regex.captures_iter(line) {
                let command = captures[1].trim().to_string();
                let full_match = captures[0].to_string();

                commands.push(BashCommand {
                    command,
                    full_match,
                    line_number: line_num + 1,
                });
            }
        }

        Ok(commands)
    }

    /// Validate that bash commands are allowed by frontmatter permissions
    fn validate_bash_permissions(
        commands: &[BashCommand],
        frontmatter: Option<&CommandFrontmatter>,
    ) -> Result<(), CommandError> {
        // Common readonly commands that are safe to execute without explicit permission
        const READONLY_COMMANDS: &[&str] = &[
            "ls", "cat", "echo", "pwd", "which", "head", "tail", "find", "grep", "dir", "type",
        ];

        // If no frontmatter, only allow readonly commands
        let default_tools = vec![];
        let allowed_tools = frontmatter.map_or(&default_tools, |fm| &fm.allowed_tools);

        let has_bash_permission = allowed_tools
            .iter()
            .any(|tool| tool == "execute_bash" || tool.starts_with("Bash("));

        for cmd in commands {
            if !has_bash_permission {
                // Only allow readonly commands if no explicit bash permission
                if !Self::is_readonly_command(&cmd.command, READONLY_COMMANDS) {
                    return Err(CommandError::SecurityViolation(format!(
                        "Bash command '{}' requires 'execute_bash' permission in frontmatter (line {})",
                        cmd.command, cmd.line_number
                    )));
                }
            } else {
                // Check specific command permissions if defined
                if let Some(specific_perms) = Self::get_specific_bash_permissions(allowed_tools) {
                    if !Self::is_command_allowed(&cmd.command, &specific_perms) {
                        return Err(CommandError::SecurityViolation(format!(
                            "Bash command '{}' not allowed by frontmatter permissions (line {})",
                            cmd.command, cmd.line_number
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if a command is readonly (safe to execute without explicit permission)
    fn is_readonly_command(command: &str, readonly_commands: &[&str]) -> bool {
        if let Some(args) = shlex::split(command) {
            if let Some(cmd) = args.first() {
                return readonly_commands.contains(&cmd.as_str());
            }
        }
        false
    }

    /// Extract specific bash permissions from allowed_tools
    fn get_specific_bash_permissions(allowed_tools: &[String]) -> Option<Vec<String>> {
        let mut permissions = Vec::new();

        for tool in allowed_tools {
            if let Some(perm) = tool.strip_prefix("Bash(").and_then(|s| s.strip_suffix(")")) {
                permissions.push(perm.to_string());
            }
        }

        if permissions.is_empty() {
            None
        } else {
            Some(permissions)
        }
    }

    /// Check if a command is allowed by specific permissions
    fn is_command_allowed(command: &str, permissions: &[String]) -> bool {
        for permission in permissions {
            if permission.ends_with(":*") {
                // Wildcard permission like "git:*"
                let prefix = permission.trim_end_matches(":*");
                if command.starts_with(prefix) {
                    return true;
                }
            } else if command == permission {
                // Exact match
                return true;
            }
        }
        false
    }

    /// Execute a bash command and return formatted output
    async fn execute_bash_command(&self, command: &str) -> Result<String, CommandError> {
        let shell = std::env::var("AMAZON_Q_CHAT_SHELL").unwrap_or_else(|_| "bash".to_string());

        let mut child = TokioCommand::new(shell)
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CommandError::Other(format!("Failed to spawn command '{}': {}", command, e)))?;

        // Set up timeout for command execution
        let output_future = async {
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();

            let stdout_reader = BufReader::new(stdout);
            let stderr_reader = BufReader::new(stderr);

            let mut stdout_lines = stdout_reader.lines();
            let mut stderr_lines = stderr_reader.lines();

            let mut stdout_output = Vec::new();
            let mut stderr_output = Vec::new();
            let mut total_size = 0;

            // Read output with size limit
            loop {
                tokio::select! {
                    line = stdout_lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                total_size += line.len();
                                if total_size > self.max_output_size {
                                    stdout_output.push("... (output truncated)".to_string());
                                    break;
                                }
                                stdout_output.push(line);
                            }
                            Ok(None) => break,
                            Err(e) => return Err(CommandError::Other(format!("Error reading stdout: {}", e))),
                        }
                    }
                    line = stderr_lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                total_size += line.len();
                                if total_size > self.max_output_size {
                                    stderr_output.push("... (output truncated)".to_string());
                                    break;
                                }
                                stderr_output.push(line);
                            }
                            Ok(None) => {},
                            Err(e) => return Err(CommandError::Other(format!("Error reading stderr: {}", e))),
                        }
                    }
                    else => break,
                }
            }

            let exit_status = child
                .wait()
                .await
                .map_err(|e| CommandError::Other(format!("Failed to wait for command: {}", e)))?;

            Ok((stdout_output, stderr_output, exit_status.code()))
        };

        let (stdout_lines, stderr_lines, exit_code) = timeout(self.timeout_duration, output_future)
            .await
            .map_err(|_timeout_err| CommandError::Other(format!("Command '{}' timed out", command)))??;

        // Format the output
        let mut formatted_output = String::new();

        if !stdout_lines.is_empty() {
            formatted_output.push_str(&stdout_lines.join("\n"));
        }

        if !stderr_lines.is_empty() {
            if !formatted_output.is_empty() {
                formatted_output.push('\n');
            }
            formatted_output.push_str(&stderr_lines.join("\n"));
        }

        // Include exit status if non-zero
        if let Some(code) = exit_code {
            if code != 0 {
                if !formatted_output.is_empty() {
                    formatted_output.push('\n');
                }
                formatted_output.push_str(&format!("(exit code: {})", code));
            }
        }

        // Trim and ensure we don't have empty output
        let formatted_output = formatted_output.trim();
        if formatted_output.is_empty() {
            Ok("(no output)".to_string())
        } else {
            Ok(formatted_output.to_string())
        }
    }
}

/// Represents a bash command found in content
#[derive(Debug, Clone)]
struct BashCommand {
    /// The actual command to execute
    command: String,
    /// The full match string (including !` and `)
    full_match: String,
    /// Line number where the command was found
    line_number: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bash_commands() {
        let _preprocessor = BashPreprocessor::default();
        let content = r#"
# Test Command

Current status: !`git status`
Current branch: !`git branch --show-current`
File count: !`ls -la | wc -l`

Some other content.
"#;

        let commands = BashPreprocessor::parse_bash_commands(content).unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].command, "git status");
        assert_eq!(commands[1].command, "git branch --show-current");
        assert_eq!(commands[2].command, "ls -la | wc -l");
    }

    #[test]
    fn test_readonly_command_validation() {
        let _preprocessor = BashPreprocessor::default();
        const READONLY_COMMANDS: &[&str] = &["ls", "cat", "echo", "pwd", "which", "head", "tail", "find", "grep"];

        assert!(BashPreprocessor::is_readonly_command("ls -la", READONLY_COMMANDS));
        assert!(BashPreprocessor::is_readonly_command("cat file.txt", READONLY_COMMANDS));
        assert!(BashPreprocessor::is_readonly_command("echo hello", READONLY_COMMANDS));
        assert!(!BashPreprocessor::is_readonly_command("rm file.txt", READONLY_COMMANDS));
        assert!(!BashPreprocessor::is_readonly_command("git commit", READONLY_COMMANDS));
    }

    #[test]
    fn test_specific_bash_permissions() {
        let _preprocessor = BashPreprocessor::default();
        let allowed_tools = vec![
            "execute_bash".to_string(),
            "Bash(git status:*)".to_string(),
            "Bash(git branch:*)".to_string(),
            "Bash(ls)".to_string(),
        ];

        let permissions = BashPreprocessor::get_specific_bash_permissions(&allowed_tools).unwrap();
        assert_eq!(permissions.len(), 3);
        assert!(permissions.contains(&"git status:*".to_string()));
        assert!(permissions.contains(&"git branch:*".to_string()));
        assert!(permissions.contains(&"ls".to_string()));
    }

    #[test]
    fn test_command_permission_matching() {
        let _preprocessor = BashPreprocessor::default();
        let permissions = vec!["git status:*".to_string(), "git branch:*".to_string(), "ls".to_string()];

        assert!(BashPreprocessor::is_command_allowed("git status", &permissions));
        assert!(BashPreprocessor::is_command_allowed("git status --short", &permissions));
        assert!(BashPreprocessor::is_command_allowed("git branch", &permissions));
        assert!(BashPreprocessor::is_command_allowed(
            "git branch --show-current",
            &permissions
        ));
        assert!(BashPreprocessor::is_command_allowed("ls", &permissions));
        assert!(!BashPreprocessor::is_command_allowed("ls -la", &permissions)); // Exact match required
        assert!(!BashPreprocessor::is_command_allowed("git commit", &permissions));
        assert!(!BashPreprocessor::is_command_allowed("rm file.txt", &permissions));
    }

    #[test]
    fn test_validate_bash_permissions_readonly() {
        let _preprocessor = BashPreprocessor::default();
        let commands = vec![
            BashCommand {
                command: "ls -la".to_string(),
                full_match: "!`ls -la`".to_string(),
                line_number: 1,
            },
            BashCommand {
                command: "cat file.txt".to_string(),
                full_match: "!`cat file.txt`".to_string(),
                line_number: 2,
            },
        ];

        // Should pass with no frontmatter (readonly commands)
        assert!(BashPreprocessor::validate_bash_permissions(&commands, None).is_ok());

        // Should fail with non-readonly command
        let dangerous_commands = vec![BashCommand {
            command: "rm file.txt".to_string(),
            full_match: "!`rm file.txt`".to_string(),
            line_number: 1,
        }];

        assert!(BashPreprocessor::validate_bash_permissions(&dangerous_commands, None).is_err());
    }

    #[test]
    fn test_validate_bash_permissions_with_frontmatter() {
        let _preprocessor = BashPreprocessor::default();
        let frontmatter = CommandFrontmatter {
            description: Some("Test command".to_string()),
            allowed_tools: vec!["Bash(git:*)".to_string()],
            timeout_seconds: None,
            max_output_size: None,
            tags: vec![],
        };

        let git_commands = vec![
            BashCommand {
                command: "git status".to_string(),
                full_match: "!`git status`".to_string(),
                line_number: 1,
            },
            BashCommand {
                command: "git branch".to_string(),
                full_match: "!`git branch`".to_string(),
                line_number: 2,
            },
        ];

        // Should pass with git permissions
        assert!(BashPreprocessor::validate_bash_permissions(&git_commands, Some(&frontmatter)).is_ok());

        // Should fail with non-git command
        let non_git_commands = vec![BashCommand {
            command: "ls -la".to_string(),
            full_match: "!`ls -la`".to_string(),
            line_number: 1,
        }];

        assert!(BashPreprocessor::validate_bash_permissions(&non_git_commands, Some(&frontmatter)).is_err());
    }
}
