use std::io::Write;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use fig_api_client::model::{
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
};
use fig_os_shim::Context;
use serde::{
    Deserialize,
    Serialize,
};

use crate::cli::chat::commands::CommandRegistry;
use crate::cli::chat::tools::use_q_command::schema::UseQCommand;
use crate::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
};

// Static flag to indicate that the application should exit after tool execution
static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

// Function to check if the application should exit
pub fn should_exit() -> bool {
    SHOULD_EXIT.load(Ordering::SeqCst)
}

// Function to reset the exit flag (useful for tests)
pub fn reset_exit_flag() {
    SHOULD_EXIT.store(false, Ordering::SeqCst);
}

/// Response from executing a Q command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseQCommandResponse {
    /// Whether the command was executed successfully
    pub success: bool,

    /// Output from the command execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Error message if the command failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl UseQCommand {
    pub fn validate(&self, _ctx: &Context) -> Result<(), ToolResult> {
        // Validate that the command is one of the known commands
        let cmd = self.command.trim_start_matches('/');
        match cmd {
            "quit" | "clear" | "help" | "context" | "profile" | "tools" | "issue" => Ok(()),
            _ => Err(ToolResult {
                tool_use_id: self.tool_use_id.clone().unwrap_or_default(),
                content: vec![ToolResultContentBlock::Text(format!(
                    "Unknown command: {}",
                    self.command
                ))],
                status: ToolResultStatus::Error,
            }),
        }
    }

    pub fn requires_acceptance(_ctx: &Context) -> bool {
        // All commands executed through use_q_command require user acceptance by default
        // This provides a security boundary between the AI and command execution
        true
    }

    /// Check if this specific command requires confirmation
    pub fn command_requires_confirmation(&self, _ctx: &Context) -> bool {
        // Get the command name without the leading slash
        let cmd = self.command.trim_start_matches('/');

        // Get the command registry
        let registry = CommandRegistry::global();

        // Check if the command exists in the registry
        if let Some(handler) = registry.get(cmd) {
            // Prepare arguments for the command
            let mut args = Vec::new();
            if let Some(subcommand) = &self.subcommand {
                args.push(subcommand.as_str());
            }
            if let Some(arg_list) = &self.args {
                for arg in arg_list {
                    args.push(arg.as_str());
                }
            }

            // Check if the command handler requires confirmation
            return handler.requires_confirmation(&args);
        }

        // Fall back to the previous implementation for commands not in the registry
        match cmd {
            "quit" => true,
            "clear" => false,
            "profile" => {
                // Check subcommand for profile
                if let Some(subcommand) = &self.subcommand {
                    match subcommand.as_str() {
                        "delete" => true,
                        _ => false,
                    }
                } else {
                    false
                }
            },
            "context" => {
                // Check subcommand for context
                if let Some(subcommand) = &self.subcommand {
                    match subcommand.as_str() {
                        "clear" => true,
                        "rm" => true,
                        _ => false,
                    }
                } else {
                    false
                }
            },
            "tools" => {
                // Check subcommand for tools
                if let Some(subcommand) = &self.subcommand {
                    match subcommand.as_str() {
                        "reset" => true,
                        _ => false,
                    }
                } else {
                    false
                }
            },
            _ => false,
        }
    }

    /// Format the command string with subcommand and arguments
    pub fn format_command_string(&self) -> String {
        // Start with the base command
        let mut cmd_str = if !self.command.starts_with('/') {
            format!("/{}", self.command)
        } else {
            self.command.clone()
        };

        // Add subcommand if present
        if let Some(subcommand) = &self.subcommand {
            cmd_str.push_str(&format!(" {}", subcommand));
        }

        // Add arguments if present
        if let Some(args) = &self.args {
            for arg in args {
                cmd_str.push_str(&format!(" {}", arg));
            }
        }

        // Add flags if present
        if let Some(flags) = &self.flags {
            for (flag, value) in flags {
                if value.is_empty() {
                    cmd_str.push_str(&format!(" --{}", flag));
                } else {
                    cmd_str.push_str(&format!(" --{}={}", flag, value));
                }
            }
        }

        cmd_str
    }

    /// Queue description for the command execution
    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        let command_str = self.format_command_string();

        queue!(
            updates,
            style::Print("Executing command: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&command_str),
            style::ResetColor,
        )?;

        Ok(())
    }

    pub async fn invoke(&self, context: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        // Build the command string using the helper method
        let cmd_str = self.format_command_string();

        // Log the command being executed
        writeln!(updates, "Executing command: {}", cmd_str)?;

        // Get the command registry
        let registry = CommandRegistry::global();

        // Execute the command using the registry
        let result = registry.parse_and_execute(&cmd_str, context, None, None);

        match result.await {
            Ok(chat_state) => {
                // Convert ChatState to appropriate InvokeOutput
                match chat_state {
                    crate::cli::chat::conversation_state::ChatState::Exit => {
                        // Special handling for Exit state - we need to propagate this back to the main application
                        // First, provide a message that will be shown to the user
                        let output = InvokeOutput {
                            output: OutputKind::Text("I'll exit the application after this response. Your chat history and context will be saved for next time.".to_string()),
                        };

                        // Then set a special flag that will be checked by the tool execution code
                        SHOULD_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);

                        Ok(output)
                    },
                    crate::cli::chat::conversation_state::ChatState::DisplayHelp { help_text, .. } => {
                        Ok(InvokeOutput {
                            output: OutputKind::Text(help_text),
                        })
                    },
                    crate::cli::chat::conversation_state::ChatState::PromptUser {
                        skip_printing_tools, ..
                    } => {
                        if skip_printing_tools {
                            // This typically happens after a clear command
                            Ok(InvokeOutput {
                                output: OutputKind::Text("I've cleared our conversation history. We're starting with a fresh chat, but any context files you've added are still available.".to_string()),
                            })
                        } else {
                            // For other cases that return to prompt
                            Ok(InvokeOutput {
                                output: OutputKind::Text(format!("Successfully executed command: {}", cmd_str)),
                            })
                        }
                    },
                    _ => {
                        // For other states, provide a generic success message
                        Ok(InvokeOutput {
                            output: OutputKind::Text(format!("Successfully executed command: {}", cmd_str)),
                        })
                    },
                }
            },
            Err(err) => {
                // Return error message
                Ok(InvokeOutput {
                    output: OutputKind::Text(format!("Failed to execute command: {}. Error: {}", cmd_str, err)),
                })
            },
        }
    }
}
