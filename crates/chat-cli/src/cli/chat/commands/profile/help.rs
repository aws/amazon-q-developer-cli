use std::future::Future;
use std::pin::Pin;

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

/// Static instance of the profile help command handler
pub static HELP_PROFILE_HANDLER: HelpProfileCommand = HelpProfileCommand;

/// Handler for the profile help command
pub struct HelpProfileCommand;

impl Default for HelpProfileCommand {
    fn default() -> Self {
        Self
    }
}

impl CommandHandler for HelpProfileCommand {
    fn name(&self) -> &'static str {
        "profile help"
    }

    fn description(&self) -> &'static str {
        "Display help information for the profile command"
    }

    fn usage(&self) -> &'static str {
        "/profile help"
    }

    fn help(&self) -> String {
        "Displays help information for the profile command and its subcommands.".to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Help {
            help_text: Some(ProfileSubcommand::help_text()),
        })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        _ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            match command {
                Command::Help { .. } => {
                    // The Help command will be handled by the Help command handler
                    // Create a new Command::Help with the same help_text
                    let help_text = ProfileSubcommand::help_text();
                    Ok(ChatState::ExecuteCommand {
                        command: Command::Help {
                            help_text: Some(help_text),
                        },
                        tool_uses,
                        pending_tool_index,
                    })
                },
                _ => Err(ChatError::Custom(
                    "HelpProfileCommand can only execute Help commands".into(),
                )),
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false
    }
}
