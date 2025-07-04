// conversation.rs
// Conversation model for Amazon Q CLI automatic naming feature

use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};

/// Represents a message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender (user or assistant)
    pub role: String,
    
    /// Content of the message
    pub content: String,
    
    /// Timestamp when the message was created
    pub timestamp: u64,
    
    /// Optional metadata for the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

/// Metadata for a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Model used for assistant messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    
    /// Tool calls made during the message
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCall>,
    
    /// Any additional metadata as key-value pairs
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub additional: std::collections::HashMap<String, String>,
}

/// Represents a tool call in a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Name of the tool
    pub name: String,
    
    /// Arguments passed to the tool
    pub arguments: String,
    
    /// Result of the tool call
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// Represents a conversation between a user and the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique identifier for the conversation
    pub id: String,
    
    /// Messages in the conversation
    pub messages: Vec<Message>,
    
    /// Timestamp when the conversation was created
    pub created_at: u64,
    
    /// Timestamp when the conversation was last updated
    pub updated_at: u64,
    
    /// Metadata for the conversation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ConversationMetadata>,
}

/// Metadata for a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMetadata {
    /// Title of the conversation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    
    /// Model used for the conversation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    
    /// Any additional metadata as key-value pairs
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", default)]
    pub additional: std::collections::HashMap<String, String>,
}

impl Conversation {
    /// Create a new conversation
    pub fn new(id: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        Self {
            id,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: None,
        }
    }
    
    /// Add a user message to the conversation
    pub fn add_user_message(&mut self, content: String) -> &mut Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        self.messages.push(Message {
            role: "user".to_string(),
            content,
            timestamp: now,
            metadata: None,
        });
        
        self.updated_at = now;
        self
    }
    
    /// Add an assistant message to the conversation
    pub fn add_assistant_message(&mut self, content: String, model: Option<String>) -> &mut Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let metadata = model.map(|m| MessageMetadata {
            model: Some(m),
            tool_calls: Vec::new(),
            additional: std::collections::HashMap::new(),
        });
        
        self.messages.push(Message {
            role: "assistant".to_string(),
            content,
            timestamp: now,
            metadata,
        });
        
        self.updated_at = now;
        self
    }
    
    /// Set the title of the conversation
    pub fn set_title(&mut self, title: String) -> &mut Self {
        if let Some(metadata) = &mut self.metadata {
            metadata.title = Some(title);
        } else {
            self.metadata = Some(ConversationMetadata {
                title: Some(title),
                model: None,
                additional: std::collections::HashMap::new(),
            });
        }
        self
    }
    
    /// Set the model for the conversation
    pub fn set_model(&mut self, model: String) -> &mut Self {
        if let Some(metadata) = &mut self.metadata {
            metadata.model = Some(model);
        } else {
            self.metadata = Some(ConversationMetadata {
                title: None,
                model: Some(model),
                additional: std::collections::HashMap::new(),
            });
        }
        self
    }
    
    /// Get all user messages in the conversation
    pub fn user_messages(&self) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.role == "user")
            .collect()
    }
    
    /// Get all assistant messages in the conversation
    pub fn assistant_messages(&self) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.role == "assistant")
            .collect()
    }
    
    /// Get the combined content of all user messages
    pub fn user_content(&self) -> String {
        self.user_messages()
            .iter()
            .map(|m| m.content.clone())
            .collect::<Vec<String>>()
            .join("\n\n")
    }
    
    /// Get the combined content of all assistant messages
    pub fn assistant_content(&self) -> String {
        self.assistant_messages()
            .iter()
            .map(|m| m.content.clone())
            .collect::<Vec<String>>()
            .join("\n\n")
    }
    
    /// Get the first few user messages (for topic extraction)
    pub fn first_user_messages(&self, count: usize) -> Vec<&Message> {
        self.user_messages()
            .into_iter()
            .take(count)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_conversation() {
        let conv = Conversation::new("test-id".to_string());
        assert_eq!(conv.id, "test-id");
        assert!(conv.messages.is_empty());
        assert!(conv.created_at > 0);
        assert_eq!(conv.created_at, conv.updated_at);
    }
    
    #[test]
    fn test_add_messages() {
        let mut conv = Conversation::new("test-id".to_string());
        
        conv.add_user_message("Hello".to_string())
            .add_assistant_message("Hi there!".to_string(), Some("gpt-4".to_string()));
        
        assert_eq!(conv.messages.len(), 2);
        assert_eq!(conv.messages[0].role, "user");
        assert_eq!(conv.messages[0].content, "Hello");
        assert_eq!(conv.messages[1].role, "assistant");
        assert_eq!(conv.messages[1].content, "Hi there!");
        
        if let Some(metadata) = &conv.messages[1].metadata {
            assert_eq!(metadata.model, Some("gpt-4".to_string()));
        } else {
            panic!("Assistant message should have metadata");
        }
    }
    
    #[test]
    fn test_user_and_assistant_messages() {
        let mut conv = Conversation::new("test-id".to_string());
        
        conv.add_user_message("Hello".to_string())
            .add_assistant_message("Hi there!".to_string(), None)
            .add_user_message("How are you?".to_string())
            .add_assistant_message("I'm doing well, thanks!".to_string(), None);
        
        let user_msgs = conv.user_messages();
        let assistant_msgs = conv.assistant_messages();
        
        assert_eq!(user_msgs.len(), 2);
        assert_eq!(assistant_msgs.len(), 2);
        
        assert_eq!(user_msgs[0].content, "Hello");
        assert_eq!(user_msgs[1].content, "How are you?");
        
        assert_eq!(assistant_msgs[0].content, "Hi there!");
        assert_eq!(assistant_msgs[1].content, "I'm doing well, thanks!");
    }
    
    #[test]
    fn test_content_extraction() {
        let mut conv = Conversation::new("test-id".to_string());
        
        conv.add_user_message("Hello".to_string())
            .add_assistant_message("Hi there!".to_string(), None)
            .add_user_message("How are you?".to_string())
            .add_assistant_message("I'm doing well, thanks!".to_string(), None);
        
        let user_content = conv.user_content();
        let assistant_content = conv.assistant_content();
        
        assert_eq!(user_content, "Hello\n\nHow are you?");
        assert_eq!(assistant_content, "Hi there!\n\nI'm doing well, thanks!");
    }
    
    #[test]
    fn test_first_user_messages() {
        let mut conv = Conversation::new("test-id".to_string());
        
        conv.add_user_message("First".to_string())
            .add_assistant_message("Response 1".to_string(), None)
            .add_user_message("Second".to_string())
            .add_assistant_message("Response 2".to_string(), None)
            .add_user_message("Third".to_string());
        
        let first_two = conv.first_user_messages(2);
        
        assert_eq!(first_two.len(), 2);
        assert_eq!(first_two[0].content, "First");
        assert_eq!(first_two[1].content, "Second");
    }
}
