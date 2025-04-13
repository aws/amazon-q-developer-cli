use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::conversation_state::ChatState;
use crate::cli::chat::QueuedTool;

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
        "Tools commands help:
/tools list - List available tools and their permission status
/tools enable <tool> - Enable a tool
/tools disable <tool> - Disable a tool
/tools trust <tool> - Trust a tool to run without confirmation
/tools untrust <tool> - Require confirmation for a tool
/tools trustall - Trust all tools to run without confirmation
/tools reset - Reset all tool permissions to defaults".to_string()
    }
    
    fn execute(
        &self, 
        args: Vec<&str>, 
        _ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        if args.is_empty() || args[0] == "list" {
            // TODO: Implement tool listing
            println!("Available tools and their permission status: [Tool list would appear here]");
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: false,
            });
        }
        
        match args[0] {
            "enable" => {
                if args.len() < 2 {
                    println!("To enable a tool, please specify the tool name. For example: /tools enable fs_write");
                } else {
                    // TODO: Implement tool enabling
                    println!("Enabled tool: {}", args[1]);
                }
            },
            "disable" => {
                if args.len() < 2 {
                    println!("To disable a tool, please specify the tool name. For example: /tools disable execute_bash");
                } else {
                    // TODO: Implement tool disabling
                    println!("Disabled tool: {}", args[1]);
                }
            },
            "trust" => {
                if args.len() < 2 {
                    println!("To trust a tool, please specify the tool name. For example: /tools trust fs_read");
                } else {
                    // TODO: Implement tool trusting
                    println!("Set tool '{}' to trusted. It will now run without confirmation.", args[1]);
                }
            },
            "untrust" => {
                if args.len() < 2 {
                    println!("To untrust a tool, please specify the tool name. For example: /tools untrust fs_write");
                } else {
                    // TODO: Implement tool untrusting
                    println!("Set tool '{}' to require confirmation before each use.", args[1]);
                }
            },
            "trustall" => {
                // TODO: Implement trusting all tools
                println!("Set all tools to trusted. They will now run without confirmation.");
            },
            "reset" => {
                // TODO: Implement resetting tool permissions
                println!("Reset all tool permissions to their default values.");
            },
            "help" => {
                println!("{}", self.help());
            },
            _ => {
                println!("Unknown tools subcommand: {}. Available subcommands: list, enable, disable, trust, untrust, trustall, reset, help", args[0]);
            }
        }
        
        Ok(ChatState::PromptUser {
            tool_uses,
            pending_tool_index,
            skip_printing_tools: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tools_command_help() {
        let command = ToolsCommand::new();
        assert!(command.help().contains("list"));
        assert!(command.help().contains("enable"));
        assert!(command.help().contains("disable"));
        assert!(command.help().contains("trust"));
        assert!(command.help().contains("untrust"));
        assert!(command.help().contains("trustall"));
        assert!(command.help().contains("reset"));
    }
}
