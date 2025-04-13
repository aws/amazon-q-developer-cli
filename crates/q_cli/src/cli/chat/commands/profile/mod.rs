mod list;
mod create;
mod delete;
mod set;
mod rename;

use std::io::Write;

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::command::ProfileSubcommand;
use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::conversation_state::ChatState;
use crate::cli::chat::QueuedTool;

pub use list::ListProfileCommand;
pub use create::CreateProfileCommand;
pub use delete::DeleteProfileCommand;
pub use set::SetProfileCommand;
pub use rename::RenameProfileCommand;

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
        "(Beta) Manage profiles for the chat session"
    }
    
    fn usage(&self) -> &'static str {
        "/profile [subcommand]"
    }
    
    fn help(&self) -> String {
        crate::cli::chat::command::ProfileSubcommand::help_text()
    }
    
    fn execute(
        &self, 
        args: Vec<&str>, 
        ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        if args.is_empty() {
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }
        
        let subcommand = match args[0] {
            "list" => ListProfileCommand::new(),
            "create" => {
                if args.len() < 2 {
                    return Ok(ChatState::DisplayHelp {
                        help_text: format!("Usage: {}", ProfileSubcommand::CREATE_USAGE),
                        tool_uses,
                        pending_tool_index,
                    });
                }
                CreateProfileCommand::new(args[1])
            },
            "delete" => {
                if args.len() < 2 {
                    return Ok(ChatState::DisplayHelp {
                        help_text: format!("Usage: {}", ProfileSubcommand::DELETE_USAGE),
                        tool_uses,
                        pending_tool_index,
                    });
                }
                DeleteProfileCommand::new(args[1])
            },
            "set" => {
                if args.len() < 2 {
                    return Ok(ChatState::DisplayHelp {
                        help_text: format!("Usage: {}", ProfileSubcommand::SET_USAGE),
                        tool_uses,
                        pending_tool_index,
                    });
                }
                SetProfileCommand::new(args[1])
            },
            "rename" => {
                if args.len() < 3 {
                    return Ok(ChatState::DisplayHelp {
                        help_text: format!("Usage: {}", ProfileSubcommand::RENAME_USAGE),
                        tool_uses,
                        pending_tool_index,
                    });
                }
                RenameProfileCommand::new(args[1], args[2])
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
