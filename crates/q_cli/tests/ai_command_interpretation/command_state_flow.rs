//! Integration tests for command state flow
//!
//! These tests verify that the ChatState flows correctly through the system
//! for various commands, ensuring that commands properly control the application flow.

use std::io::Write;
use std::sync::Arc;

use eyre::Result;
use fig_os_shim::{Context, ContextBuilder};
use q_cli::cli::chat::ChatState;
use q_cli::cli::chat::commands::CommandRegistry;
use q_cli::cli::chat::tools::internal_command::schema::InternalCommand;
use q_cli::cli::chat::tools::{InvokeOutput, OutputKind, Tool};

/// Test context setup for integration tests
struct TestContext {
    /// The context for command execution
    context: Arc<Context>,
    /// A buffer to capture command output
    output_buffer: Vec<u8>,
}

impl TestContext {
    /// Create a new test context
    async fn new() -> Result<Self> {
        let context = ContextBuilder::new()
            .with_test_home()
            .await?
            .build_fake();

        Ok(Self {
            context,
            output_buffer: Vec::new(),
        })
    }

    /// Execute a command via the internal_command tool
    async fn execute_via_tool(&mut self, command: InternalCommand) -> Result<InvokeOutput> {
        let tool = Tool::InternalCommand(command);
        tool.invoke(&self.context, &mut self.output_buffer).await
    }

    /// Get the captured output as a string
    fn get_output(&self) -> String {
        String::from_utf8_lossy(&self.output_buffer).to_string()
    }

    /// Clear the output buffer
    fn clear_output(&mut self) {
        self.output_buffer.clear();
    }
}

/// Helper function to create an InternalCommand from a command string
fn create_command(command_str: &str) -> InternalCommand {
    // Parse the command string to extract command, subcommand, and args
    let parts: Vec<&str> = command_str.trim_start_matches('/').split_whitespace().collect();
    
    let command = parts[0].to_string();
    let mut subcommand = None;
    let mut args = None;
    
    if parts.len() > 1 {
        subcommand = Some(parts[1].to_string());
        
        if parts.len() > 2 {
            args = Some(parts[2..].iter().map(|s| s.to_string()).collect());
        }
    }
    
    InternalCommand {
        command,
        subcommand,
        args,
        flags: None,
        tool_use_id: Some("test-id".to_string()),
    }
}

#[tokio::test]
async fn test_exit_command_returns_exit_state() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute quit command via tool
    let tool_command = create_command("/quit");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Verify that the next_state is set to Exit
    match tool_result.next_state {
        Some(ChatState::Exit) => {
            // This is the expected state
            println!("Exit command correctly returned Exit state");
        },
        None => panic!("Expected next_state to be set to Exit, but it was None"),
        Some(other) => panic!("Expected Exit state, got {:?}", other),
    }
    
    // Verify the output message
    let output = test_ctx.get_output();
    assert!(output.contains("Executing command: /quit"), "Output should contain execution message");
    
    // Verify the tool result message
    match tool_result.output {
        OutputKind::Text(text) => {
            assert!(text.contains("exit the application"), "Tool result should mention exiting");
        },
        _ => panic!("Expected text output"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_help_command_returns_promptuser_state() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute help command via tool
    let tool_command = create_command("/help");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Verify that the next_state is set to PromptUser
    match tool_result.next_state {
        Some(ChatState::PromptUser { .. }) => {
            // This is the expected state
            println!("Help command correctly returned PromptUser state");
        },
        None => panic!("Expected next_state to be set to PromptUser, but it was None"),
        Some(other) => panic!("Expected PromptUser state, got {:?}", other),
    }
    
    // Verify the output message
    let output = test_ctx.get_output();
    assert!(output.contains("Executing command: /help"), "Output should contain execution message");
    
    // Verify the tool result message
    match tool_result.output {
        OutputKind::Text(text) => {
            assert!(text.contains("Help information has been displayed"), 
                   "Tool result should mention help being displayed");
        },
        _ => panic!("Expected text output"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_clear_command_returns_promptuser_state() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute clear command via tool
    let tool_command = create_command("/clear");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Verify that the next_state is set to PromptUser
    match tool_result.next_state {
        Some(ChatState::PromptUser { skip_printing_tools, .. }) => {
            // This is the expected state
            assert!(skip_printing_tools, "skip_printing_tools should be true for clear command");
            println!("Clear command correctly returned PromptUser state with skip_printing_tools=true");
        },
        None => panic!("Expected next_state to be set to PromptUser, but it was None"),
        Some(other) => panic!("Expected PromptUser state, got {:?}", other),
    }
    
    // Verify the output message
    let output = test_ctx.get_output();
    assert!(output.contains("Executing command: /clear"), "Output should contain execution message");
    
    // Verify the tool result message
    match tool_result.output {
        OutputKind::Text(text) => {
            assert!(text.contains("cleared"), "Tool result should mention clearing conversation");
        },
        _ => panic!("Expected text output"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_context_command_returns_promptuser_state() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute context show command via tool
    let tool_command = create_command("/context show");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Verify that the next_state is set to PromptUser
    match tool_result.next_state {
        Some(ChatState::PromptUser { .. }) => {
            // This is the expected state
            println!("Context command correctly returned PromptUser state");
        },
        None => panic!("Expected next_state to be set to PromptUser, but it was None"),
        Some(other) => panic!("Expected PromptUser state, got {:?}", other),
    }
    
    // Verify the output message
    let output = test_ctx.get_output();
    assert!(output.contains("Executing command: /context show"), 
           "Output should contain execution message");
    
    // Verify the tool result message
    match tool_result.output {
        OutputKind::Text(text) => {
            assert!(text.contains("Successfully executed command"), 
                   "Tool result should indicate successful execution");
        },
        _ => panic!("Expected text output"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_profile_command_returns_promptuser_state() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute profile list command via tool
    let tool_command = create_command("/profile list");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Verify that the next_state is set to PromptUser
    match tool_result.next_state {
        Some(ChatState::PromptUser { .. }) => {
            // This is the expected state
            println!("Profile command correctly returned PromptUser state");
        },
        None => panic!("Expected next_state to be set to PromptUser, but it was None"),
        Some(other) => panic!("Expected PromptUser state, got {:?}", other),
    }
    
    // Verify the output message
    let output = test_ctx.get_output();
    assert!(output.contains("Executing command: /profile list"), 
           "Output should contain execution message");
    
    // Verify the tool result message
    match tool_result.output {
        OutputKind::Text(text) => {
            assert!(text.contains("Successfully executed command"), 
                   "Tool result should indicate successful execution");
        },
        _ => panic!("Expected text output"),
    }
    
    Ok(())
}

#[tokio::test]
async fn test_tools_command_returns_promptuser_state() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute tools command via tool
    let tool_command = create_command("/tools");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Verify that the next_state is set to PromptUser
    match tool_result.next_state {
        Some(ChatState::PromptUser { .. }) => {
            // This is the expected state
            println!("Tools command correctly returned PromptUser state");
        },
        None => panic!("Expected next_state to be set to PromptUser, but it was None"),
        Some(other) => panic!("Expected PromptUser state, got {:?}", other),
    }
    
    // Verify the output message
    let output = test_ctx.get_output();
    assert!(output.contains("Executing command: /tools"), 
           "Output should contain execution message");
    
    // Verify the tool result message
    match tool_result.output {
        OutputKind::Text(text) => {
            assert!(text.contains("Successfully executed command"), 
                   "Tool result should indicate successful execution");
        },
        _ => panic!("Expected text output"),
    }
    
    Ok(())
}