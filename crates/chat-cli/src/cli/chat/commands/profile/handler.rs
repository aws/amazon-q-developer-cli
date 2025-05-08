use std::future::Future;
use std::pin::Pin;

use crossterm::style::Color;
use crossterm::{
    queue,
    style,
};

use crate::cli::chat::command::ProfileSubcommand;
use crate::cli::chat::commands::context_adapter::CommandContextAdapter;
use crate::cli::chat::commands::handler::CommandHandler;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Handler for profile commands
pub struct ProfileCommandHandler;

impl ProfileCommandHandler {
    /// Create a new profile command handler
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProfileCommandHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandler for ProfileCommandHandler {
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
        color_print::cformat!(
            r#"
<magenta,em>(Beta) Profile Management</magenta,em>

Profiles allow you to organize and manage different sets of context files for different projects or tasks.

<cyan!>Available commands</cyan!>
  <em>help</em>                <black!>Show an explanation for the profile command</black!>
  <em>list</em>                <black!>List all available profiles</black!>
  <em>create <<n>></em>       <black!>Create a new profile with the specified name</black!>
  <em>delete <<n>></em>       <black!>Delete the specified profile</black!>
  <em>set <<n>></em>          <black!>Switch to the specified profile</black!>
  <em>rename <<old>> <<new>></em>  <black!>Rename a profile</black!>

<cyan!>Notes</cyan!>
• The "global" profile contains context files that are available in all profiles
• The "default" profile is used when no profile is specified
• You can switch between profiles to work on different projects
• Each profile maintains its own set of context files
"#
        )
    }

    fn llm_description(&self) -> String {
        r#"The profile command manages Amazon Q profiles.

Subcommands:
- list: List all available profiles
- create <n>: Create a new profile
- delete <n>: Delete an existing profile
- set <n>: Switch to a different profile
- rename <old_name> <new_name>: Rename an existing profile

Examples:
- "/profile list" - Lists all available profiles
- "/profile create work" - Creates a new profile named "work"
- "/profile set personal" - Switches to the "personal" profile
- "/profile delete test" - Deletes the "test" profile

To get the current profiles, use the command "/profile list" which will display all available profiles with the current one marked."#.to_string()
    }

    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse arguments to determine the subcommand
            let subcommand = if args.is_empty() {
                ProfileSubcommand::List
            } else if let Some(first_arg) = args.first() {
                match *first_arg {
                    "list" => ProfileSubcommand::List,
                    "set" => {
                        if args.len() < 2 {
                            return Err(ChatError::Custom("Missing profile name for set command".into()));
                        }
                        ProfileSubcommand::Set {
                            name: args[1].to_string(),
                        }
                    },
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
                    "rename" => {
                        if args.len() < 3 {
                            return Err(ChatError::Custom("Missing old or new profile name for rename command".into()));
                        }
                        ProfileSubcommand::Rename {
                            old_name: args[1].to_string(),
                            new_name: args[2].to_string(),
                        }
                    },
                    "help" => ProfileSubcommand::Help,
                    _ => ProfileSubcommand::Help,
                }
            } else {
                ProfileSubcommand::List // Fallback, should not happen
            };

            match subcommand {
                ProfileSubcommand::List => {
                    // Get the context manager
                    if let Some(context_manager) = &ctx.conversation_state.context_manager {
                        // Get the list of profiles
                        let profiles = context_manager.list_profiles().await?;
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
                    } else {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nContext manager is not available.\n\n"),
                            style::ResetColor
                        )?;
                    }
                },
                ProfileSubcommand::Create { name } => {
                    // Get the context manager
                    if let Some(context_manager) = &ctx.conversation_state.context_manager {
                        // Create the profile
                        context_manager.create_profile(&name).await?;

                        queue!(
                            ctx.output,
                            style::Print("\nProfile '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(name),
                            style::ResetColor,
                            style::Print("' created successfully.\n\n")
                        )?;
                    } else {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nContext manager is not available.\n\n"),
                            style::ResetColor
                        )?;
                    }
                },
                ProfileSubcommand::Delete { name } => {
                    // Get the context manager
                    if let Some(context_manager) = &ctx.conversation_state.context_manager {
                        // Delete the profile
                        context_manager.delete_profile(&name).await?;

                        queue!(
                            ctx.output,
                            style::Print("\nProfile '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(name),
                            style::ResetColor,
                            style::Print("' deleted successfully.\n\n")
                        )?;
                    } else {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nContext manager is not available.\n\n"),
                            style::ResetColor
                        )?;
                    }
                },
                ProfileSubcommand::Set { name } => {
                    // Get the context manager
                    if let Some(context_manager) = &mut ctx.conversation_state.context_manager {
                        // Switch to the profile
                        context_manager.switch_profile(&name).await?;

                        queue!(
                            ctx.output,
                            style::Print("\nSwitched to profile '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(name),
                            style::ResetColor,
                            style::Print("'.\n\n")
                        )?;
                    } else {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nContext manager is not available.\n\n"),
                            style::ResetColor
                        )?;
                    }
                },
                ProfileSubcommand::Rename { old_name, new_name } => {
                    // Get the context manager
                    if let Some(context_manager) = &mut ctx.conversation_state.context_manager {
                        // Rename the profile
                        context_manager.rename_profile(&old_name, &new_name).await?;

                        queue!(
                            ctx.output,
                            style::Print("\nProfile '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(old_name),
                            style::ResetColor,
                            style::Print("' renamed to '"),
                            style::SetForegroundColor(Color::Green),
                            style::Print(new_name),
                            style::ResetColor,
                            style::Print("'.\n\n")
                        )?;
                    } else {
                        queue!(
                            ctx.output,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nContext manager is not available.\n\n"),
                            style::ResetColor
                        )?;
                    }
                },
                ProfileSubcommand::Help => {
                    // Display help text
                    queue!(
                        ctx.output,
                        style::Print("\n"),
                        style::Print(self.help()),
                        style::Print("\n")
                    )?;
                },
            }

            Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            })
        })
    }

    fn requires_confirmation(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false; // Default list doesn't require confirmation
        }

        match args[0] {
            "list" | "help" => false, // Read-only commands don't require confirmation
            "delete" => true,         // Delete always requires confirmation
            _ => false,               // Other commands don't require confirmation
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::platform::Context;

    use super::*;
    use crate::Settings;
    use crate::cli::chat::conversation_state::ConversationState;
    use crate::cli::chat::input_source::InputSource;
    use crate::shared_writer::SharedWriter;
    use crate::cli::chat::tools::ToolPermissions;

    #[tokio::test]
    async fn test_profile_list_command() {
        let handler = ProfileCommandHandler::new();

        // Create a minimal context
        let context = Arc::new(Context::new_fake());
        let output = SharedWriter::null();
        let mut conversation_state =
            ConversationState::new(Arc::clone(&context), HashMap::new(), None, Some(SharedWriter::null())).await;
        let mut tool_permissions = ToolPermissions::new(0);
        let mut input_source = InputSource::new_mock(vec![]);
        let settings = Settings::new_fake();

        let mut ctx = CommandContextAdapter {
            context: &context,
            output: &mut output.clone(),
            conversation_state: &mut conversation_state,
            tool_permissions: &mut tool_permissions,
            interactive: true,
            input_source: &mut input_source,
            settings: &settings,
        };

        // Execute the list subcommand
        let args = vec!["list"];
        let result = handler.execute(args, &mut ctx, None, None).await;

        assert!(result.is_ok());
    }
}
