use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::ChatState;
use crate::cli::chat::QueuedTool;

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
        "Manage profiles"
    }
    
    fn usage(&self) -> &'static str {
        "/profile [subcommand]"
    }
    
    fn help(&self) -> String {
        "Profile commands help:
/profile list - List available profiles
/profile create <name> - Create a new profile
/profile delete <name> - Delete a profile
/profile set <name> - Switch to a profile
/profile rename <old> <new> - Rename a profile".to_string()
    }
    
    fn execute(
        &self, 
        args: Vec<&str>, 
        _ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        if args.is_empty() || args[0] == "list" {
            // TODO: Implement profile listing
            println!("Available profiles: [Profile list would appear here]");
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: false,
            });
        }
        
        match args[0] {
            "create" => {
                if args.len() < 2 {
                    println!("To create a profile, please specify a name. For example: /profile create work");
                } else {
                    // TODO: Implement profile creation
                    println!("Created profile: {}", args[1]);
                }
            },
            "delete" => {
                if args.len() < 2 {
                    println!("To delete a profile, please specify the profile name. For example: /profile delete work");
                } else {
                    // TODO: Implement profile deletion
                    println!("Deleted profile: {}", args[1]);
                }
            },
            "set" => {
                if args.len() < 2 {
                    println!("To switch profiles, please specify the profile name. For example: /profile set work");
                } else {
                    // TODO: Implement profile switching
                    println!("Switched to profile: {}", args[1]);
                }
            },
            "rename" => {
                if args.len() < 3 {
                    println!("To rename a profile, please specify the old and new names. For example: /profile rename work business");
                } else {
                    // TODO: Implement profile renaming
                    println!("Renamed profile '{}' to '{}'", args[1], args[2]);
                }
            },
            "help" => {
                println!("{}", self.help());
            },
            _ => {
                println!("Unknown profile subcommand: {}. Available subcommands: list, create, delete, set, rename, help", args[0]);
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
    fn test_profile_command_help() {
        let command = ProfileCommand::new();
        assert!(command.help().contains("list"));
        assert!(command.help().contains("create"));
        assert!(command.help().contains("delete"));
        assert!(command.help().contains("set"));
        assert!(command.help().contains("rename"));
    }
}
