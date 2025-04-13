use std::io::Write;

use crossterm::{
    queue,
    style::{self, Color},
};
use eyre::{Result, eyre};
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::conversation_state::ChatState;
use crate::cli::chat::QueuedTool;

/// Handler for the profile rename command
pub struct RenameProfileCommand {
    old_name: String,
    new_name: String,
}

impl RenameProfileCommand {
    pub fn new(old_name: &str, new_name: &str) -> Self {
        Self {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        }
    }
}

impl CommandHandler for RenameProfileCommand {
    fn name(&self) -> &'static str {
        "rename"
    }
    
    fn description(&self) -> &'static str {
        "Rename a profile"
    }
    
    fn usage(&self) -> &'static str {
        "/profile rename <old_profile_name> <new_profile_name>"
    }
    
    fn help(&self) -> String {
        "Rename a profile from <old_profile_name> to <new_profile_name>.".to_string()
    }
    
    fn execute(
        &self, 
        _args: Vec<&str>, 
        ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        // Check if names are provided
        if self.old_name.is_empty() || self.new_name.is_empty() {
            return Err(eyre!("Profile names cannot be empty. Usage: {}", self.usage()));
        }
        
        // Get the conversation state from the context
        let mut stdout = ctx.stdout();
        let conversation_state = ctx.get_conversation_state()?;
        
        // Get the context manager
        let Some(context_manager) = &mut conversation_state.context_manager else {
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
        
        // Rename the profile
        match context_manager.rename_profile(&self.old_name, &self.new_name).await {
            Ok(_) => {
                // Success message
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("Renamed profile '{}' to '{}'\n", self.old_name, self.new_name)),
                    style::ResetColor
                )?;
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
