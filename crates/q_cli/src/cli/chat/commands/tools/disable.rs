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

/// Handler for the tools disable command
pub struct DisableToolCommand {
    tool_name: String,
}

impl DisableToolCommand {
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
        }
    }
}

impl CommandHandler for DisableToolCommand {
    fn name(&self) -> &'static str {
        "disable"
    }
    
    fn description(&self) -> &'static str {
        "Disable a specific tool"
    }
    
    fn usage(&self) -> &'static str {
        "/tools disable <tool_name>"
    }
    
    fn help(&self) -> String {
        "Disable a specific tool to prevent Amazon Q from using it during the chat session.".to_string()
    }
    
    fn execute(
        &self, 
        _args: Vec<&str>, 
        ctx: &Context,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        // Check if tool name is provided
        if self.tool_name.is_empty() {
            return Err(eyre!("Tool name cannot be empty. Usage: {}", self.usage()));
        }
        
        // Get the conversation state from the context
        let mut stdout = ctx.stdout();
        let conversation_state = ctx.get_conversation_state()?;
        
        // Get the tool registry to check if the tool exists
        let tool_registry = conversation_state.tool_registry();
        
        // Check if the tool exists
        if !tool_registry.get_tool_names().contains(&self.tool_name) {
            queue!(
                stdout,
                style::SetForegroundColor(Color::Red),
                style::Print(format!("Error: Tool '{}' does not exist\n", self.tool_name)),
                style::ResetColor
            )?;
            stdout.flush()?;
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }
        
        // Get the tool settings
        let mut tool_settings = conversation_state.tool_settings().clone();
        
        // Check if the tool is already disabled
        if !tool_settings.is_tool_enabled(&self.tool_name) {
            queue!(
                stdout,
                style::SetForegroundColor(Color::Yellow),
                style::Print(format!("Tool '{}' is already disabled\n", self.tool_name)),
                style::ResetColor
            )?;
            stdout.flush()?;
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }
        
        // Disable the tool
        tool_settings.disable_tool(&self.tool_name);
        
        // Save the updated settings
        conversation_state.set_tool_settings(tool_settings)?;
        
        // Success message
        queue!(
            stdout,
            style::SetForegroundColor(Color::Green),
            style::Print(format!("Tool '{}' has been disabled\n", self.tool_name)),
            style::ResetColor
        )?;
        stdout.flush()?;
        
        Ok(ChatState::PromptUser {
            tool_uses,
            pending_tool_index,
            skip_printing_tools: true,
        })
    }
}
