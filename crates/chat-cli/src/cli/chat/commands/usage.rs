use std::future::Future;
use std::pin::Pin;

use crossterm::style::Color;
use crossterm::{
    queue,
    style,
};

use super::context_adapter::CommandContextAdapter;
use super::handler::CommandHandler;
use crate::cli::chat::command::Command;
use crate::cli::chat::{
    ChatError,
    ChatState,
    QueuedTool,
};

/// Command handler for the `/usage` command
pub struct UsageCommand;

// Create a static instance of the handler
pub static USAGE_HANDLER: UsageCommand = UsageCommand;

impl Default for UsageCommand {
    fn default() -> Self {
        Self
    }
}

impl UsageCommand {
    #[allow(dead_code)]
    /// Format a progress bar based on percentage
    fn format_progress_bar(percentage: f64, width: usize) -> String {
        let filled_width = ((percentage / 100.0) * width as f64).round() as usize;
        let empty_width = width.saturating_sub(filled_width);

        let filled = "█".repeat(filled_width);
        let empty = "░".repeat(empty_width);

        format!("{}{}", filled, empty)
    }

    #[allow(dead_code)]
    /// Get color based on usage percentage
    fn get_color_for_percentage(percentage: f64) -> Color {
        if percentage < 50.0 {
            Color::Green
        } else if percentage < 75.0 {
            Color::Yellow
        } else {
            Color::Red
        }
    }
}

impl CommandHandler for UsageCommand {
    fn name(&self) -> &'static str {
        "usage"
    }

    fn description(&self) -> &'static str {
        "Display token usage statistics"
    }

    fn usage(&self) -> &'static str {
        "/usage"
    }

    fn help(&self) -> String {
        color_print::cformat!(
            r#"
<magenta,em>Token Usage Statistics</magenta,em>

Displays information about the current token usage in the conversation.

<cyan!>Usage: /usage</cyan!>

<cyan!>Description</cyan!>
  Shows the number of tokens used in the conversation history,
  context files, and the remaining capacity. This helps you
  understand how much of the context window is being utilized.

<cyan!>Notes</cyan!>
• The context window has a fixed size limit
• When the window fills up, older messages may be summarized or removed
• Adding large context files can significantly reduce available space
• Use /compact to summarize conversation history and free up space
"#
        )
    }

    fn llm_description(&self) -> String {
        r#"
The usage command displays token usage statistics for the current conversation.

Usage:
- /usage

This command shows:
- Total tokens used in the conversation history
- Tokens used by context files
- Remaining token capacity
- Visual representation of token usage

This command is useful when:
- The user wants to understand how much of the context window is being used
- The user is experiencing truncated responses due to context limits
- The user wants to optimize their context usage
- The user is deciding whether to use /compact to free up space

The command provides a visual progress bar showing:
- Green: Less than 50% usage
- Yellow: Between 50-75% usage
- Red: Over 75% usage

No arguments or options are needed for this command.
"#
        .to_string()
    }

    fn to_command(&self, _args: Vec<&str>) -> Result<Command, ChatError> {
        Ok(Command::Usage)
    }

    fn execute_command<'a>(
        &'a self,
        command: &'a Command,
        ctx: &'a mut CommandContextAdapter<'a>,
        _tool_uses: Option<Vec<QueuedTool>>,
        _pending_tool_index: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<ChatState, ChatError>> + Send + 'a>> {
        Box::pin(async move {
            if let Command::Usage = command {
                // Calculate token usage statistics
                let char_count = ctx.conversation_state.calculate_char_count().await;
                let total_chars = *char_count;

                // Get conversation size details
                let backend_state = ctx.conversation_state.backend_conversation_state(false, true).await;
                let conversation_size = backend_state.calculate_conversation_size();

                // Get character counts
                let history_chars = *conversation_size.user_messages + *conversation_size.assistant_messages;
                let context_chars = *conversation_size.context_messages;

                // Convert to token counts using the TokenCounter ratio
                let max_chars = crate::cli::chat::consts::MAX_CHARS;
                let max_tokens = max_chars / 3;
                let history_tokens = history_chars / 3;
                let context_tokens = context_chars / 3;
                let total_tokens = total_chars / 3;
                let remaining_tokens = max_tokens.saturating_sub(total_tokens);

                // Calculate percentages
                let history_percentage = (history_chars as f64 / max_chars as f64) * 100.0;
                let context_percentage = (context_chars as f64 / max_chars as f64) * 100.0;
                let total_percentage = (total_chars as f64 / max_chars as f64) * 100.0;

                // Format progress bars
                let bar_width = 30;
                let history_bar = Self::format_progress_bar(history_percentage, bar_width);
                let context_bar = Self::format_progress_bar(context_percentage, bar_width);
                let total_bar = Self::format_progress_bar(total_percentage, bar_width);

                // Get colors based on usage
                let history_color = Self::get_color_for_percentage(history_percentage);
                let context_color = Self::get_color_for_percentage(context_percentage);
                let total_color = Self::get_color_for_percentage(total_percentage);

                // Display the usage statistics
                queue!(
                    ctx.output,
                    style::Print("\n📊 Token Usage Statistics\n\n"),
                    style::Print("Conversation History: "),
                    style::SetForegroundColor(history_color),
                    style::Print(format!("{} ", history_bar)),
                    style::ResetColor,
                    style::Print(format!("{} tokens ({:.1}%)\n", history_tokens, history_percentage)),
                    style::Print("Context Files:       "),
                    style::SetForegroundColor(context_color),
                    style::Print(format!("{} ", context_bar)),
                    style::ResetColor,
                    style::Print(format!("{} tokens ({:.1}%)\n", context_tokens, context_percentage)),
                    style::Print("Total Usage:         "),
                    style::SetForegroundColor(total_color),
                    style::Print(format!("{} ", total_bar)),
                    style::ResetColor,
                    style::Print(format!("{} tokens ({:.1}%)\n", total_tokens, total_percentage)),
                    style::Print(format!("\nRemaining Capacity:   {} tokens\n", remaining_tokens)),
                    style::Print(format!("Maximum Capacity:     {} tokens\n\n", max_tokens))
                )?;

                // Add a tip if usage is high
                if total_percentage > 75.0 {
                    queue!(
                        ctx.output,
                        style::SetForegroundColor(Color::Yellow),
                        style::Print("💡 Tip: Use '/compact' to summarize conversation history and free up space.\n\n"),
                        style::ResetColor
                    )?;
                }

                Ok(ChatState::PromptUser {
                    tool_uses: None,
                    pending_tool_index: None,
                    skip_printing_tools: true,
                })
            } else {
                Err(ChatError::Custom("UsageCommand can only execute Usage commands".into()))
            }
        })
    }

    fn requires_confirmation(&self, _args: &[&str]) -> bool {
        false // Usage command doesn't require confirmation
    }
}
