#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use fig_api_client::model::{AssistantResponseMessage, ChatMessage, FigConversationState, UserInputMessage};
    use fig_os_shim::Context;
    use mockall::predicate::*;
    use mockall::mock;

    use crate::cli::chat::conversation_state::{ConversationState, HistoryOverflowError};
    use crate::cli::chat::history_overflow_handler::HistoryOverflowHandler;
    use crate::cli::chat::input_source::InputSource;
    use crate::cli::chat::ChatState;

    // Mock the StreamingClient
    mock! {
        StreamingClient {}
        impl fig_api_client::StreamingClient {
            pub async fn send_message(&self, state: FigConversationState) -> Result<fig_api_client::model::ChatResponseStream, fig_api_client::Error>;
        }
    }

    #[tokio::test]
    async fn test_compact_history_for_overflow() {
        // Setup
        let ctx = Arc::new(Context::default());
        let mut output = Cursor::new(Vec::new());
        let mut input_source = InputSource::new(Cursor::new(b"1\n".to_vec()));
        let interactive = false;
        let mut spinner = None;
        let mut mock_client = MockStreamingClient::new();
        
        // Create a conversation state with overflow
        let mut conversation_state = ConversationState::new(
            "test_conversation".to_string(),
            None,
            Vec::new(),
            None,
        );
        
        // Add messages to the conversation state
        for i in 0..10 {
            conversation_state.append_new_user_message(format!("User message {}", i)).await;
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: format!("Assistant response {}", i),
                tool_uses: None,
            });
        }
        
        // Setup mock expectations
        mock_client
            .expect_send_message()
            .times(2)
            .returning(|_| {
                Ok(fig_api_client::model::ChatResponseStream::default())
            });
        
        // Create the handler
        let mut handler = HistoryOverflowHandler::new(
            &ctx,
            &mut output,
            &mut input_source,
            interactive,
            &mut spinner,
            &mock_client,
        );
        
        // Test the handler
        let result = handler.handle_history_overflow(&mut conversation_state).await;
        
        // Verify the result
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ChatState::HandleResponseStream(_)));
        
        // Verify the conversation state was modified
        assert!(conversation_state.extract_history().len() < 10);
    }

    #[tokio::test]
    async fn test_reset_history_for_overflow() {
        // Setup
        let ctx = Arc::new(Context::default());
        let mut output = Cursor::new(Vec::new());
        let mut input_source = InputSource::new(Cursor::new(b"2\n".to_vec()));
        let interactive = false;
        let mut spinner = None;
        let mut mock_client = MockStreamingClient::new();
        
        // Create a conversation state with overflow
        let mut conversation_state = ConversationState::new(
            "test_conversation".to_string(),
            None,
            Vec::new(),
            None,
        );
        
        // Add messages to the conversation state
        for i in 0..10 {
            conversation_state.append_new_user_message(format!("User message {}", i)).await;
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: format!("Assistant response {}", i),
                tool_uses: None,
            });
        }
        
        // Setup mock expectations
        mock_client
            .expect_send_message()
            .times(1)
            .returning(|_| {
                Ok(fig_api_client::model::ChatResponseStream::default())
            });
        
        // Create the handler
        let mut handler = HistoryOverflowHandler::new(
            &ctx,
            &mut output,
            &mut input_source,
            interactive,
            &mut spinner,
            &mock_client,
        );
        
        // Test the handler
        let result = handler.handle_history_overflow(&mut conversation_state).await;
        
        // Verify the result
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ChatState::HandleResponseStream(_)));
        
        // Verify the conversation state was cleared
        assert_eq!(conversation_state.extract_history().len(), 1);
    }

    #[tokio::test]
    async fn test_continue_with_warning() {
        // Setup
        let ctx = Arc::new(Context::default());
        let mut output = Cursor::new(Vec::new());
        let mut input_source = InputSource::new(Cursor::new(b"3\n".to_vec()));
        let interactive = false;
        let mut spinner = None;
        let mut mock_client = MockStreamingClient::new();
        
        // Create a conversation state with overflow
        let mut conversation_state = ConversationState::new(
            "test_conversation".to_string(),
            None,
            Vec::new(),
            None,
        );
        
        // Add messages to the conversation state
        for i in 0..10 {
            conversation_state.append_new_user_message(format!("User message {}", i)).await;
            conversation_state.push_assistant_message(AssistantResponseMessage {
                message_id: None,
                content: format!("Assistant response {}", i),
                tool_uses: None,
            });
        }
        
        let original_history_len = conversation_state.extract_history().len();
        
        // Setup mock expectations
        mock_client
            .expect_send_message()
            .times(1)
            .returning(|_| {
                Ok(fig_api_client::model::ChatResponseStream::default())
            });
        
        // Create the handler
        let mut handler = HistoryOverflowHandler::new(
            &ctx,
            &mut output,
            &mut input_source,
            interactive,
            &mut spinner,
            &mock_client,
        );
        
        // Test the handler
        let result = handler.handle_history_overflow(&mut conversation_state).await;
        
        // Verify the result
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), ChatState::HandleResponseStream(_)));
        
        // Verify the conversation state was not cleared
        assert!(conversation_state.extract_history().len() > original_history_len - 2);
    }
}
