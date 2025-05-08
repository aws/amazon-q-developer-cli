use std::future::Future;
use std::io::Write;
use std::pin::Pin;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::command::{
    Command,
    ProfileSubcommand,
};
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Static instance of the profile create command handler
pub static CREATE_PROFILE_HANDLER: CreateProfileCommand = CreateProfileCommand;

/// Handler for the profile create command
pub struct CreateProfileCommand;

impl CommandHandler for CreateProfileCommand {
    fn name(&self) -> &'static str {
        "create"
    }

    fn description(&self) -> &'static str {
        "Create a new profile"
    }

    fn usage(&self) -> &'static str {
        "/profile create <n>"
    }

    fn help(&self) -> String {
        "Create a new profile with the specified name.".to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        if args.len() != 1 {
            return Err(ChatError::Custom("Expected profile name argument".into()));
        }

        Ok(Command::Profile {
            subcommand: ProfileSubcommand::Create {
                name: args[0].to_string(),
            },
        })
    }

    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        _ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse the command to get the profile name
            let command = self.to_command(args)?;

            // Return the command wrapped in ExecuteCommand state
            Ok(ChatState::ExecuteCommand {
                command,
                tool_uses,
                pending_tool_index,
            })
        })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Extract the profile name from the command
            let name = match command {
                Command::Profile {
                    subcommand: ProfileSubcommand::Create { name },
                } => name,
                _ => return Err(ChatError::Custom("Invalid command".into())),
            };

            // Get the context manager
            if let Some(context_manager) = &ctx.conversation_state.context_manager {
                // Create the profile
                match context_manager.create_profile(name).await {
                    Ok(_) => {
                        queue!(
                            ctx.output,
                            style::Print("\nProfile '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(name),
                            style::ResetColor,
                            style::Print("' created successfully.\n\n")
                        )?;
                    },
                    Err(e) => {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nError creating profile: {}\n\n", e)),
                            style::ResetColor
                        )?;
                    },
                }
                ctx.output.flush()?;
            } else {
                queue!(
                    ctx.output,
                    style::SetForegroundColor(Color::Red),
                    style::Print("\nContext manager is not available.\n\n"),
                    style::ResetColor
                )?;
                ctx.output.flush()?;
            }

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Create command requires confirmation as it's a mutative operation
    }
}
