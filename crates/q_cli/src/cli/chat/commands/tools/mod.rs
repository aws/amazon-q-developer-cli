mod list;
mod enable;
mod disable;

use std::io::Write;

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::conversation_state::ChatState;
use crate::cli::chat::QueuedTool;

pub use list::ListToolsCommand;
pub use enable::EnableToolCommand;
pub use disable::DisableToolCommand;

/// Handler for the tools command
pub struct ToolsCommand;

impl ToolsCommand {
    pub fn new() -> Self {
        Self
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
You can view, enable, or disable tools using the following commands:

<cyan!>Available commands</cyan!>
  <em>list</em>                <black!>List all available tools and their status</black!>
  <em>enable <<tool>></em>     <black!>Enable a specific tool</black!>
  <em>disable <<tool>></em>    <black!>Disable a specific tool</black!>

<cyan!>Notes</cyan!>
• Disabled tools cannot be used by Amazon Q
• You will be prompted for permission before any tool is used
• You can trust tools for the duration of a session
"#
        )
    }
    
    fn execute(
        &self, 
        args: Vec<&str>, 
        ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        if args.is_empty() {
            return Ok(ChatState::DisplayHelp {
                help_text: self.help(),
                tool_uses,
                pending_tool_index,
            });
        }
        
        let subcommand = match args[0] {
            "list" => ListToolsCommand::new(),
            "enable" => {
                if args.len() < 2 {
                    return Ok(ChatState::DisplayHelp {
                        help_text: format!("Usage: /tools enable <tool_name>"),
                        tool_uses,
                        pending_tool_index,
                    });
                }
                EnableToolCommand::new(args[1])
            },
            "disable" => {
                if args.len() < 2 {
                    return Ok(ChatState::DisplayHelp {
                        help_text: format!("Usage: /tools disable <tool_name>"),
                        tool_uses,
                        pending_tool_index,
                    });
                }
                DisableToolCommand::new(args[1])
            },
            "help" => {
                return Ok(ChatState::DisplayHelp {
                    help_text: self.help(),
                    tool_uses,
                    pending_tool_index,
                });
            },
            _ => {
                return Ok(ChatState::DisplayHelp {
                    help_text: self.help(),
                    tool_uses,
                    pending_tool_index,
                });
            }
        };
        
        subcommand.execute(args, ctx, tool_uses, pending_tool_index)
    }
    
    fn parse_args(&self, args: Vec<&str>) -> Result<Vec<&str>> {
        Ok(args)
    }
}
