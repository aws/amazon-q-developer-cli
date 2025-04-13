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

/// Handler for the tools list command
pub struct ListToolsCommand;

impl ListToolsCommand {
    pub fn new() -> Self {
        Self
    }
}

impl CommandHandler for ListToolsCommand {
    fn name(&self) -> &'static str {
        "list"
    }
    
    fn description(&self) -> &'static str {
        "List all available tools and their status"
    }
    
    fn usage(&self) -> &'static str {
        "/tools list"
    }
    
    fn help(&self) -> String {
        "List all available tools and their current status (enabled/disabled).".to_string()
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
        
        // Get the tool registry
        let tool_registry = conversation_state.tool_registry();
        
        // Get the tool settings
        let tool_settings = conversation_state.tool_settings();
        
        // Display header
        queue!(
            stdout,
            style::SetForegroundColor(Color::Blue),
            style::Print("Available tools:\n"),
            style::ResetColor
        )?;
        
        // Display all tools
        for tool_name in tool_registry.get_tool_names() {
            let is_enabled = tool_settings.is_tool_enabled(tool_name);
            let status_color = if is_enabled { Color::Green } else { Color::Red };
            let status_text = if is_enabled { "enabled" } else { "disabled" };
            
            queue!(
                stdout,
                style::Print("  "),
                style::Print(tool_name),
                style::Print(" - "),
                style::SetForegroundColor(status_color),
                style::Print(status_text),
                style::ResetColor,
                style::Print("\n")
            )?;
        }
        
        stdout.flush()?;
        
        Ok(ChatState::PromptUser {
            tool_uses,
            pending_tool_index,
            skip_printing_tools: true,
        })
    }
}
