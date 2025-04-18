use std::collections::HashMap;
use std::io::Write;

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
use crate::cli::chat::tools::internal_command::schema::InternalCommand;
use crate::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
};

/// Response from executing a Q command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalCommandResponse {
    /// Whether the command was executed successfully
    pub success: bool,

    /// Output from the command execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,

    /// Error message if the command failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl InternalCommand {
    /// Create a new InternalCommand instance
    ///
    /// TODO: This function is currently unused. Consider removing it or implementing its usage
    /// as part of Phase 7.4: Technical Debt Reduction in the implementation plan.
    #[allow(dead_code)]
    pub fn new(
        command: String,
        subcommand: Option<String>,
        args: Option<Vec<String>>,
        flags: Option<HashMap<String, String>>,
        tool_use_id: Option<String>,
    ) -> Self {
        Self {
            command,
            subcommand,
            args,
            flags,
            tool_use_id,
        }
    }

    pub fn validate(&self, _ctx: &Context) -> Result<(), ToolResult> {
        // Validate that the command is one of the known commands
        let cmd = self.command.trim_start_matches('/');

        // Get the command registry to check if the command exists
        let registry = CommandRegistry::global();

        if registry.get(cmd).is_some() {
            return Ok(());
        }

        // Fall back to the previous implementation for commands not in the registry
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

    pub fn requires_acceptance(&self, _ctx: &Context) -> bool {
        // Check if the command is one that should be trusted without confirmation
        let cmd = self.command.trim_start_matches('/');

        // Get the command registry
        let registry = CommandRegistry::global();

        // Check if the command exists and requires confirmation
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

            return handler.requires_confirmation(&args);
        }

        // For commands not in the registry, default to requiring acceptance
        // This provides a security boundary between the AI and command execution
        true
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
            style::SetForegroundColor(Color::Blue),
            style::Print("Execute command: "),
            style::SetForegroundColor(Color::Yellow),
            style::Print(&command_str),
            style::ResetColor,
            style::Print("\n"),
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
                    crate::cli::chat::ChatState::Exit => {
                        // Special handling for Exit state - we need to propagate this back to the main application
                        // Provide a message that will be shown to the user and pass through the Exit state
                        let output = InvokeOutput {
                            output: OutputKind::Text("I'll exit the application after this response.".to_string()),
                            next_state: Some(chat_state), // Pass the Exit state through
                        };

                        Ok(output)
                    },
                    crate::cli::chat::ChatState::DisplayHelp { help_text, .. } => {
                        // Print the help text directly to the output instead of returning it for display
                        queue!(
                            updates,
                            style::ResetColor,
                            style::Print(help_text),
                            style::Print("\n"),
                            style::ResetColor,
                        )?;

                        Ok(InvokeOutput {
                            output: OutputKind::Text("Help information has been displayed directly to the user. DO NOT give further information in your response, other than an acknowledgement.".to_string()),
                            next_state: Some(crate::cli::chat::ChatState::PromptUser {
                                tool_uses: None,
                                pending_tool_index: None,
                                skip_printing_tools: false,
                            }),
                        })
                    },
                    crate::cli::chat::ChatState::PromptUser {
                        skip_printing_tools, ..
                    } => {
                        if skip_printing_tools {
                            // This typically happens after a clear command
                            Ok(InvokeOutput {
                                output: OutputKind::Text("I've cleared our conversation history. We're starting with a fresh chat, but any context files you've added are still available.".to_string()),
                                next_state: Some(chat_state), // Pass the PromptUser state through
                            })
                        } else {
                            // For other cases that return to prompt
                            Ok(InvokeOutput {
                                output: OutputKind::Text(format!("Successfully executed command: {}", cmd_str)),
                                next_state: Some(chat_state), // Pass the PromptUser state through
                            })
                        }
                    },
                    _ => {
                        // For other states, provide a generic success message but also pass through the chat_state
                        Ok(InvokeOutput {
                            output: OutputKind::Text(format!("Successfully executed command: {}", cmd_str)),
                            next_state: Some(chat_state), // Pass through any other state
                        })
                    },
                }
            },
            Err(err) => {
                // Return error message with default PromptUser state
                Ok(InvokeOutput {
                    output: OutputKind::Text(format!("Failed to execute command: {}. Error: {}", cmd_str, err)),
                    next_state: Some(crate::cli::chat::ChatState::PromptUser {
                        tool_uses: None,
                        pending_tool_index: None,
                        skip_printing_tools: false,
                    }),
                })
            },
        }
    }
}
