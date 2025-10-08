use crate::cli::chat::message::{UserMessage, AssistantMessage};

/// A single message in the conversation (either user input or assistant response)
#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub user: Option<UserMessage>,
    pub assistant: Option<AssistantMessage>,
}

impl ConversationEntry {
    pub fn new_user(user: UserMessage) -> Self {
        Self { user: Some(user), assistant: None }
    }
    
    pub fn new_assistant(assistant: AssistantMessage) -> Self {
        Self { user: None, assistant: Some(assistant) }
    }
}
