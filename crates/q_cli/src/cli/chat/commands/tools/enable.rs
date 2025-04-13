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

/// Handler for the tools enable command
pub struct EnableToolCommand {
    tool_name: String,
}

impl EnableToolCommand {
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
        }
    }
}

impl CommandHandler for EnableToolCommand {
    fn name(&self) -> &'static str {
        "enable"
    }
    
    fn description(&self) -> &'static str {
        "Enable a specific tool"
    }
    
    fn usage(&self) -> &'static str {
        "/tools enable <tool_name>"
    }
    
    fn help(&self) -> String {
        "Enable a specific tool to allow Amazon Q to use it during the chat session.".to_string()
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
        
        // Check if the tool is already enabled
        if tool_settings.is_tool_enabled(&self.tool_name) {
            queue!(
                stdout,
                style::SetForegroundColor(Color::Yellow),
                style::Print(format!("Tool '{}' is already enabled\n", self.tool_name)),
                style::ResetColor
            )?;
            stdout.flush()?;
            return Ok(ChatState::PromptUser {
                tool_uses,
                pending_tool_index,
                skip_printing_tools: true,
            });
        }
        
        // Enable the tool
        tool_settings.enable_tool(&self.tool_name);
        
        // Save the updated settings
        conversation_state.set_tool_settings(tool_settings)?;
        
        // Success message
        queue!(
            stdout,
            style::SetForegroundColor(Color::Green),
            style::Print(format!("Tool '{}' has been enabled\n", self.tool_name)),
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
