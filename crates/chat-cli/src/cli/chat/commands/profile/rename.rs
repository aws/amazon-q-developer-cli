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

/// Static instance of the profile rename command handler
pub static RENAME_PROFILE_HANDLER: RenameProfileCommand = RenameProfileCommand;

/// Handler for the profile rename command
pub struct RenameProfileCommand;

impl Default for RenameProfileCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl RenameProfileCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for RenameProfileCommand {
    fn name(&self) -> &'static str {
        "rename"
    }

    fn description(&self) -> &'static str {
        "Rename a profile"
    }

    fn usage(&self) -> &'static str {
        "/profile rename <old_name> <new_name>"
    }

    fn help(&self) -> String {
        "Rename a profile from <old_name> to <new_name>.".to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        if args.len() != 2 {
            return Err(ChatError::Custom("Expected old_name and new_name arguments".into()));
        }

        let old_name = args[0].to_string();
        let new_name = args[1].to_string();

        Ok(Command::Profile {
            subcommand: ProfileSubcommand::Rename { old_name, new_name },
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
            // Extract the profile names from the command
            let (old_name, new_name) = match command {
                Command::Profile {
                    subcommand: ProfileSubcommand::Rename { old_name, new_name },
                } => (old_name, new_name),
                _ => return Err(ChatError::Custom("Invalid command".into())),
            };

            // Get the context manager
            if let Some(context_manager) = &mut ctx.conversation_state.context_manager {
                // Rename the profile
                match context_manager.rename_profile(old_name, new_name).await {
                    Ok(_) => {
                        queue!(
                            ctx.output,
                            style::Print("\nRenamed profile '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(old_name),
                            style::ResetColor,
                            style::Print("' to '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(new_name),
                            style::ResetColor,
                            style::Print("'.\n\n")
                        )?;
                    },
                    Err(e) => {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("\nError renaming profile: {}\n\n", e)),
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
        true // Rename command requires confirmation as it's a mutative operation
    }
}
