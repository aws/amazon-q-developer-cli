//! Unit tests for the use_q_command tool.
//!
//! These tests verify that commands executed through the use_q_command tool
//! behave identically to commands executed directly.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use eyre::Result;
    use fig_os_shim::Context;

    use crate::cli::chat::ChatState;
    use crate::cli::chat::commands::CommandRegistry;
    use crate::cli::chat::tools::use_q_command::schema::UseQCommand;
    use crate::cli::chat::tools::{
        OutputKind,
        Tool,
    };

    #[test]
    fn test_use_q_command_schema() -> Result<()> {
        // Create a basic command
        let command = UseQCommand {
            command: "help".to_string(),
            subcommand: None,
            args: None,
            flags: None,
            tool_use_id: None,
        };

        // Verify the command structure
        assert_eq!(command.command, "help");
        assert!(command.subcommand.is_none());
        assert!(command.args.is_none());
        assert!(command.flags.is_none());
        assert!(command.tool_use_id.is_none());

        // Create a command with subcommand
        let command_with_subcommand = UseQCommand {
            command: "context".to_string(),
            subcommand: Some("add".to_string()),
            args: Some(vec!["file.txt".to_string()]),
            flags: None,
            tool_use_id: None,
        };

        // Verify the command structure
        assert_eq!(command_with_subcommand.command, "context");
        assert_eq!(command_with_subcommand.subcommand, Some("add".to_string()));
        assert_eq!(command_with_subcommand.args, Some(vec!["file.txt".to_string()]));
        assert!(command_with_subcommand.flags.is_none());
        assert!(command_with_subcommand.tool_use_id.is_none());

        // Create a command with flags
        let mut flags = HashMap::new();
        flags.insert("force".to_string(), "true".to_string());

        let command_with_flags = UseQCommand {
            command: "context".to_string(),
            subcommand: Some("add".to_string()),
            args: Some(vec!["file.txt".to_string()]),
            flags: Some(flags),
            tool_use_id: Some("test-id".to_string()),
        };

        // Verify the command structure
        assert_eq!(command_with_flags.command, "context");
        assert_eq!(command_with_flags.subcommand, Some("add".to_string()));
        assert_eq!(command_with_flags.args, Some(vec!["file.txt".to_string()]));
        assert_eq!(command_with_flags.flags.as_ref().unwrap().get("force").unwrap(), "true");
        assert_eq!(command_with_flags.tool_use_id, Some("test-id".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_help_command_execution() -> Result<()> {
        // Create a test context
        let context = Context::builder().build_fake();

        // Get the command registry
        let registry = CommandRegistry::global();

        // Execute help command directly
        let direct_result = registry.parse_and_execute("/help", &context, None, None).await?;

        // Create a help command for the tool
        let tool_command = UseQCommand {
            command: "help".to_string(),
            subcommand: None,
            args: None,
            flags: None,
            tool_use_id: Some("test-id".to_string()),
        };

        // Create the tool
        let tool = Tool::UseQCommand(tool_command);

        // Execute the tool
        let mut output_buffer = Vec::new();
        let tool_result = tool.invoke(&context, &mut output_buffer).await?;

        // Compare results
        match direct_result {
            ChatState::DisplayHelp { .. } => {
                match tool_result.output {
                    OutputKind::Text(text) => {
                        // The help command now displays help directly to the user and returns a message
                        // indicating that it has been displayed, so we check for that message instead
                        assert!(text.contains("Help information has been displayed"));

                        // The output buffer should contain the command execution message
                        let buffer_text = String::from_utf8_lossy(&output_buffer).to_string();
                        assert!(buffer_text.contains("Executing command:"));
                    },
                    _ => panic!("Expected text output"),
                }
            },
            _ => panic!("Expected DisplayHelp state"),
        }

        Ok(())
    }
}
