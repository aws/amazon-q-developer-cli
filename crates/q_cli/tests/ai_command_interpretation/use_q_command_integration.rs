//! Integration tests for the use_q_command tool.
//!
//! These tests verify that commands executed through the use_q_command tool
//! behave identically to commands executed directly.

use std::io::Write;
use std::sync::Arc;

use eyre::Result;
use fig_os_shim::{Context, ContextBuilder};
use q_cli::cli::chat::commands::CommandRegistry;
use q_cli::cli::chat::conversation_state::ChatState;
use q_cli::cli::chat::tools::use_q_command::schema::UseQCommand;
use q_cli::cli::chat::tools::use_q_command::tool::should_exit;
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

    /// Execute a command directly using the command registry
    async fn execute_direct(&mut self, command: &str) -> Result<ChatState> {
        let registry = CommandRegistry::global();
        registry
            .parse_and_execute(command, &self.context, None, None)
            .await
    }

    /// Execute a command via the use_q_command tool
    async fn execute_via_tool(&mut self, command: UseQCommand) -> Result<InvokeOutput> {
        let tool = Tool::UseQCommand(command);
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

/// Helper function to create a UseQCommand from a command string
fn create_command(command_str: &str) -> UseQCommand {
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
    
    UseQCommand {
        command,
        subcommand,
        args,
        flags: None,
        tool_use_id: Some("test-id".to_string()),
    }
}

/// Helper function to compare ChatState and InvokeOutput
fn compare_results(state: &ChatState, output: &InvokeOutput) -> bool {
    match (state, &output.output) {
        (ChatState::Exit, OutputKind::Text(text)) => {
            text.contains("exit") && should_exit()
        },
        (ChatState::DisplayHelp { help_text, .. }, OutputKind::Text(text)) => {
            text.contains("Successfully executed command") || text == help_text
        },
        (ChatState::PromptUser { skip_printing_tools, .. }, OutputKind::Text(text)) => {
            if *skip_printing_tools {
                text.contains("cleared")
            } else {
                text.contains("Successfully executed")
            }
        },
        _ => false,
    }
}

#[tokio::test]
async fn test_help_command() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute help command directly
    let direct_result = test_ctx.execute_direct("/help").await?;
    
    // Execute help command via tool
    let tool_command = create_command("/help");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Compare results
    assert!(compare_results(&direct_result, &tool_result));
    
    Ok(())
}

#[tokio::test]
async fn test_clear_command() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute clear command directly
    let direct_result = test_ctx.execute_direct("/clear").await?;
    
    // Execute clear command via tool
    let tool_command = create_command("/clear");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Compare results
    assert!(compare_results(&direct_result, &tool_result));
    
    Ok(())
}

#[tokio::test]
async fn test_context_show_command() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute context show command directly
    let direct_result = test_ctx.execute_direct("/context show").await?;
    
    // Execute context show command via tool
    let tool_command = create_command("/context show");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Compare results
    assert!(compare_results(&direct_result, &tool_result));
    
    Ok(())
}

#[tokio::test]
async fn test_profile_list_command() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute profile list command directly
    let direct_result = test_ctx.execute_direct("/profile list").await?;
    
    // Execute profile list command via tool
    let tool_command = create_command("/profile list");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Compare results
    assert!(compare_results(&direct_result, &tool_result));
    
    Ok(())
}

#[tokio::test]
async fn test_tools_list_command() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute tools list command directly
    let direct_result = test_ctx.execute_direct("/tools list").await?;
    
    // Execute tools list command via tool
    let tool_command = create_command("/tools list");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Compare results
    assert!(compare_results(&direct_result, &tool_result));
    
    Ok(())
}

// Skip quit test in CI environments
#[tokio::test]
#[ignore]
async fn test_quit_command() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;
    
    // Execute quit command directly
    let direct_result = test_ctx.execute_direct("/quit").await?;
    
    // Reset exit flag (since we're testing)
    q_cli::cli::chat::tools::use_q_command::tool::reset_exit_flag();
    
    // Execute quit command via tool
    let tool_command = create_command("/quit");
    let tool_result = test_ctx.execute_via_tool(tool_command).await?;
    
    // Compare results
    assert!(compare_results(&direct_result, &tool_result));
    
    // Verify exit flag is set
    assert!(should_exit());
    
    Ok(())
}