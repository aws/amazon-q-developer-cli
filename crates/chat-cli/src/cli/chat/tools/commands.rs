use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use serde::Deserialize;

use super::{
    InvokeOutput,
    OutputKind,
};
use crate::os::Os;
use crate::util::command_manager::CommandManager;

/// The Commands tool allows executing custom user-defined commands.
/// This feature can be enabled/disabled via settings:
/// `q settings chat.enableCommands true`
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command", rename_all = "lowercase")]
pub enum Commands {
    Execute(CommandExecute),
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandExecute {
    pub name: String,
    #[serde(default)]
    pub args: Option<String>,
}

impl Commands {
    /// Checks if the commands feature is enabled in settings
    pub fn is_enabled(os: &Os) -> bool {
        CommandManager::is_enabled(os)
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        match self {
            Commands::Execute(execute) => {
                if execute.name.is_empty() {
                    eyre::bail!("Command name cannot be empty");
                }

                // Validate command name format
                if execute.name.contains('/') || execute.name.contains('\\') {
                    eyre::bail!("Invalid command name: {}", execute.name);
                }

                Ok(())
            },
        }
    }

    pub async fn queue_description(&self, _os: &Os, updates: &mut impl Write) -> Result<()> {
        match self {
            Commands::Execute(execute) => {
                queue!(
                    updates,
                    style::Print("Executing custom command: "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(&execute.name),
                    style::ResetColor,
                )?;

                if let Some(args) = &execute.args {
                    queue!(
                        updates,
                        style::Print(" with arguments: "),
                        style::SetForegroundColor(Color::Blue),
                        style::Print(args),
                        style::ResetColor,
                    )?;
                }
            },
        }
        Ok(())
    }

    pub async fn invoke(&self, os: &Os, _updates: &mut impl Write) -> Result<InvokeOutput> {
        let mut manager =
            CommandManager::new(os).map_err(|e| eyre::eyre!("Failed to initialize command manager: {}", e))?;

        let result = match self {
            Commands::Execute(execute) => {
                match manager.execute_command(&execute.name) {
                    Ok(content) => {
                        // For MVP, we just return the command content
                        // In full implementation, this would be processed further
                        format!("Executing command '{}':\n\n{}", execute.name, content)
                    },
                    Err(e) => {
                        // Format error message consistently with knowledge base
                        match e {
                            crate::util::command_types::CommandError::NotFound(name) => {
                                format!(
                                    "❌ Command '{}' not found in project scope.\n\nUse '/commands add {}' to create it.",
                                    name, name
                                )
                            },
                            _ => format!("Failed to execute command '{}': {}", execute.name, e),
                        }
                    },
                }
            },
        };

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }
}
