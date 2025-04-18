mod create;
mod delete;
mod list;
mod rename;
mod set;

use std::future::Future;
use std::io::Write;
use std::pin::Pin;

pub use create::CreateProfileCommand;
use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
pub use delete::DeleteProfileCommand;
use eyre::Result;
use fig_os_shim::Context;
pub use list::ListProfilesCommand;
pub use rename::RenameProfileCommand;
pub use set::SetProfileCommand;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::context::ContextExt;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

/// Handler for the profile command
pub struct ProfileCommand;

impl ProfileCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for ProfileCommand {
    fn name(&self) -> &'static str {
        "profile"
    }

    fn description(&self) -> &'static str {
        "Manage profiles for the chat session"
    }

    fn usage(&self) -> &'static str {
        "/profile [subcommand]"
    }

    fn help(&self) -> String {
        "Manage profiles for the chat session. Use subcommands to list, create, delete, or switch profiles.".to_string()
    }

    fn llm_description(&self) -> String {
        "Manage profiles for the chat session. Profiles allow you to maintain separate context configurations."
            .to_string()
    }

    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        ctx: &'a Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState>> + Send + 'a>> {
        Box::pin(async move {
            let mut stdout = ctx.stdout();

            // If no subcommand is provided, show help
            if args.is_empty() {
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("\nProfile Command\n\n"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("Usage: /profile [subcommand]\n\n"),
                    style::Print("Available subcommands:\n"),
                    style::Print("  list    - List available profiles\n"),
                    style::Print("  set     - Switch to a profile\n"),
                    style::Print("  create  - Create a new profile\n"),
                    style::Print("  delete  - Delete a profile\n"),
                    style::Print("  rename  - Rename a profile\n"),
                    style::Print("  help    - Show this help message\n\n"),
                )?;
                stdout.flush()?;
                return Ok(ChatState::PromptUser {
                    tool_uses,
                    pending_tool_index,
                    skip_printing_tools: true,
                });
            }

            // Parse subcommand
            let subcommand = args[0];
            let subcommand_args = if args.len() > 1 { &args[1..] } else { &[] };

            // Dispatch to appropriate subcommand handler
            match subcommand {
                "list" => {
                    let command = ListProfilesCommand::new();
                    command
                        .execute(subcommand_args.to_vec(), ctx, tool_uses, pending_tool_index)
                        .await
                },
                "set" => {
                    if subcommand_args.is_empty() {
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nError: Missing profile name\n"),
                            style::Print("Usage: /profile set <name>\n\n"),
                            style::ResetColor
                        )?;
                        stdout.flush()?;
                        return Ok(ChatState::PromptUser {
                            tool_uses,
                            pending_tool_index,
                            skip_printing_tools: true,
                        });
                    }
                    let command = SetProfileCommand::new(subcommand_args[0]);
                    command
                        .execute(subcommand_args[1..].to_vec(), ctx, tool_uses, pending_tool_index)
                        .await
                },
                "create" => {
                    if subcommand_args.is_empty() {
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nError: Missing profile name\n"),
                            style::Print("Usage: /profile create <name>\n\n"),
                            style::ResetColor
                        )?;
                        stdout.flush()?;
                        return Ok(ChatState::PromptUser {
                            tool_uses,
                            pending_tool_index,
                            skip_printing_tools: true,
                        });
                    }
                    let command = CreateProfileCommand::new(subcommand_args[0]);
                    command
                        .execute(subcommand_args[1..].to_vec(), ctx, tool_uses, pending_tool_index)
                        .await
                },
                "delete" => {
                    if subcommand_args.is_empty() {
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nError: Missing profile name\n"),
                            style::Print("Usage: /profile delete <name>\n\n"),
                            style::ResetColor
                        )?;
                        stdout.flush()?;
                        return Ok(ChatState::PromptUser {
                            tool_uses,
                            pending_tool_index,
                            skip_printing_tools: true,
                        });
                    }
                    let command = DeleteProfileCommand::new(subcommand_args[0]);
                    command
                        .execute(subcommand_args[1..].to_vec(), ctx, tool_uses, pending_tool_index)
                        .await
                },
                "rename" => {
                    if subcommand_args.len() < 2 {
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Red),
                            style::Print("\nError: Missing profile names\n"),
                            style::Print("Usage: /profile rename <old-name> <new-name>\n\n"),
                            style::ResetColor
                        )?;
                        stdout.flush()?;
                        return Ok(ChatState::PromptUser {
                            tool_uses,
                            pending_tool_index,
                            skip_printing_tools: true,
                        });
                    }
                    let command = RenameProfileCommand::new(subcommand_args[0], subcommand_args[1]);
                    command
                        .execute(subcommand_args[2..].to_vec(), ctx, tool_uses, pending_tool_index)
                        .await
                },
                "help" => {
                    // Show help text
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("\nProfile Command Help\n\n"),
                        style::SetForegroundColor(Color::Reset),
                        style::Print("Usage: /profile [subcommand]\n\n"),
                        style::Print("Available subcommands:\n"),
                        style::Print("  list                - List available profiles\n"),
                        style::Print("  set <profile>       - Switch to a profile\n"),
                        style::Print("  create <profile>    - Create a new profile\n"),
                        style::Print("  delete <profile>    - Delete a profile\n"),
                        style::Print("  rename <old> <new>  - Rename a profile\n"),
                        style::Print("  help                - Show this help message\n\n"),
                        style::Print("Examples:\n"),
                        style::Print("  /profile list\n"),
                        style::Print("  /profile set work\n"),
                        style::Print("  /profile create personal\n"),
                        style::Print("  /profile delete test\n"),
                        style::Print("  /profile rename old-name new-name\n\n"),
                    )?;
                    stdout.flush()?;
                    Ok(ChatState::PromptUser {
                        tool_uses,
                        pending_tool_index,
                        skip_printing_tools: true,
                    })
                },
                _ => {
                    // Unknown subcommand
                    queue!(
                        stdout,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nUnknown subcommand: {}\n\n", subcommand)),
                        style::SetForegroundColor(Color::Reset),
                        style::Print("Available subcommands: list, set, create, delete, rename, help\n\n"),
                    )?;
                    stdout.flush()?;
                    Ok(ChatState::PromptUser {
                        tool_uses,
                        pending_tool_index,
                        skip_printing_tools: true,
                    })
                },
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Profile command doesn't require confirmation
    }

    fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::commands::test_utils::create_test_context;

    #[tokio::test]
    async fn test_profile_command_help() {
        let command = ProfileCommand::new();
        assert_eq!(command.name(), "profile");
        assert_eq!(command.description(), "Manage profiles for the chat session");
        assert_eq!(command.usage(), "/profile [subcommand]");
    }

    #[tokio::test]
    async fn test_profile_command_no_args() {
        let command = ProfileCommand::new();
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_profile_command_unknown_subcommand() {
        let command = ProfileCommand::new();
        let ctx = create_test_context();
        let result = command.execute(vec!["unknown"], &ctx, None, None).await;
        assert!(result.is_ok());
    }
}
