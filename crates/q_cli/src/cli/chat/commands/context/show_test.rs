use std::io::Write;
use std::sync::Arc;

use eyre::Result;
use fig_os_shim::Context;

use crate::cli::chat::commands::CommandHandler;
use crate::cli::chat::commands::context::show::ShowContextCommand;
use crate::cli::chat::context::ContextExt;
use crate::cli::chat::context::ContextManager;
use crate::cli::chat::conversation_state::ConversationState;

/// Test implementation of Context with a mock ConversationState
#[derive(Debug)]
struct TestContext {
    conversation_state: ConversationState,
}

impl TestContext {
    async fn new() -> Result<Self> {
        let ctx = Context::builder().with_test_home().await?.build_fake();
        let conversation_state = ConversationState::new(Arc::new(ctx.clone()), Default::default(), None).await;
        
        // Initialize the context manager
        let mut conversation_state = conversation_state;
        let context_manager = ContextManager::new(Arc::new(ctx)).await?;
        conversation_state.context_manager = Some(context_manager);
        
        Ok(Self { conversation_state })
    }
}

impl Context for TestContext {
    fn env(&self) -> &dyn fig_os_shim::Env {
        panic!("TestContext::env not implemented for tests")
    }

    fn fs(&self) -> &dyn fig_os_shim::Fs {
        panic!("TestContext::fs not implemented for tests")
    }
}

impl ContextExt for TestContext {
    fn stdout(&self) -> Box<dyn Write + Send> {
        // Return a thread-safe stdout wrapper that discards output
        Box::new(std::io::sink())
    }

    fn get_conversation_state(&self) -> Result<&mut ConversationState> {
        // Return a mutable reference to the conversation state
        // This is unsafe but necessary for testing
        let conversation_state = &self.conversation_state as *const _ as *mut ConversationState;
        Ok(unsafe { &mut *conversation_state })
    }
}

#[tokio::test]
async fn test_show_context_command_end_to_end() -> Result<()> {
    // Create a test context with a mock conversation state
    let ctx = TestContext::new().await?;
    let ctx = Arc::new(ctx);
    
    // Create the show context command
    let command = ShowContextCommand::new(false, false);
    
    // Execute the command
    let result = command.execute(vec![], &*ctx, None, None).await?;
    
    // Verify the result is a PromptUser state
    match result {
        crate::cli::chat::ChatState::PromptUser { skip_printing_tools, .. } => {
            assert!(skip_printing_tools, "Expected skip_printing_tools to be true");
            Ok(())
        },
        _ => Err(eyre::eyre!("Expected PromptUser state, got {:?}", result)),
    }
}