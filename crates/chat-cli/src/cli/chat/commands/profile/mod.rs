use std::future::Future;
use std::pin::Pin;

use super::CommandHandler;
use crate::cli::chat::command::{
    Command,
    ProfileSubcommand,
};
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

mod create;
mod delete;
mod help;
mod list;
mod rename;
mod set;

// Static handlers for profile subcommands
pub use create::CREATE_PROFILE_HANDLER;
pub use delete::DELETE_PROFILE_HANDLER;
pub use help::HELP_PROFILE_HANDLER;
pub use list::LIST_PROFILE_HANDLER;
pub use rename::RENAME_PROFILE_HANDLER;
pub use set::SET_PROFILE_HANDLER;

/// Profile command handler
pub struct ProfileCommand;

/// Static instance of the profile command handler
pub static PROFILE_HANDLER: ProfileCommand = ProfileCommand;

impl ProfileCommand {
    /// Create a new profile command handler
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProfileCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandler for ProfileCommand {
    fn name(&self) -> &'static str {
        "profile"
    }

    fn description(&self) -> &'static str {
        "Manage profiles"
    }

    fn usage(&self) -> &'static str {
        "/profile [subcommand]"
    }

    fn help(&self) -> String {
        "Manage profiles for the chat session.\n\n\
        Subcommands:\n\
        help        Show profile help\n\
        list        List profiles\n\
        set         Set the current profile\n\
        create      Create a new profile\n\
        delete      Delete a profile\n\
        rename      Rename a profile"
            .to_string()
    }

    fn llm_description(&self) -> String {
        r#"The profile command manages different profiles for organizing context files.

Subcommands:
- list: List all available profiles
- create <n>: Create a new profile
- delete <n>: Delete a profile
- set <n>: Switch to a different profile
- rename <old_name> <new_name>: Rename a profile

Examples:
- "/profile list" - Lists all available profiles
- "/profile create work" - Creates a new profile named "work"
- "/profile set work" - Switches to the "work" profile
- "/profile delete old_profile" - Deletes the profile named "old_profile"
- "/profile rename work work_new" - Renames the "work" profile to "work_new"

Profiles allow you to organize context files for different projects or tasks. The "global" profile contains context files that are available in all profiles."#
            .to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        // Check if this is a help request
        if args.is_empty() || (args.len() == 1 && args[0] == "help") {
            return Ok(Command::Help {
                help_text: Some(ProfileSubcommand::help_text()),
            });
        }

        // Parse arguments to determine the subcommand
        let subcommand = if let Some(first_arg) = args.first() {
            match *first_arg {
                "list" => ProfileSubcommand::List,
                "create" => {
                    if args.len() < 2 {
                        return Err(ChatError::Custom("Missing profile name for create command".into()));
                    }
                    ProfileSubcommand::Create {
                        name: args[1].to_string(),
                    }
                },
                "delete" => {
                    if args.len() < 2 {
                        return Err(ChatError::Custom("Missing profile name for delete command".into()));
                    }
                    ProfileSubcommand::Delete {
                        name: args[1].to_string(),
                    }
                },
                "set" => {
                    if args.len() < 2 {
                        return Err(ChatError::Custom("Missing profile name for set command".into()));
                    }
                    ProfileSubcommand::Set {
                        name: args[1].to_string(),
                    }
                },
                "rename" => {
                    if args.len() < 3 {
                        return Err(ChatError::Custom("Missing profile names for rename command".into()));
                    }
                    ProfileSubcommand::Rename {
                        old_name: args[1].to_string(),
                        new_name: args[2].to_string(),
                    }
                },
                "help" => {
                    // This case is handled above, but we'll include it here for completeness
                    return Ok(Command::Help {
                        help_text: Some(ProfileSubcommand::help_text()),
                    });
                },
                _ => {
                    // For unknown subcommands, show help
                    return Ok(Command::Help {
                        help_text: Some(ProfileSubcommand::help_text()),
                    });
                },
            }
        } else {
            // This case is handled above, but we'll include it here for completeness
            return Ok(Command::Help {
                help_text: Some(ProfileSubcommand::help_text()),
            });
        };

        Ok(Command::Profile { subcommand })
    }

    fn requires_confirmation(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false; // Default help doesn't require confirmation
        }

        match args[0] {
            "list" | "help" => false, // Read-only commands don't require confirmation
            "delete" => true,         // Delete requires confirmation
            _ => false,               // Other commands don't require confirmation
        }
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut crate::cli::chat::commands::context_adapter::CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            match command {
                Command::Profile { subcommand } => {
                    // Delegate to the appropriate subcommand handler
                    subcommand
                        .to_handler()
                        .execute_command(command, ctx, tool_uses, pending_tool_index)
                        .await
                },
                _ => Err(ChatError::Custom(
                    "ProfileCommand can only execute Profile commands".into(),
                )),
            }
        })
    }
}
