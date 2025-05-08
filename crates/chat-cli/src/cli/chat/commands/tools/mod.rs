use std::future::Future;
use std::pin::Pin;

use crate::cli::chat::command::{
    Command,
    ToolsSubcommand,
};
use crate::cli::chat::commands::{
    CommandContextAdapter,
    CommandHandler,
};
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

mod help;
mod list;
mod reset;
mod reset_single;
mod trust;
mod trustall;
mod untrust;

// Static handlers for tools subcommands
pub use help::HELP_TOOLS_HANDLER;
pub use list::LIST_TOOLS_HANDLER;
pub use reset::RESET_TOOLS_HANDLER;
pub use reset_single::RESET_SINGLE_TOOL_HANDLER;
pub use trust::TRUST_TOOLS_HANDLER;
pub use trustall::TRUSTALL_TOOLS_HANDLER;
pub use untrust::UNTRUST_TOOLS_HANDLER;

/// Static instance of the tools command handler
pub static TOOLS_HANDLER: ToolsCommand = ToolsCommand;

/// Handler for the tools command
pub struct ToolsCommand;

impl ToolsCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolsCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandler for ToolsCommand {
    fn name(&self) -> &'static str {
        "tools"
    }

    fn description(&self) -> &'static str {
        "View and manage tools and permissions"
    }

    fn usage(&self) -> &'static str {
        "/tools [subcommand]"
    }

    fn help(&self) -> String {
        color_print::cformat!(
            r#"
<magenta,em>Tools Management</magenta,em>

Tools allow Amazon Q to perform actions on your system, such as executing commands or modifying files.
You can view and manage tool permissions using the following commands:

<cyan!>Available commands</cyan!>
  <em>list</em>                <black!>List all available tools and their status</black!>
  <em>trust <<tool>></em>      <black!>Trust a specific tool for the session</black!>
  <em>untrust <<tool>></em>    <black!>Revert a tool to per-request confirmation</black!>
  <em>trustall</em>            <black!>Trust all tools for the session</black!>
  <em>reset</em>               <black!>Reset all tools to default permission levels</black!>

<cyan!>Notes</cyan!>
• You will be prompted for permission before any tool is used
• You can trust tools for the duration of a session
• Trusted tools will not require confirmation each time they're used
"#
        )
    }

    fn llm_description(&self) -> String {
        r#"The tools command manages tool permissions and settings.

Subcommands:
- <empty>: List all available tools and their trust status
- trust <tool_name>: Trust a specific tool (don't ask for confirmation)
- untrust <tool_name>: Untrust a specific tool (ask for confirmation)
- trustall: Trust all tools
- reset: Reset all tool permissions to default

Examples:
- "/tools" - Lists all available tools
- "/tools trust fs_write" - Trusts the fs_write tool
- "/tools untrust execute_bash" - Untrusts the execute_bash tool
- "/tools trustall" - Trusts all tools
- "/tools reset" - Resets all tool permissions to default

To get the current tool status, use the command "/tools" which will display all available tools with their current permission status."#.to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        if args.is_empty() {
            // Default to showing the list when no subcommand is provided
            return Ok(Command::Tools { subcommand: None });
        }

        // Check if this is a help request
        if args.len() == 1 && args[0] == "help" {
            return Ok(Command::Help {
                help_text: Some(ToolsSubcommand::help_text()),
            });
        }

        // Parse arguments to determine the subcommand
        let subcommand = if let Some(first_arg) = args.first() {
            match *first_arg {
                "list" => None, // "list" is an unlisted alias for the default behavior (list tools)
                "trust" => {
                    let tool_names = args[1..].iter().map(|s| (*s).to_string()).collect();
                    Some(ToolsSubcommand::Trust { tool_names })
                },
                "untrust" => {
                    let tool_names = args[1..].iter().map(|s| (*s).to_string()).collect();
                    Some(ToolsSubcommand::Untrust { tool_names })
                },
                "trustall" => Some(ToolsSubcommand::TrustAll { from_deprecated: false }),
                "reset" => {
                    if args.len() > 1 {
                        Some(ToolsSubcommand::ResetSingle {
                            tool_name: args[1].to_string(),
                        })
                    } else {
                        Some(ToolsSubcommand::Reset)
                    }
                },
                "help" => {
                    // This case is handled above, but we'll include it here for completeness
                    return Ok(Command::Help {
                        help_text: Some(ToolsSubcommand::help_text()),
                    });
                },
                _ => {
                    // For unknown subcommands, show help
                    return Ok(Command::Help {
                        help_text: Some(ToolsSubcommand::help_text()),
                    });
                },
            }
        } else {
            None // Default to list if no arguments (should not happen due to earlier check)
        };

        Ok(Command::Tools { subcommand })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            match command {
                Command::Tools { subcommand: None } => {
                    // Default behavior is to list tools
                    LIST_TOOLS_HANDLER
                        .execute_command(command, ctx, tool_uses, pending_tool_index)
                        .await
                },
                Command::Tools {
                    subcommand: Some(subcommand),
                } => {
                    // Delegate to the appropriate subcommand handler
                    subcommand
                        .to_handler()
                        .execute_command(command, ctx, tool_uses, pending_tool_index)
                        .await
                },
                _ => Err(ChatError::Custom("ToolsCommand can only execute Tools commands".into())),
            }
        })
    }

    fn requires_confirmation(&self, args: &[&str]) -> bool {
        if args.is_empty() {
            return false; // Default list doesn't require confirmation
        }

        // Shouldn't get here, as this should delegate to the subcommand
        match args[0] {
            "help" | "list" => false, // Help and list don't require confirmation
            "trustall" => true,       // Trustall requires confirmation
            _ => true,                // Other commands require confirmation
        }
    }
}
pub mod test_separation;
impl ToolsSubcommand {
    pub fn to_handler(&self) -> &'static dyn CommandHandler {
        match self {
            ToolsSubcommand::Schema => &TOOLS_HANDLER,
            ToolsSubcommand::Trust { .. } => &TRUST_TOOLS_HANDLER,
            ToolsSubcommand::Untrust { .. } => &UNTRUST_TOOLS_HANDLER,
            ToolsSubcommand::TrustAll { .. } => &TRUSTALL_TOOLS_HANDLER,
            ToolsSubcommand::Reset => &RESET_TOOLS_HANDLER,
            ToolsSubcommand::ResetSingle { .. } => &RESET_SINGLE_TOOL_HANDLER,
            ToolsSubcommand::Help => &HELP_TOOLS_HANDLER,
        }
    }
}
