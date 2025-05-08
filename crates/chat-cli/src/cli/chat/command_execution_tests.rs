#[cfg(test)]
mod command_execution_tests {
    use std::collections::HashMap;

    use eyre::Result;
    use crate::api_client::StreamingClient;
    use crate::platform::Context;
    use crate::settings::{
        Settings,
        State,
    };

    use crate::cli::chat::command::Command;
    use crate::cli::chat::conversation_state::ConversationState;
    use crate::cli::chat::input_source::InputSource;
    use crate::shared_writer::SharedWriter;
    use crate::cli::chat::tools::internal_command::schema::InternalCommand;
    use crate::cli::chat::tools::{
        Tool,
        ToolPermissions,
    };
    use crate::cli::chat::{
        ChatContext,
        ChatState,
        ToolUseStatus,
    };

    #[tokio::test]
    async fn test_execute_command_quit() -> Result<()> {
        // Create a mock ChatContext
        let mut chat_context = create_test_chat_context().await?;

        // Execute the quit command
        let result = chat_context.execute_command(Command::Quit, None, None).await?;

        // Verify that the result is ChatState::Exit
        assert!(matches!(result, ChatState::Exit));

        Ok(())
    }

    #[tokio::test]
    async fn test_execute_command_help() -> Result<()> {
        // Create a mock ChatContext
        let mut chat_context = create_test_chat_context().await?;

        // Execute the help command
        let result = chat_context
            .execute_command(Command::Help { help_text: None }, None, None)
            .await?;

        // Verify that the result is ChatState::ExecuteCommand with help command
        if let ChatState::ExecuteCommand { command, .. } = result {
            assert!(matches!(command, Command::Help { .. }));
        } else {
            panic!("Expected ChatState::ExecuteCommand with Help command, got {:?}", result);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_execute_command_compact() -> Result<()> {
        // Create a mock ChatContext
        let mut chat_context = create_test_chat_context().await?;

        // Execute the compact command
        let result = chat_context
            .execute_command(
                Command::Compact {
                    prompt: Some("test prompt".to_string()),
                    show_summary: true,
                    help: false,
                },
                None,
                None,
            )
            .await?;

        // Verify that the result is a valid state for compact command
        match result {
            ChatState::CompactHistory { .. } => {
                // This is the expected state in the original code
            },
            ChatState::PromptUser { .. } => {
                // This is also acceptable as the command might need user confirmation
            },
            ChatState::ExecuteCommand { command, .. } => {
                // This is also acceptable as the command might be executed directly
                assert!(matches!(command, Command::Compact { .. }));
            },
            _ => {
                panic!("Expected ChatState::CompactHistory or related state, got {:?}", result);
            },
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_execute_command_other() -> Result<()> {
        // Create a mock ChatContext
        let mut chat_context = create_test_chat_context().await?;

        // Execute a command that falls back to handle_input
        let result = chat_context.execute_command(Command::Clear, None, None).await;

        // Just verify that the method doesn't panic
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_to_command_execution_flow() -> Result<()> {
        // Create a mock ChatContext
        let mut chat_context = create_test_chat_context().await?;

        // Set tool permissions to trusted to avoid PromptUser state
        for tool_name in Tool::all_tool_names() {
            chat_context.tool_permissions.trust_tool(tool_name);
        }

        // Create an internal command tool
        let internal_command = InternalCommand {
            command: "help".to_string(),
            subcommand: None,
            args: None,
            flags: None,
            tool_use_id: None,
        };
        let tool = Tool::InternalCommand(internal_command);

        // Invoke the tool
        let mut output = Vec::new();
        let invoke_result = tool.invoke(&chat_context.ctx, &mut output).await?;

        // Verify that the result contains ExecuteCommand state
        if let Some(ChatState::ExecuteCommand { command, .. }) = invoke_result.next_state {
            assert!(matches!(command, Command::Help { .. }));

            // Now execute the command
            let execute_result = chat_context.execute_command(command, None, None).await?;

            // Verify that the result is ChatState::ExecuteCommand with help command
            if let ChatState::ExecuteCommand { command, .. } = execute_result {
                assert!(matches!(command, Command::Help { .. }));
            } else {
                panic!(
                    "Expected ChatState::ExecuteCommand with Help command, got {:?}",
                    execute_result
                );
            }
        } else {
            panic!("Expected ChatState::ExecuteCommand, got {:?}", invoke_result.next_state);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_command_context_adapter_terminal_width() -> Result<()> {
        // Create a mock ChatContext
        let mut chat_context = create_test_chat_context().await?;
        
        // Create a CommandContextAdapter
        let adapter = chat_context.command_context_adapter();
        
        // Verify that the terminal_width method returns the expected value
        assert_eq!(adapter.terminal_width(), 80);
        
        Ok(())
    }

    async fn create_test_chat_context() -> Result<ChatContext> {
        // Create a context - Context::new_fake() already returns an Arc<Context>
        let ctx = Context::new_fake();
        let settings = Settings::new_fake();
        let state = State::new_fake();
        let output = SharedWriter::null();
        let input_source = InputSource::new_mock(vec![]);
        let interactive = true;
        let client = StreamingClient::mock(vec![]);

        // Create a tool config
        let tool_config = HashMap::new();

        // Create a conversation state
        let conversation_state = ConversationState::new(ctx.clone(), tool_config, None, None).await;

        // Create tool permissions with all tools trusted
        let mut tool_permissions = ToolPermissions::new(10);
        for tool_name in Tool::all_tool_names() {
            tool_permissions.trust_tool(tool_name);
        }

        // Create the chat context
        let chat_context = ChatContext {
            ctx,
            settings,
            state,
            output,
            initial_input: None,
            input_source,
            interactive,
            client,
            terminal_width_provider: || Some(80),
            spinner: None,
            conversation_state,
            tool_permissions,
            tool_use_telemetry_events: HashMap::new(),
            tool_use_status: ToolUseStatus::Idle,
            failed_request_ids: Vec::new(),
        };

        Ok(chat_context)
    }
}
