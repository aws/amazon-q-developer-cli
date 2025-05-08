use std::future::Future;
use std::pin::Pin;

use super::{
    CommandContextAdapter,
    CommandHandler,
};
use crate::cli::chat::command::Command;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Compact command handler
pub struct CompactCommand;

// Create a static instance of the handler
pub static COMPACT_HANDLER: CompactCommand = CompactCommand;

impl CompactCommand {
    /// Create a new compact command handler
    pub fn new() -> Self {
        Self
    }
}

impl Default for CompactCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandler for CompactCommand {
    fn name(&self) -> &'static str {
        "compact"
    }

    fn description(&self) -> &'static str {
        "Summarize the conversation to free up context space"
    }

    fn usage(&self) -> &'static str {
        "/compact [prompt] [--summary]"
    }

    fn help(&self) -> String {
        "Summarize the conversation history to free up context space while preserving essential information.\n\
        This is useful for long-running conversations that may eventually reach memory constraints.\n\n\
        Usage:\n\
        /compact                   Summarize the conversation and clear history\n\
        /compact [prompt]          Provide custom guidance for summarization\n\
        /compact --summary         Show the summary after compacting"
            .to_string()
    }

    fn to_command(&self, args: Vec<&str>) -> Result<Command, ChatError> {
        // Parse arguments to determine if this is a help request, has a custom prompt, or shows summary
        let mut prompt = None;
        let mut show_summary = false;
        let mut help = false;

        for arg in args {
            match arg {
                "--summary" => show_summary = true,
                "help" => help = true,
                _ => prompt = Some(arg.to_string()),
            }
        }

        Ok(Command::Compact {
            prompt,
            show_summary,
            help,
        })
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        _ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let Command::Compact {
                prompt,
                show_summary,
                help,
            } = command
            {
                // Return CompactHistory state directly
                Ok(ChatState::CompactHistory {
                    tool_uses,
                    pending_tool_index,
                    prompt: prompt.clone(),
                    show_summary: *show_summary,
                    help: *help,
                })
            } else {
                Err(ChatError::Custom(
                    "CompactCommand can only execute Compact commands".into(),
                ))
            }
        })
    }

    // Override the default execute implementation because compact command
    // needs to return ChatState::CompactHistory instead of ChatState::ExecuteCommand
    fn execute<'a>(
        &'a self,
        args: Vec<&'a str>,
        _ctx: &'a mut CommandContextAdapter<'a>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse arguments to determine if this is a help request, has a custom prompt, or shows summary
            let mut prompt = None;
            let mut show_summary = false;
            let mut help = false;

            for arg in args {
                match arg {
                    "--summary" => show_summary = true,
                    "help" => help = true,
                    _ => prompt = Some(arg.to_string()),
                }
            }

            // Return CompactHistory state directly instead of ExecuteCommand
            Ok(ChatState::CompactHistory {
                tool_uses,
                pending_tool_index,
                prompt,
                show_summary,
                help,
            })
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        true // Compact command requires confirmation as it's mutative
    }
}
