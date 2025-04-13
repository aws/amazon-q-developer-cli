use std::io::{
    self,
    Write,
};

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::conversation_state::ConversationState;

/// Extension trait to add required methods to Context for command handlers
pub trait ContextExt {
    /// Get a writer for stdout that implements Send
    fn stdout(&self) -> Box<dyn Write + Send>;

    /// Get the current conversation state
    fn get_conversation_state(&self) -> Result<&mut ConversationState>;
}

impl ContextExt for Context {
    fn stdout(&self) -> Box<dyn Write + Send> {
        // Return a thread-safe stdout wrapper
        Box::new(io::stdout())
    }

    fn get_conversation_state(&self) -> Result<&mut ConversationState> {
        // This is a placeholder implementation
        // In the real implementation, we would get the conversation state from the context
        Err(eyre::eyre!("ConversationState not available in this context"))
    }
}

/// Extension trait for testing purposes
#[cfg(test)]
pub trait TestContextExt {
    /// Set a value in the context
    fn set_value(&self, key: &str, value: Box<dyn std::any::Any + Send + Sync>);

    /// Get a value from the context
    fn get_value(&self, key: &str) -> Option<&(dyn std::any::Any + Send + Sync)>;
}

#[cfg(test)]
impl TestContextExt for Context {
    fn set_value(&self, _key: &str, _value: Box<dyn std::any::Any + Send + Sync>) {
        // This is a placeholder implementation for tests
    }

    fn get_value(&self, _key: &str) -> Option<&(dyn std::any::Any + Send + Sync)> {
        None
    }
}
