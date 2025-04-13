//! Integration tests for whitespace handling in use_q_command tool
//!
//! These tests verify that the use_q_command tool correctly handles file paths
//! with spaces when executing context commands.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use eyre::Result;
use fig_os_shim::{
    Context,
    ContextBuilder,
};
use q_cli::cli::chat::commands::CommandRegistry;
use q_cli::cli::chat::conversation_state::ChatState;
use q_cli::cli::chat::tools::use_q_command::schema::UseQCommand;
use q_cli::cli::chat::tools::{
    InvokeOutput,
    OutputKind,
    Tool,
};

/// Test context setup for whitespace handling tests
struct TestContext {
    /// The context for command execution
    context: Arc<Context>,
    /// A buffer to capture command output
    output_buffer: Vec<u8>,
    /// Test directory with spaces in the name
    test_dir: PathBuf,
    /// Test file with spaces in the name
    test_file: PathBuf,
}

impl TestContext {
    /// Create a new test context with files that have spaces in their names
    async fn new() -> Result<Self> {
        let context = ContextBuilder::new().with_test_home().await?.build_fake();

        // Create a test directory with spaces in the name
        let test_dir = context.fs().chroot_path("Test Directory");
        context.fs().create_dir_all(&test_dir).await?;

        // Create a test file with spaces in the name
        let test_file = test_dir.join("Test File.txt");
        context.fs().write(&test_file, "Test content").await?;

        Ok(Self {
            context,
            output_buffer: Vec::new(),
            test_dir,
            test_file,
        })
    }

    /// Execute a command directly using the command registry
    async fn execute_direct(&mut self, command: &str) -> Result<ChatState> {
        let registry = CommandRegistry::global();
        registry.parse_and_execute(command, &self.context, None, None).await
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

/// Helper function to create a UseQCommand for context add
fn create_context_add_command(file_path: &str) -> UseQCommand {
    UseQCommand {
        command: "context".to_string(),
        subcommand: Some("add".to_string()),
        args: Some(vec![file_path.to_string()]),
        flags: None,
        tool_use_id: Some("test-id".to_string()),
    }
}

/// Helper function to create a UseQCommand for context remove
fn create_context_remove_command(file_path: &str) -> UseQCommand {
    UseQCommand {
        command: "context".to_string(),
        subcommand: Some("rm".to_string()),
        args: Some(vec![file_path.to_string()]),
        flags: None,
        tool_use_id: Some("test-id".to_string()),
    }
}

#[tokio::test]
async fn test_context_add_with_spaces_direct() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;

    // Execute context add command directly with a file path containing spaces
    let file_path = test_ctx.test_file.to_string_lossy().to_string();
    let command = format!("/context add \"{}\"", file_path);

    let result = test_ctx.execute_direct(&command).await?;

    // Verify the file was added to context
    if let ChatState::PromptUser { .. } = result {
        // Now check if the file is in context
        let show_result = test_ctx.execute_direct("/context show").await?;
        if let ChatState::DisplayHelp { help_text, .. } = show_result {
            assert!(help_text.contains(&file_path), "File with spaces not found in context");
        } else {
            panic!("Expected DisplayHelp state");
        }
    } else {
        panic!("Expected PromptUser state");
    }

    Ok(())
}

#[tokio::test]
async fn test_context_add_with_spaces_via_tool() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;

    // Execute context add command via tool with a file path containing spaces
    let file_path = test_ctx.test_file.to_string_lossy().to_string();
    let command = create_context_add_command(&file_path);

    let result = test_ctx.execute_via_tool(command).await?;

    // Verify the command was executed successfully
    if let OutputKind::Text(text) = result.output {
        assert!(text.contains("Successfully executed") || text.contains("added to context"));

        // Now check if the file is in context
        let show_result = test_ctx.execute_direct("/context show").await?;
        if let ChatState::DisplayHelp { help_text, .. } = show_result {
            assert!(help_text.contains(&file_path), "File with spaces not found in context");
        } else {
            panic!("Expected DisplayHelp state");
        }
    } else {
        panic!("Expected text output");
    }

    Ok(())
}

#[tokio::test]
async fn test_context_remove_with_spaces_direct() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;

    // First add the file to context
    let file_path = test_ctx.test_file.to_string_lossy().to_string();
    let add_command = format!("/context add \"{}\"", file_path);
    test_ctx.execute_direct(&add_command).await?;

    // Execute context remove command directly with a file path containing spaces
    let remove_command = format!("/context rm \"{}\"", file_path);
    let result = test_ctx.execute_direct(&remove_command).await?;

    // Verify the file was removed from context
    if let ChatState::PromptUser { .. } = result {
        // Now check if the file is no longer in context
        let show_result = test_ctx.execute_direct("/context show").await?;
        if let ChatState::DisplayHelp { help_text, .. } = show_result {
            assert!(
                !help_text.contains(&file_path),
                "File with spaces still found in context after removal"
            );
        } else {
            panic!("Expected DisplayHelp state");
        }
    } else {
        panic!("Expected PromptUser state");
    }

    Ok(())
}

#[tokio::test]
async fn test_context_remove_with_spaces_via_tool() -> Result<()> {
    let mut test_ctx = TestContext::new().await?;

    // First add the file to context
    let file_path = test_ctx.test_file.to_string_lossy().to_string();
    let add_command = format!("/context add \"{}\"", file_path);
    test_ctx.execute_direct(&add_command).await?;

    // Execute context remove command via tool with a file path containing spaces
    let command = create_context_remove_command(&file_path);
    let result = test_ctx.execute_via_tool(command).await?;

    // Verify the command was executed successfully
    if let OutputKind::Text(text) = result.output {
        assert!(text.contains("Successfully executed") || text.contains("removed from context"));

        // Now check if the file is no longer in context
        let show_result = test_ctx.execute_direct("/context show").await?;
        if let ChatState::DisplayHelp { help_text, .. } = show_result {
            assert!(
                !help_text.contains(&file_path),
                "File with spaces still found in context after removal"
            );
        } else {
            panic!("Expected DisplayHelp state");
        }
    } else {
        panic!("Expected text output");
    }

    Ok(())
}

#[tokio::test]
async fn test_natural_language_with_spaces_in_path() -> Result<()> {
    // This test would normally use the AI to interpret a natural language query
    // Since we can't directly test the AI in unit tests, we'll simulate the AI's response

    let mut test_ctx = TestContext::new().await?;
    let file_path = test_ctx.test_file.to_string_lossy().to_string();

    // Simulate the AI interpreting "Add 'Test File.txt' to my context"
    // The AI should create a UseQCommand with the correct file path
    let command = create_context_add_command(&file_path);

    // Execute the command via the tool
    let result = test_ctx.execute_via_tool(command).await?;

    // Verify the command was executed successfully
    if let OutputKind::Text(text) = result.output {
        assert!(text.contains("Successfully executed") || text.contains("added to context"));

        // Now check if the file is in context
        let show_result = test_ctx.execute_direct("/context show").await?;
        if let ChatState::DisplayHelp { help_text, .. } = show_result {
            assert!(help_text.contains(&file_path), "File with spaces not found in context");
        } else {
            panic!("Expected DisplayHelp state");
        }
    } else {
        panic!("Expected text output");
    }

    Ok(())
}
