use crate::cli::chat::message::{UserMessage, AssistantMessage};
use super::conversation_entry::ConversationEntry;

/// Manages conversation history as alternating messages
#[derive(Debug, Clone)]
pub struct ConversationHistory {
    entries: Vec<ConversationEntry>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an input message to the conversation
    pub fn push_input_message(&mut self, content: String) {
        self.entries.push(ConversationEntry::new_user(
            UserMessage::new_prompt(content, None)
        ));
    }

    /// Add an assistant response to the conversation
    pub fn push_assistant_message(&mut self, assistant: AssistantMessage) {
        self.entries.push(ConversationEntry::new_assistant(assistant));
    }

    /// Get all conversation entries
    pub fn get_entries(&self) -> &[ConversationEntry] {
        &self.entries
    }

    /// Get number of entries (individual messages, not turns)
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ConversationHistory {
    fn default() -> Self {
        Self::new()
    }
}
