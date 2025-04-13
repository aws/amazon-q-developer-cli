use std::io::Write;

use crossterm::{
    queue,
    style::{self, Color},
};
use eyre::{Result, eyre};
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::ChatState;
use crate::cli::chat::QueuedTool;

/// Handler for the profile delete command
pub struct DeleteProfileCommand {
    name: String,
}

impl DeleteProfileCommand {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl CommandHandler for DeleteProfileCommand {
    fn name(&self) -> &'static str {
        "delete"
    }
    
    fn description(&self) -> &'static str {
        "Delete the specified profile"
    }
    
    fn usage(&self) -> &'static str {
        "/profile delete <profile_name>"
    }
    
    fn help(&self) -> String {
        "Delete the specified profile. This will remove the profile and all its associated context files.".to_string()
    }
    
    fn execute(
        &self, 
        _args: Vec<&str>, 
        ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        // Check if name is provided
        if self.name.is_empty() {
            return Err(eyre!("Profile name cannot be empty. Usage: {}", self.usage()));
        }
        
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
        
        // Delete the profile
        match context_manager.delete_profile(&self.name).await {
            Ok(_) => {
                // Success message
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("Profile '{}' deleted successfully\n", self.name)),
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
    
    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Deleting a profile should require confirmation
    }
}
