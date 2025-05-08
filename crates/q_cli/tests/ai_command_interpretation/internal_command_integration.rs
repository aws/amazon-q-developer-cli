use std::io::Write;
use std::sync::Arc;

use eyre::Result;
use fig_os_shim::Context;

use q_chat::ChatState;
use q_chat::command::{Command, ContextSubcommand, ProfileSubcommand, ToolsSubcommand};
use q_chat::tools::internal_command::schema::InternalCommand;
use q_chat::tools::{InvokeOutput, Tool};

struct TestContext {
    context: Arc<Context>,
    output_buffer: Vec<u8>,
}

impl TestContext {
    async fn new() -> Result<Self> {
        let context = Arc::new(Context::default());
        
        Ok(Self {
            context,
            output_buffer: Vec::new(),
        })
    }

    async fn execute_direct(&mut self, command: &str) -> Result<ChatState> {
        // This is a simplified version - in a real implementation, this would use the CommandRegistry
        match command {
            "/quit" => Ok(ChatState::Exit),
            "/help" => Ok(ChatState::ExecuteCommand {
                command: Command::Help { help_text: Some("Help text".to_string()) },
                tool_uses: None,
                pending_tool_index: None,
            }),
            "/clear" => Ok(ChatState::PromptUser {
                tool_uses: None,
                pending_tool_index: None,
                skip_printing_tools: true,
            }),
            _ => Ok(ChatState::PromptUser {
                tool_uses: None,
                pending_tool_index: None,
                skip_printing_tools: true,
            }),
        }
    }

    async fn execute_via_tool(&mut self, command: InternalCommand) -> Result<InvokeOutput> {
        let tool = Tool::InternalCommand(command);
        tool.invoke(&self.context, &mut self.output_buffer).await
    }

    fn get_output(&self) -> String {
        String::from_utf8_lossy(&self.output_buffer).to_string()
    }

    fn clear_output(&mut self) {
        self.output_buffer.clear();
    }
}

fn create_command(command_str: &str) -> InternalCommand {
    InternalCommand {
        command: command_str.to_string(),
        subcommand: None,
        args: None,
        flags: None,
        tool_use_id: None,
    }
}

#[tokio::test]
async fn test_exit_command_returns_exit_state() -> Result<()> {
    let mut test_context = TestContext::new().await?;
    
    // Create a quit command
    let command = create_command("quit");
    
    // Execute the command via the tool
    let result = test_context.execute_via_tool(command).await?;
    
    // Check that the result contains the expected next state
    if let Some(ChatState::ExecuteCommand { command, .. }) = result.next_state {
        assert!(matches!(command, Command::Quit));
    } else {
        panic!("Expected ExecuteCommand state with Quit command, got {:?}", result.next_state);
    }
    
    // Check that the output contains the expected text
    let output = test_context.get_output();
    assert!(output.contains("Suggested command: `/quit`"));
    assert!(output.contains("Exit the chat session"));
    
    Ok(())
}

#[tokio::test]
async fn test_help_command_returns_promptuser_state() -> Result<()> {
    let mut test_context = TestContext::new().await?;
    
    // Create a help command
    let command = create_command("help");
    
    // Execute the command via the tool
    let result = test_context.execute_via_tool(command).await?;
    
    // Check that the result contains the expected next state
    if let Some(ChatState::ExecuteCommand { command, .. }) = result.next_state {
        assert!(matches!(command, Command::Help { .. }));
    } else {
        panic!("Expected ExecuteCommand state with Help command, got {:?}", result.next_state);
    }
    
    // Check that the output contains the expected text
    let output = test_context.get_output();
    assert!(output.contains("Suggested command: `/help`"));
    assert!(output.contains("Show help information"));
    
    Ok(())
}

#[tokio::test]
async fn test_clear_command_returns_promptuser_state() -> Result<()> {
    let mut test_context = TestContext::new().await?;
    
    // Create a clear command
    let command = create_command("clear");
    
    // Execute the command via the tool
    let result = test_context.execute_via_tool(command).await?;
    
    // Check that the result contains the expected next state
    if let Some(ChatState::ExecuteCommand { command, .. }) = result.next_state {
        assert!(matches!(command, Command::Clear));
    } else {
        panic!("Expected ExecuteCommand state with Clear command, got {:?}", result.next_state);
    }
    
    // Check that the output contains the expected text
    let output = test_context.get_output();
    assert!(output.contains("Suggested command: `/clear`"));
    assert!(output.contains("Clear the current conversation history"));
    
    Ok(())
}

#[tokio::test]
async fn test_context_command_returns_promptuser_state() -> Result<()> {
    let mut test_context = TestContext::new().await?;
    
    // Create a context add command with arguments and flags
    let mut command = InternalCommand {
        command: "context".to_string(),
        subcommand: Some("add".to_string()),
        args: Some(vec!["file.txt".to_string()]),
        flags: Some([("global".to_string(), "".to_string())].iter().cloned().collect()),
        tool_use_id: None,
    };
    
    // Execute the command via the tool
    let result = test_context.execute_via_tool(command).await?;
    
    // Check that the result contains the expected next state
    if let Some(ChatState::ExecuteCommand { command: Command::Context { subcommand }, .. }) = result.next_state {
        if let ContextSubcommand::Add { global, paths, .. } = subcommand {
            assert!(global);
            assert_eq!(paths, vec!["file.txt"]);
        } else {
            panic!("Expected ContextSubcommand::Add, got {:?}", subcommand);
        }
    } else {
        panic!("Expected ExecuteCommand state with Context command, got {:?}", result.next_state);
    }
    
    // Check that the output contains the expected text
    let output = test_context.get_output();
    assert!(output.contains("Suggested command: `/context add file.txt --global`"));
    assert!(output.contains("Add a file to the conversation context"));
    
    Ok(())
}

#[tokio::test]
async fn test_profile_command_returns_promptuser_state() -> Result<()> {
    let mut test_context = TestContext::new().await?;
    
    // Create a profile create command with arguments
    let mut command = InternalCommand {
        command: "profile".to_string(),
        subcommand: Some("create".to_string()),
        args: Some(vec!["test-profile".to_string()]),
        flags: None,
        tool_use_id: None,
    };
    
    // Execute the command via the tool
    let result = test_context.execute_via_tool(command).await?;
    
    // Check that the result contains the expected next state
    if let Some(ChatState::ExecuteCommand { command: Command::Profile { subcommand }, .. }) = result.next_state {
        if let ProfileSubcommand::Create { name } = subcommand {
            assert_eq!(name, "test-profile");
        } else {
            panic!("Expected ProfileSubcommand::Create, got {:?}", subcommand);
        }
    } else {
        panic!("Expected ExecuteCommand state with Profile command, got {:?}", result.next_state);
    }
    
    // Check that the output contains the expected text
    let output = test_context.get_output();
    assert!(output.contains("Suggested command: `/profile create test-profile`"));
    assert!(output.contains("Create a new profile"));
    
    Ok(())
}

#[tokio::test]
async fn test_tools_command_returns_promptuser_state() -> Result<()> {
    let mut test_context = TestContext::new().await?;
    
    // Create a tools trust command with arguments
    let mut command = InternalCommand {
        command: "tools".to_string(),
        subcommand: Some("trust".to_string()),
        args: Some(vec!["fs_write".to_string()]),
        flags: None,
        tool_use_id: None,
    };
    
    // Execute the command via the tool
    let result = test_context.execute_via_tool(command).await?;
    
    // Check that the result contains the expected next state
    if let Some(ChatState::ExecuteCommand { command: Command::Tools { subcommand }, .. }) = result.next_state {
        if let Some(ToolsSubcommand::Trust { tool_names }) = subcommand {
            assert!(tool_names.contains("fs_write"));
        } else {
            panic!("Expected ToolsSubcommand::Trust, got {:?}", subcommand);
        }
    } else {
        panic!("Expected ExecuteCommand state with Tools command, got {:?}", result.next_state);
    }
    
    // Check that the output contains the expected text
    let output = test_context.get_output();
    assert!(output.contains("Suggested command: `/tools trust fs_write`"));
    assert!(output.contains("Trust a specific tool"));
    
    Ok(())
}
