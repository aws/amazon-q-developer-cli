/// Shared utilities for UI implementations
///
/// This module provides common functionality used by different UI implementations,
/// such as token usage calculation and context information formatting.

use crate::agent_env::{Worker, WorkerLifecycleState};
use crate::agent_env::context_container::ConversationEntry;

/// Token usage statistics for a worker
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

/// Estimate token count for text
///
/// Simple estimation: approximately 4 characters per token
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Calculate token usage for a worker
///
/// Iterates through the conversation history and estimates token counts
/// for input (user) and output (assistant) messages.
pub fn calculate_token_usage(worker: &Worker) -> TokenUsage {
    let history = worker.context_container
        .conversation_history
        .lock()
        .unwrap();
    
    let mut input_tokens = 0;
    let mut output_tokens = 0;
    
    for entry in history.get_entries() {
        // Count user message tokens if present
        if let Some(user_msg) = &entry.user {
            if let Some(prompt) = user_msg.prompt() {
                input_tokens += estimate_tokens(prompt);
            }
        }
        
        // Count assistant message tokens if present
        if let Some(asst_msg) = &entry.assistant {
            output_tokens += estimate_tokens(asst_msg.content());
        }
    }
    
    TokenUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens + output_tokens,
    }
}

/// Format context information for display
///
/// Returns a formatted string with worker name, message count, and lifecycle state.
pub fn format_context_info(worker: &Worker) -> String {
    let history = worker.context_container
        .conversation_history
        .lock()
        .unwrap();
    
    let lifecycle_state = worker.lifecycle_state.lock().unwrap();
    
    format!(
        "Worker: {}\nMessages: {}\nState: {:?}",
        worker.name,
        history.len(),
        *lifecycle_state
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_env::{ContextContainer, WorkerStates};
    use crate::cli::chat::message::AssistantMessage;
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn create_test_worker() -> Worker {
        Worker {
            id: Uuid::new_v4(),
            name: "test_worker".to_string(),
            lifecycle_state: Arc::new(Mutex::new(WorkerLifecycleState::Idle)),
            task_metadata: Arc::new(Mutex::new(HashMap::new())),
            context_container: ContextContainer::new(),
            model_provider: None,
            os: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(WorkerStates::Inactive)),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("test"), 1); // 4 chars = 1 token
        assert_eq!(estimate_tokens("hello world"), 2); // 11 chars = 2 tokens
        assert_eq!(estimate_tokens("a".repeat(100).as_str()), 25); // 100 chars = 25 tokens
    }

    #[test]
    fn test_calculate_token_usage_empty() {
        let worker = create_test_worker();
        let usage = calculate_token_usage(&worker);
        
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_calculate_token_usage_with_messages() {
        let worker = create_test_worker();
        
        // Add some messages
        {
            let mut history = worker.context_container
                .conversation_history
                .lock()
                .unwrap();
            
            history.push_input_message("Hello, how are you?".to_string()); // 19 chars = 4 tokens
            history.push_assistant_message(AssistantMessage::new_response(
                None,
                "I'm doing well, thank you!".to_string() // 27 chars = 6 tokens
            ));
        }
        
        let usage = calculate_token_usage(&worker);
        
        // "Hello, how are you?" = 19 chars = 4 tokens
        // "I'm doing well, thank you!" = 27 chars = 6 tokens
        assert_eq!(usage.input_tokens, 4);
        assert_eq!(usage.output_tokens, 6);
        assert_eq!(usage.total_tokens, 10);
    }

    #[test]
    fn test_format_context_info() {
        let worker = create_test_worker();
        
        // Add a message
        {
            let mut history = worker.context_container
                .conversation_history
                .lock()
                .unwrap();
            
            history.push_input_message("Test message".to_string());
        }
        
        let info = format_context_info(&worker);
        
        assert!(info.contains("Worker: test_worker"));
        assert!(info.contains("Messages: 1"));
        assert!(info.contains("State: Idle"));
    }

    #[test]
    fn test_format_context_info_busy_state() {
        let worker = create_test_worker();
        
        // Set worker to Busy state
        {
            let mut state = worker.lifecycle_state.lock().unwrap();
            *state = WorkerLifecycleState::Busy;
        }
        
        let info = format_context_info(&worker);
        
        assert!(info.contains("State: Busy"));
    }
}
