mod add;
mod clear;
mod remove;
mod show;

use std::future::Future;
use std::pin::Pin;

pub use add::AddContextCommand;
pub use clear::ClearContextCommand;
use eyre::{
    Result,
    eyre,
};
use fig_os_shim::Context;
pub use remove::RemoveContextCommand;
pub use show::ShowContextCommand;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::context::ContextExt;
use crate::cli::chat::{
    ChatState,
    QueuedTool,
};

/// Handler for the context command
pub struct ContextCommand;

impl ContextCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for ContextCommand {
    fn name(&self) -> &'static str {
        "context"
    }

    fn description(&self) -> &'static str {
        "Manage context files for the chat session"
    }

    fn usage(&self) -> &'static str {
        "/context [subcommand]"
    }

    fn help(&self) -> String {
        crate::cli::chat::command::ContextSubcommand::help_text()
    }

    fn llm_description(&self) -> String {
        r#"The context command allows you to manage context files for the chat session.

Available subcommands:
- add: Add files to the context
- rm/remove: Remove files from the context
- clear: Clear all context files
- show/list: Show current context files
- help: Show help for context commands

Examples:
- /context add file.txt - Add a file to the context
- /context add --global file.txt - Add a file to the global context
- /context add --force file.txt - Add a file to the context, even if it's large
- /context rm file.txt - Remove a file from the context
- /context rm 1 - Remove the first file from the context
- /context clear - Clear all context files
- /context show - Show current context files
- /context show --expand - Show current context files with their content"#
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
            // If no arguments, show help
            if args.is_empty() {
                return Ok(ChatState::DisplayHelp {
                    help_text: self.help(),
                    tool_uses,
                    pending_tool_index,
                });
            }

            // Parse subcommand
            let subcommand = args[0];
            let subcommand_args = args[1..].to_vec();

            // Execute subcommand
            match subcommand {
                "add" => {
                    // Parse flags
                    let mut global = false;
                    let mut force = false;
                    let mut paths = Vec::new();

                    for arg in &subcommand_args {
                        match *arg {
                            "--global" => global = true,
                            "--force" => force = true,
                            _ => paths.push(*arg),
                        }
                    }

                    let command = AddContextCommand::new(global, force, paths);
                    command.execute(Vec::new(), ctx, tool_uses, pending_tool_index).await
                },
                "rm" | "remove" => {
                    // Parse flags
                    let mut global = false;
                    let mut paths = Vec::new();

                    for arg in &subcommand_args {
                        match *arg {
                            "--global" => global = true,
                            _ => paths.push(*arg),
                        }
                    }

                    let command = RemoveContextCommand::new(global, paths);
                    command.execute(Vec::new(), ctx, tool_uses, pending_tool_index).await
                },
                "clear" => {
                    // Parse flags
                    let mut global = false;

                    for arg in &subcommand_args {
                        if *arg == "--global" {
                            global = true;
                        }
                    }

                    let command = ClearContextCommand::new(global);
                    command.execute(Vec::new(), ctx, tool_uses, pending_tool_index).await
                },
                "show" | "list" => {
                    // Parse flags
                    let mut global = false;
                    let mut expand = false;

                    for arg in &subcommand_args {
                        match *arg {
                            "--global" => global = true,
                            "--expand" => expand = true,
                            _ => {},
                        }
                    }

                    let command = ShowContextCommand::new(global, expand);
                    command.execute(Vec::new(), ctx, tool_uses, pending_tool_index).await
                },
                "help" => {
                    // Show help text
                    Ok(ChatState::DisplayHelp {
                        help_text: self.help(),
                        tool_uses,
                        pending_tool_index,
                    })
                },
                _ => {
                    // Unknown subcommand
                    Err(eyre!(
                        "Unknown subcommand: {}. Use '/context help' for usage information.",
                        subcommand
                    ))
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_command_help() {
        let command = ContextCommand::new();
        assert_eq!(command.name(), "context");
        assert_eq!(command.description(), "Manage context files for the chat session");
        assert_eq!(command.usage(), "/context [subcommand]");

        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec!["help"], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::DisplayHelp { .. } => {},
                _ => panic!("Expected DisplayHelp state"),
            }
        }
    }

    #[tokio::test]
    async fn test_context_command_no_args() {
        let command = ContextCommand::new();
        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec![], &ctx, None, None).await;
        assert!(result.is_ok());

        if let Ok(state) = result {
            match state {
                ChatState::DisplayHelp { .. } => {},
                _ => panic!("Expected DisplayHelp state"),
            }
        }
    }

    #[tokio::test]
    async fn test_context_command_unknown_subcommand() {
        let command = ContextCommand::new();
        use crate::cli::chat::commands::test_utils::create_test_context;
        let ctx = create_test_context();
        let result = command.execute(vec!["unknown"], &ctx, None, None).await;
        assert!(result.is_err());
    }
}
