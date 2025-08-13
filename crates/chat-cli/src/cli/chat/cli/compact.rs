use clap::Args;

use crate::cli::chat::consts::MAX_USER_MESSAGE_SIZE;
use crate::cli::chat::message::UserMessageContent;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
#[command(
    before_long_help = "/compact summarizes the conversation history to free up context space
while preserving essential information. This is useful for long-running conversations
that may eventually reach memory constraints.

When to use
• When you see the memory constraint warning message
• When a conversation has been running for a long time
• Before starting a new topic within the same session
• After completing complex tool operations

How it works
• Creates an AI-generated summary of your conversation
• Retains key information, code, and tool executions in the summary
• Clears the conversation history to free up space
• The assistant will reference the summary context in future responses

Compaction will be automatically performed whenever the context window overflows.
To disable this behavior, run: `q settings chat.disableAutoCompaction true`"
)]
pub struct CompactArgs {
    /// The prompt to use when generating the summary
    prompt: Vec<String>,
    #[arg(long)]
    show_summary: bool,
    /// The number of user and assistant message pairs to exclude from the summarization.
    #[arg(long)]
    messages_to_exclude: Option<usize>,
    /// Whether or not large messages should be truncated.
    #[arg(long)]
    truncate_large_messages: Option<bool>,
    /// Maximum allowed size of messages in the conversation history. Requires
    /// truncate_large_messages to be set.
    #[arg(long, requires = "truncate_large_messages")]
    max_message_length: Option<usize>,
}

impl CompactArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let default = CompactStrategy::default();
        let prompt = if self.prompt.is_empty() {
            None
        } else {
            Some(self.prompt.join(" "))
        };

        // Compact interrupts the current conversation so this will always result in a new user
        // turn.
        session.reset_user_turn();

        session
            .compact_history(os, prompt, self.show_summary, CompactStrategy {
                messages_to_exclude: self.messages_to_exclude.unwrap_or(default.messages_to_exclude),
                truncate_large_messages: self.truncate_large_messages.unwrap_or(default.truncate_large_messages),
                max_message_length: self.max_message_length.map_or(default.max_message_length, |v| {
                    v.clamp(UserMessageContent::TRUNCATED_SUFFIX.len(), MAX_USER_MESSAGE_SIZE)
                }),
            })
            .await
    }
}

/// Parameters for performing the history compaction request.
#[derive(Debug, Copy, Clone)]
pub struct CompactStrategy {
    /// Number of user/assistant pairs to exclude from the history as part of compaction.
    pub messages_to_exclude: usize,
    /// Whether or not to truncate large messages in the history.
    pub truncate_large_messages: bool,
    /// Maximum allowed size of messages in the conversation history.
    pub max_message_length: usize,
}

impl Default for CompactStrategy {
    fn default() -> Self {
        Self {
            messages_to_exclude: Default::default(),
            truncate_large_messages: Default::default(),
            max_message_length: MAX_USER_MESSAGE_SIZE,
        }
    }
}
