use clap::Subcommand;
use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::util::command_manager::CommandManager;
use crate::util::command_types::{
    CommandScope,
    CustomCommand,
};

/// Custom commands management
#[derive(Clone, Debug, PartialEq, Eq, Subcommand)]
pub enum CommandsSubcommand {
    /// Add a new custom command
    Add { name: String },
    /// Show available commands
    Show {
        /// Filter by scope (project or global)
        #[arg(long, value_enum)]
        scope: Option<CommandScope>,
        /// Show detailed information
        #[arg(long)]
        expand: bool,
        /// Show specific command details
        name: Option<String>,
    },
    /// Remove a custom command
    Remove {
        /// Name of the command to remove
        name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
    /// Update an existing custom command
    Update {
        /// Name of the command to update
        name: String,
    },
    /// Clear all custom commands
    Clear {
        /// Filter by scope (project or global)
        #[arg(long, value_enum)]
        scope: Option<CommandScope>,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

impl CommandsSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        if !CommandManager::is_enabled(os) {
            Self::write_feature_disabled_message(session)?;
            return Ok(Self::default_chat_state());
        }

        let result = self.execute_operation(os, session).await;
        Self::write_operation_result(session, result)?;
        Ok(Self::default_chat_state())
    }

    fn write_feature_disabled_message(session: &mut ChatSession) -> Result<(), std::io::Error> {
        queue!(
            session.stderr,
            style::SetForegroundColor(Color::Red),
            style::Print("\n❌ Commands tool is disabled. Enable it with: q settings chat.enableCommands true\n\n"),
            style::SetForegroundColor(Color::Reset)
        )
    }

    fn default_chat_state() -> ChatState {
        ChatState::PromptUser {
            skip_printing_tools: true,
        }
    }

    async fn execute_operation(&self, os: &Os, _session: &mut ChatSession) -> OperationResult {
        match self {
            CommandsSubcommand::Add { name } => Self::handle_add(os, name).await,
            CommandsSubcommand::Show { scope, expand, name } => {
                Self::handle_show(os, scope.clone(), *expand, name.clone()).await
            },
            CommandsSubcommand::Remove { name, force } => Self::handle_remove(os, name, *force).await,
            CommandsSubcommand::Update { name } => Self::handle_update(os, name).await,
            CommandsSubcommand::Clear { scope, force } => Self::handle_clear(os, scope.clone(), *force).await,
        }
    }

    async fn handle_show(os: &Os, scope: Option<CommandScope>, expand: bool, name: Option<String>) -> OperationResult {
        let mut manager = match CommandManager::new(os) {
            Ok(manager) => manager,
            Err(e) => return OperationResult::Error(format!("Failed to initialize command manager: {}", e)),
        };

        // If a specific command name is provided, show details for that command
        if let Some(command_name) = name {
            return Self::show_command_details(&mut manager, &command_name).await;
        }

        // Otherwise, list all commands
        match manager.list_commands_detailed(scope.as_ref()) {
            Ok(commands) => {
                if commands.is_empty() {
                    let scope_text = match scope {
                        Some(CommandScope::Project) => " project",
                        Some(CommandScope::Global) => " global",
                        None => "",
                    };
                    OperationResult::Success(format!(
                        "No{} commands found. Use '/commands add <name>' to create one.",
                        scope_text
                    ))
                } else {
                    Self::format_commands_list(commands, expand)
                }
            },
            Err(e) => OperationResult::Error(format!("Failed to list commands: {}", e)),
        }
    }

    async fn show_command_details(manager: &mut CommandManager, name: &str) -> OperationResult {
        match manager.get_command(name) {
            Ok(command) => {
                let mut output = String::new();
                output.push_str(&format!("📄 Command: {}\n", command.name));
                output.push_str(&format!("📁 Scope: {}\n", match command.scope {
                    CommandScope::Project => "Project",
                    CommandScope::Global => "Global",
                }));
                output.push_str(&format!("📍 Path: {}\n", command.file_path.display()));

                if let Some(description) = &command.frontmatter.description {
                    output.push_str(&format!("📝 Description: {}\n", description));
                }

                if !command.frontmatter.parameters.is_empty() {
                    output.push_str("\n🔧 Parameters:\n");
                    for param in &command.frontmatter.parameters {
                        let description = param.description.as_deref().unwrap_or("No description");
                        output.push_str(&format!("  • {}: {}\n", param.name, description));
                    }
                }

                if !command.frontmatter.allowed_tools.is_empty() {
                    output.push_str("\n🛠️  Allowed Tools:\n");
                    for tool in &command.frontmatter.allowed_tools {
                        output.push_str(&format!("  • {}\n", tool));
                    }
                }

                output.push_str(&format!(
                    "\n💡 Usage: /{}:{}\n",
                    match command.scope {
                        CommandScope::Project => "project",
                        CommandScope::Global => "user",
                    },
                    command.name
                ));

                OperationResult::Success(output)
            },
            Err(e) => OperationResult::Error(format!("Command '{}' not found: {}", name, e)),
        }
    }

    fn format_commands_list(commands: Vec<CustomCommand>, expand: bool) -> OperationResult {
        let mut output = String::new();
        output.push_str("📁 Available Commands:\n\n");

        for command in commands {
            let scope_icon = match command.scope {
                CommandScope::Project => "📁",
                CommandScope::Global => "🌍",
            };

            let scope_text = match command.scope {
                CommandScope::Project => "project",
                CommandScope::Global => "user",
            };

            output.push_str(&format!(
                "  {} {} (/{scope_text}:{})\n",
                scope_icon, command.name, command.name
            ));

            if expand {
                if let Some(description) = &command.frontmatter.description {
                    output.push_str(&format!("     📝 {}\n", description));
                }
                if !command.frontmatter.parameters.is_empty() {
                    output.push_str(&format!(
                        "     🔧 {} parameter(s)\n",
                        command.frontmatter.parameters.len()
                    ));
                }
                output.push('\n');
            }
        }

        if !expand {
            output.push_str("\n💡 Use '--expand' to see more details or specify a command name for full details.\n");
        }

        OperationResult::Success(output)
    }

    async fn handle_add(os: &Os, name: &str) -> OperationResult {
        let mut manager = match CommandManager::new(os) {
            Ok(manager) => manager,
            Err(e) => return OperationResult::Error(format!("Failed to initialize command manager: {}", e)),
        };

        match manager.add_command(name, os) {
            Ok(message) => OperationResult::Success(message),
            Err(e) => OperationResult::Error(format!("Failed to add command: {}", e)),
        }
    }

    async fn handle_remove(os: &Os, name: &str, force: bool) -> OperationResult {
        let mut manager = match CommandManager::new(os) {
            Ok(manager) => manager,
            Err(e) => return OperationResult::Error(format!("Failed to initialize command manager: {}", e)),
        };

        // Check if command exists
        let command = match manager.get_command(name) {
            Ok(command) => command.clone(),
            Err(_) => return OperationResult::Error(format!("Command '{}' not found", name)),
        };

        // Confirmation prompt unless force is used
        if !force {
            use std::io::{
                self,
                Write,
            };
            print!("Are you sure you want to remove command '{}'? [y/N]: ", name);
            io::stdout().flush().unwrap();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                return OperationResult::Error("Failed to read confirmation".to_string());
            }

            let input = input.trim().to_lowercase();
            if input != "y" && input != "yes" {
                return OperationResult::Error("Command removal cancelled.".to_string());
            }
        }

        // Remove the file
        match std::fs::remove_file(&command.file_path) {
            Ok(_) => OperationResult::Success(format!("✅ Command '{}' removed successfully.", name)),
            Err(e) => OperationResult::Error(format!("Failed to remove command file: {}", e)),
        }
    }

    async fn handle_update(os: &Os, name: &str) -> OperationResult {
        let mut manager = match CommandManager::new(os) {
            Ok(manager) => manager,
            Err(e) => return OperationResult::Error(format!("Failed to initialize command manager: {}", e)),
        };

        // Check if command exists
        let command = match manager.get_command(name) {
            Ok(command) => command.clone(),
            Err(_) => return OperationResult::Error(format!("Command '{}' not found", name)),
        };

        // Open editor
        match CommandManager::open_editor(&command.file_path) {
            Ok(_) => OperationResult::Success(format!("✅ Command '{}' updated successfully.", name)),
            Err(e) => OperationResult::Error(format!("Failed to open editor: {}", e)),
        }
    }

    async fn handle_clear(os: &Os, scope: Option<CommandScope>, force: bool) -> OperationResult {
        let mut manager = match CommandManager::new(os) {
            Ok(manager) => manager,
            Err(e) => return OperationResult::Error(format!("Failed to initialize command manager: {}", e)),
        };

        // Get list of commands to be removed
        let commands = match manager.list_commands_detailed(scope.as_ref()) {
            Ok(commands) => commands,
            Err(e) => return OperationResult::Error(format!("Failed to list commands: {}", e)),
        };

        if commands.is_empty() {
            let scope_text = match scope {
                Some(CommandScope::Project) => " project",
                Some(CommandScope::Global) => " global",
                None => "",
            };
            return OperationResult::Success(format!("No{} commands to clear.", scope_text));
        }

        // Confirmation prompt unless force is used
        if !force {
            use std::io::{
                self,
                Write,
            };
            let scope_text = match scope {
                Some(CommandScope::Project) => " project",
                Some(CommandScope::Global) => " global",
                None => "",
            };
            print!(
                "Are you sure you want to remove all{} commands ({} commands)? [y/N]: ",
                scope_text,
                commands.len()
            );
            io::stdout().flush().unwrap();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                return OperationResult::Error("Failed to read confirmation".to_string());
            }

            let input = input.trim().to_lowercase();
            if input != "y" && input != "yes" {
                return OperationResult::Success("Command clearing cancelled.".to_string());
            }
        }

        // Remove all command files
        let mut removed_count = 0;
        let mut errors = Vec::new();

        for command in commands {
            match std::fs::remove_file(&command.file_path) {
                Ok(_) => removed_count += 1,
                Err(e) => errors.push(format!("Failed to remove '{}': {}", command.name, e)),
            }
        }

        if errors.is_empty() {
            OperationResult::Success(format!("✅ Successfully removed {} commands.", removed_count))
        } else {
            let mut message = format!(
                "⚠️  Removed {} commands, but encountered {} errors:\n",
                removed_count,
                errors.len()
            );
            for error in errors {
                message.push_str(&format!("  • {}\n", error));
            }
            OperationResult::Error(message)
        }
    }

    fn write_operation_result(session: &mut ChatSession, result: OperationResult) -> Result<(), std::io::Error> {
        match result {
            OperationResult::Success(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("\n{}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
            OperationResult::Error(msg) => {
                queue!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("\n❌ Error: {}\n\n", msg)),
                    style::SetForegroundColor(Color::Reset)
                )
            },
        }
    }
}

#[derive(Debug)]
enum OperationResult {
    Success(String),
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::command_types::CommandScope;

    #[test]
    fn test_commands_add_basic() {
        let subcommand = CommandsSubcommand::Add {
            name: "test-command".to_string(),
        };

        // Test that the command structure is correct
        match subcommand {
            CommandsSubcommand::Add { name } => {
                assert_eq!(name, "test-command");
                assert!(crate::util::command_types::CustomCommand::validate_name(&name).is_ok());
            },
            _ => panic!("Expected Add subcommand"),
        }
    }

    #[test]
    fn test_commands_add_invalid_name() {
        let subcommand = CommandsSubcommand::Add {
            name: "invalid name with spaces".to_string(),
        };

        match subcommand {
            CommandsSubcommand::Add { name } => {
                assert!(crate::util::command_types::CustomCommand::validate_name(&name).is_err());
            },
            _ => panic!("Expected Add subcommand"),
        }
    }

    #[test]
    fn test_commands_show_all() {
        let subcommand = CommandsSubcommand::Show {
            scope: None,
            expand: false,
            name: None,
        };

        match subcommand {
            CommandsSubcommand::Show { scope, expand, name } => {
                assert!(scope.is_none());
                assert!(!expand);
                assert!(name.is_none());
            },
            _ => panic!("Expected Show subcommand"),
        }
    }

    #[test]
    fn test_commands_show_with_scope() {
        let subcommand = CommandsSubcommand::Show {
            scope: Some(CommandScope::Project),
            expand: true,
            name: Some("test-command".to_string()),
        };

        match subcommand {
            CommandsSubcommand::Show { scope, expand, name } => {
                assert_eq!(scope, Some(CommandScope::Project));
                assert!(expand);
                assert_eq!(name, Some("test-command".to_string()));
            },
            _ => panic!("Expected Show subcommand"),
        }
    }

    #[test]
    fn test_commands_remove_basic() {
        let subcommand = CommandsSubcommand::Remove {
            name: "test-command".to_string(),
            force: false,
        };

        match subcommand {
            CommandsSubcommand::Remove { name, force } => {
                assert_eq!(name, "test-command");
                assert!(!force);
            },
            _ => panic!("Expected Remove subcommand"),
        }
    }

    #[test]
    fn test_commands_remove_with_force() {
        let subcommand = CommandsSubcommand::Remove {
            name: "test-command".to_string(),
            force: true,
        };

        match subcommand {
            CommandsSubcommand::Remove { name, force } => {
                assert_eq!(name, "test-command");
                assert!(force);
            },
            _ => panic!("Expected Remove subcommand"),
        }
    }

    #[test]
    fn test_commands_update_basic() {
        let subcommand = CommandsSubcommand::Update {
            name: "test-command".to_string(),
        };

        match subcommand {
            CommandsSubcommand::Update { name } => {
                assert_eq!(name, "test-command");
            },
            _ => panic!("Expected Update subcommand"),
        }
    }

    #[test]
    fn test_commands_clear_all() {
        let subcommand = CommandsSubcommand::Clear {
            scope: None,
            force: false,
        };

        match subcommand {
            CommandsSubcommand::Clear { scope, force } => {
                assert!(scope.is_none());
                assert!(!force);
            },
            _ => panic!("Expected Clear subcommand"),
        }
    }

    #[test]
    fn test_commands_clear_with_scope_and_force() {
        let subcommand = CommandsSubcommand::Clear {
            scope: Some(CommandScope::Project),
            force: true,
        };

        match subcommand {
            CommandsSubcommand::Clear { scope, force } => {
                assert_eq!(scope, Some(CommandScope::Project));
                assert!(force);
            },
            _ => panic!("Expected Clear subcommand"),
        }
    }

    #[test]
    fn test_operation_result_success() {
        let result = OperationResult::Success("Command created successfully".to_string());

        match result {
            OperationResult::Success(msg) => {
                assert_eq!(msg, "Command created successfully");
            },
            OperationResult::Error(_) => panic!("Expected Success result"),
        }
    }

    #[test]
    fn test_operation_result_error() {
        let result = OperationResult::Error("Command not found".to_string());

        match result {
            OperationResult::Error(msg) => {
                assert_eq!(msg, "Command not found");
            },
            OperationResult::Success(_) => panic!("Expected Error result"),
        }
    }
}
