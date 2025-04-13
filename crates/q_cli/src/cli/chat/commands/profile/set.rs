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

/// Handler for the profile set command
pub struct SetProfileCommand {
    name: String,
}

impl SetProfileCommand {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl CommandHandler for SetProfileCommand {
    fn name(&self) -> &'static str {
        "set"
    }
    
    fn description(&self) -> &'static str {
        "Switch to the specified profile"
    }
    
    fn usage(&self) -> &'static str {
        "/profile set <profile_name>"
    }
    
    fn help(&self) -> String {
        "Switch to the specified profile. This will change the active profile for the current chat session.".to_string()
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
        
        // Check if already on the specified profile
        if context_manager.current_profile == self.name {
            queue!(
                stdout,
                style::SetForegroundColor(Color::Yellow),
                style::Print(format!("Already using profile '{}'\n", self.name)),
                style::ResetColor
            )?;
            stdout.flush()?;
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }
        
        // Switch to the profile
        match context_manager.switch_profile(&self.name).await {
            Ok(_) => {
                // Success message
                queue!(
                    stdout,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!("Switched to profile '{}'\n", self.name)),
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
