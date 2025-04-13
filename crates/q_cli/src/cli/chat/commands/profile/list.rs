use std::io::Write;

use crossterm::{
    queue,
    style::{self, Color},
};
use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::ChatState;
use crate::cli::chat::QueuedTool;

/// Handler for the profile list command
pub struct ListProfileCommand;

impl ListProfileCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for ListProfileCommand {
    fn name(&self) -> &'static str {
        "list"
    }
    
    fn description(&self) -> &'static str {
        "List all available profiles"
    }
    
    fn usage(&self) -> &'static str {
        "/profile list"
    }
    
    fn help(&self) -> String {
        "List all available profiles for the chat session.".to_string()
    }
    
    fn execute(
        &self, 
        _args: Vec<&str>, 
        ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        // Get the conversation state from the context
        let mut stdout = ctx.stdout();
        let conversation_state = ctx.get_conversation_state()?;
        
        // Get the context manager
        let Some(context_manager) = &conversation_state.context_manager else {
            queue!(
                stdout,
                style::SetForegroundColor(Color::Red),
                style::Print("Error: Context manager not initialized\n"),
                style::ResetColor
            )?;
            stdout.flush()?;
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        };
        
        // Get the list of profiles
        match context_manager.list_profiles().await {
            Ok(profiles) => {
                // Display header
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Blue),
                    style::Print("Available profiles:\n"),
                    style::ResetColor
                )?;
                
                // Display current profile with indicator
                let current_profile = &context_manager.current_profile;
                
                // Display all profiles
                for profile in profiles {
                    if &profile == current_profile {
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Green),
                            style::Print("* "),
                            style::Print(&profile),
                            style::Print(" (current)"),
                            style::ResetColor,
                            style::Print("\n")
                        )?;
                    } else {
                        queue!(
                            stdout,
                            style::Print("  "),
                            style::Print(&profile),
                            style::Print("\n")
                        )?;
                    }
                }
                
                stdout.flush()?;
            },
            Err(e) => {
                // Error message
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("Error: {}\n", e)),
                    style::ResetColor
                )?;
                stdout.flush()?;
            }
        }
        
        Ok(ChatState::PromptUser {
            tool_uses,
            pending_tool_index,
            skip_printing_tools: true,
        })
    }
}
