use std::sync::Arc;

use fig_os_shim::Context;

use crate::cli::chat::conversation_state::ConversationState;

/// Create a test context for unit tests
pub fn create_test_context() -> Arc<Context> {
    Context::new()
}

/// Extension trait to add test-specific methods to Context
pub trait TestContextExt {
    /// Set the conversation state in the context
    fn with_conversation_state(self, conversation_state: ConversationState) -> Self;

    /// Get the stdout content as a string
    fn stdout_content(&self) -> String;
}

impl TestContextExt for Arc<Context> {
    fn with_conversation_state(self, _conversation_state: ConversationState) -> Self {
        // In a real implementation, we would store the conversation state in the context
        self
    }

    fn stdout_content(&self) -> String {
        // In a real implementation, we would get the stdout content from the context
        String::new()
    }
}
