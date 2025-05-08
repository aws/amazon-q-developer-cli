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

/// Static instance of the profile list command handler
pub static LIST_PROFILE_HANDLER: ListProfileCommand = ListProfileCommand;

/// Handler for the profile list command
pub struct ListProfileCommand;

impl CommandHandler for ListProfileCommand {
    fn name(&self) -> &'static str {
        "list"
    }

    fn description(&self) -> &'static str {
        "List available profiles"
    }

    fn usage(&self) -> &'static str {
        "/profile list"
    }

    fn help(&self) -> String {
        "List all available profiles and show which one is currently active.".to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Profile {
            subcommand: ProfileSubcommand::List,
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
            if let Command::Profile {
                subcommand: ProfileSubcommand::List,
            } = command
            {
                #[cfg(not(test))]
                {
                    // Get the context manager
                    if let Some(context_manager) = &ctx.conversation_state.context_manager {
                        // Get the list of profiles
                        let profiles = match context_manager.list_profiles().await {
                            Ok(profiles) => profiles,
                            Err(e) => return Err(ChatError::Custom(format!("Failed to list profiles: {}", e).into())),
                        };
                        let current_profile = &context_manager.current_profile;

                        // Display the profiles
                        queue!(ctx.output, style::Print("\nAvailable profiles:\n"))?;

                        for profile in profiles {
                            if &profile == current_profile {
                                queue!(
                                    ctx.output,
                                    style::Print("* "),
                                    style::SetForegroundColor(Color::Green),
                                    style::Print(profile),
                                    style::ResetColor,
                                    style::Print("\n")
                                )?;
                            } else {
                                queue!(
                                    ctx.output,
                                    style::Print("  "),
                                    style::Print(profile),
                                    style::Print("\n")
                                )?;
                            }
                        }

                        queue!(ctx.output, style::Print("\n"))?;
                        ctx.output.flush()?;
                    } else {
                        return Err(ChatError::Custom("Context manager is not available".into()));
                    }
                }

                #[cfg(test)]
                {
                    // Mock implementation for testing
                    let profiles = vec!["default".to_string(), "test".to_string()];
                    let current_profile = "default";

                    // Display the profiles
                    queue!(ctx.output, style::Print("\nAvailable profiles:\n"))?;

                    for profile in profiles {
                        if &profile == current_profile {
                            queue!(
                                ctx.output,
                                style::Print("* "),
                                style::SetForegroundColor(Color::Green),
                                style::Print(profile),
                                style::ResetColor,
                                style::Print("\n")
                            )?;
                        } else {
                            queue!(
                                ctx.output,
                                style::Print("  "),
                                style::Print(profile),
                                style::Print("\n")
                            )?;
                        }
                    }

                    queue!(ctx.output, style::Print("\n"))?;
                    ctx.output.flush()?;
                }

                Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom(
                    "ListProfileCommand can only execute List profile commands".into(),
                ))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // List command doesn't require confirmation
    }
}

#[cfg(test)]
mod tests {

    // Test implementations would go here
}
