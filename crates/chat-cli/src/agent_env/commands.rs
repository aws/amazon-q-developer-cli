//! Command system for AgentEnvironment
//!
//! This module defines the command types used for communication between UIs and AgentEnvironment.
//! Commands are parsed from user input and routed to appropriate handlers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Commands that AgentEnvironment handles (task-spawning operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEnvironmentCommand {
    /// Send a prompt to a worker
    Prompt { worker_id: Uuid, text: String },
    /// Compact conversation history for a worker
    Compact {
        worker_id: Uuid,
        instruction: Option<String>,
    },
    /// Quit the application
    Quit,
}

/// Commands that UI handles internally (display-only operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiCommand {
    /// Display token usage statistics
    Usage,
    /// Display context information
    Context,
    /// Display worker status
    Status,
    /// List all workers
    Workers,
    // UI-specific commands can be added by implementations
}

/// Parsed command from user input
#[derive(Debug, Clone)]
pub enum Command {
    /// Forward to AgentEnvironment for execution
    Agent(AgentEnvironmentCommand),
    /// Handle in UI
    Ui(UiCommand),
}

/// Result from UI prompt
#[derive(Debug, Clone)]
pub enum PromptResult {
    /// Pass command to AgentEnvironment
    Command(AgentEnvironmentCommand),
    /// Shutdown requested
    Shutdown,
}

/// Error type for command parsing
#[derive(Debug, Clone)]
pub enum ParseError {
    /// Unknown command name
    UnknownCommand(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownCommand(cmd) => write!(f, "Unknown command: {}", cmd),
        }
    }
}

impl std::error::Error for ParseError {}

/// Command parser for user input
pub struct CommandParser;

impl CommandParser {
    /// Parse user input into a Command
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Explicit commands
    /// let cmd = CommandParser::parse("/quit")?;
    /// let cmd = CommandParser::parse("/compact summarize briefly")?;
    /// let cmd = CommandParser::parse("/usage")?;
    ///
    /// // Implicit prompt command
    /// let cmd = CommandParser::parse("Hello, world!")?;
    /// ```
    pub fn parse(input: &str) -> Result<Command, ParseError> {
        let trimmed = input.trim();

        if trimmed.starts_with('/') {
            // Explicit command
            let parts: Vec<&str> = trimmed[1..].splitn(2, ' ').collect();
            match parts[0] {
                "quit" | "q" => Ok(Command::Agent(AgentEnvironmentCommand::Quit)),
                "compact" => {
                    let instruction = parts.get(1).map(|s| s.to_string());
                    Ok(Command::Agent(AgentEnvironmentCommand::Compact {
                        worker_id: Uuid::nil(), // Will be filled by UI
                        instruction,
                    }))
                }
                "usage" => Ok(Command::Ui(UiCommand::Usage)),
                "context" => Ok(Command::Ui(UiCommand::Context)),
                "status" => Ok(Command::Ui(UiCommand::Status)),
                "workers" => Ok(Command::Ui(UiCommand::Workers)),
                _ => Err(ParseError::UnknownCommand(parts[0].to_string())),
            }
        } else {
            // Implicit /prompt command
            Ok(Command::Agent(AgentEnvironmentCommand::Prompt {
                worker_id: Uuid::nil(), // Will be filled by UI
                text: trimmed.to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_quit_command() {
        let cmd = CommandParser::parse("/quit").unwrap();
        assert!(matches!(
            cmd,
            Command::Agent(AgentEnvironmentCommand::Quit)
        ));

        let cmd = CommandParser::parse("/q").unwrap();
        assert!(matches!(
            cmd,
            Command::Agent(AgentEnvironmentCommand::Quit)
        ));
    }

    #[test]
    fn test_parse_compact_command() {
        let cmd = CommandParser::parse("/compact").unwrap();
        match cmd {
            Command::Agent(AgentEnvironmentCommand::Compact {
                worker_id,
                instruction,
            }) => {
                assert_eq!(worker_id, Uuid::nil());
                assert_eq!(instruction, None);
            }
            _ => panic!("Expected Compact command"),
        }
    }

    #[test]
    fn test_parse_compact_with_instruction() {
        let cmd = CommandParser::parse("/compact summarize briefly").unwrap();
        match cmd {
            Command::Agent(AgentEnvironmentCommand::Compact {
                worker_id,
                instruction,
            }) => {
                assert_eq!(worker_id, Uuid::nil());
                assert_eq!(instruction, Some("summarize briefly".to_string()));
            }
            _ => panic!("Expected Compact command"),
        }
    }

    #[test]
    fn test_parse_ui_commands() {
        let cmd = CommandParser::parse("/usage").unwrap();
        assert!(matches!(cmd, Command::Ui(UiCommand::Usage)));

        let cmd = CommandParser::parse("/context").unwrap();
        assert!(matches!(cmd, Command::Ui(UiCommand::Context)));

        let cmd = CommandParser::parse("/status").unwrap();
        assert!(matches!(cmd, Command::Ui(UiCommand::Status)));

        let cmd = CommandParser::parse("/workers").unwrap();
        assert!(matches!(cmd, Command::Ui(UiCommand::Workers)));
    }

    #[test]
    fn test_parse_implicit_prompt() {
        let cmd = CommandParser::parse("Hello, world!").unwrap();
        match cmd {
            Command::Agent(AgentEnvironmentCommand::Prompt { worker_id, text }) => {
                assert_eq!(worker_id, Uuid::nil());
                assert_eq!(text, "Hello, world!");
            }
            _ => panic!("Expected Prompt command"),
        }
    }

    #[test]
    fn test_parse_unknown_command() {
        let result = CommandParser::parse("/unknown");
        assert!(matches!(result, Err(ParseError::UnknownCommand(_))));
    }

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::UnknownCommand("test".to_string());
        assert_eq!(format!("{}", err), "Unknown command: test");
    }
}
