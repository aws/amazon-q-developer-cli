use eyre::Result;

use super::context_container::ContextContainer;
use super::model_providers::{ConversationMessage, MessageRole, ModelRequest};
use crate::cli::chat::message::AssistantMessage;
use crate::os::Os;

pub struct ContextBuilder;

impl ContextBuilder {
    pub async fn build_request(
        context_container: &ContextContainer,
        os: &Os,
    ) -> Result<ModelRequest> {
        let messages = Self::build_messages(context_container)?;
        let system_prompt = context_container.get_agent_prompt();
        let context = Self::load_resources(context_container, os).await?;

        Ok(ModelRequest {
            messages,
            system_prompt,
            context,
            conversation_id: None,
        })
    }

    fn build_messages(context_container: &ContextContainer) -> Result<Vec<ConversationMessage>> {
        let history = context_container.conversation_history.lock().unwrap();
        let entries = history.get_entries();

        if entries.is_empty() {
            return Err(eyre::eyre!("No messages in conversation history"));
        }

        let mut messages = Vec::new();

        for entry in entries {
            if let Some(user_msg) = &entry.user {
                messages.push(ConversationMessage {
                    role: MessageRole::User,
                    content: user_msg.content_with_context(),
                });
            }

            if let Some(assistant_msg) = &entry.assistant {
                let content = match assistant_msg {
                    AssistantMessage::Response { content, .. } => content.clone(),
                    AssistantMessage::ToolUse { content, .. } => content.clone(),
                };

                messages.push(ConversationMessage {
                    role: MessageRole::Assistant,
                    content,
                });
            }
        }

        Ok(messages)
    }

    async fn load_resources(context_container: &ContextContainer, os: &Os) -> Result<Option<String>> {
        use glob::glob;
        
        let resource_refs = context_container.get_resource_references();
        
        if resource_refs.is_empty() {
            return Ok(None);
        }
        
        let mut content = String::new();
        
        for resource_ref in resource_refs {
            if !resource_ref.starts_with("file://") {
                continue;
            }
            
            let path = resource_ref.strip_prefix("file://").unwrap();
            
            if path.contains('*') {
                // Glob pattern
                match glob(path) {
                    Ok(entries) => {
                        for entry in entries {
                            match entry {
                                Ok(file_path) => {
                                    if let Ok(file_content) = os.fs.read_to_string(&file_path).await {
                                        content.push_str(&format!("\n--- {} ---\n", file_path.display()));
                                        content.push_str(&file_content);
                                        content.push('\n');
                                    }
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                    Err(_) => continue,
                }
            } else {
                // Single file
                if let Ok(file_content) = os.fs.read_to_string(path).await {
                    content.push_str(&format!("\n--- {} ---\n", path));
                    content.push_str(&file_content);
                    content.push('\n');
                }
            }
        }
        
        if content.is_empty() {
            Ok(None)
        } else {
            Ok(Some(content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::message::UserMessage;

    #[tokio::test]
    async fn test_build_request_with_full_context() {
        let os = Os::new().await.unwrap();
        let container = ContextContainer::new();
        container.set_agent_prompt("You are a helpful assistant".to_string());
        
        container.conversation_history.lock().unwrap()
            .push_input_message("Hello".to_string());
        
        let request = ContextBuilder::build_request(&container, &os).await.unwrap();
        
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, MessageRole::User);
        assert_eq!(request.system_prompt, Some("You are a helpful assistant".to_string()));
        assert_eq!(request.context, None); // No resources loaded
    }

    #[tokio::test]
    async fn test_build_request_empty_history() {
        let os = Os::new().await.unwrap();
        let container = ContextContainer::new();
        
        let result = ContextBuilder::build_request(&container, &os).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No messages"));
    }

    #[tokio::test]
    async fn test_build_request_no_agent_context() {
        let os = Os::new().await.unwrap();
        let container = ContextContainer::new();
        container.conversation_history.lock().unwrap()
            .push_input_message("Hello".to_string());
        
        let request = ContextBuilder::build_request(&container, &os).await.unwrap();
        
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.system_prompt, None);
        assert_eq!(request.context, None);
    }

    #[tokio::test]
    async fn test_build_messages_with_assistant_response() {
        let os = Os::new().await.unwrap();
        let container = ContextContainer::new();
        
        container.conversation_history.lock().unwrap()
            .push_input_message("Hello".to_string());
        container.conversation_history.lock().unwrap()
            .push_assistant_message(AssistantMessage::new_response(None, "Hi there".to_string()));
        
        let request = ContextBuilder::build_request(&container, &os).await.unwrap();
        
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, MessageRole::User);
        assert_eq!(request.messages[1].role, MessageRole::Assistant);
        assert_eq!(request.messages[1].content, "Hi there");
    }

    #[tokio::test]
    async fn test_build_messages_with_tool_use() {
        let os = Os::new().await.unwrap();
        let container = ContextContainer::new();
        
        container.conversation_history.lock().unwrap()
            .push_input_message("Run command".to_string());
        container.conversation_history.lock().unwrap()
            .push_assistant_message(AssistantMessage::new_tool_use(
                None,
                "Using tool".to_string(),
                vec![],
            ));
        
        let request = ContextBuilder::build_request(&container, &os).await.unwrap();
        
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[1].content, "Using tool");
    }
}
