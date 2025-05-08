#[cfg(test)]
mod tests {
    use crate::cli::chat::command::{
        Command,
        ToolsSubcommand,
    };
    use crate::cli::chat::commands::CommandHandler;
    use crate::cli::chat::commands::tools::{
        LIST_TOOLS_HANDLER,
        TRUST_TOOLS_HANDLER,
        TRUSTALL_TOOLS_HANDLER,
        UNTRUST_TOOLS_HANDLER,
    };

    #[test]
    fn test_parsing_without_output() {
        // Test that the to_command method doesn't produce any output

        // Test list command
        let result = LIST_TOOLS_HANDLER.to_command(vec![]);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Command::Tools { subcommand: None }));

        // Test trust command
        let result = TRUST_TOOLS_HANDLER.to_command(vec!["fs_write"]);
        assert!(result.is_ok());
        if let Ok(Command::Tools {
            subcommand: Some(ToolsSubcommand::Trust { tool_names }),
        }) = result
        {
            assert_eq!(tool_names.len(), 1);
            assert!(tool_names.contains("fs_write"));
        } else {
            panic!("Expected Trust subcommand");
        }

        // Test untrust command
        let result = UNTRUST_TOOLS_HANDLER.to_command(vec!["fs_write"]);
        assert!(result.is_ok());
        if let Ok(Command::Tools {
            subcommand: Some(ToolsSubcommand::Untrust { tool_names }),
        }) = result
        {
            assert_eq!(tool_names.len(), 1);
            assert!(tool_names.contains("fs_write"));
        } else {
            panic!("Expected Untrust subcommand");
        }

        // Test trustall command
        let result = TRUSTALL_TOOLS_HANDLER.to_command(vec![]);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Command::Tools {
            subcommand: Some(ToolsSubcommand::TrustAll { from_deprecated: false })
        }));
    }

    #[test]
    fn test_trust_empty_args_error() {
        // Test that trust command with empty args returns an error
        let result = TRUST_TOOLS_HANDLER.to_command(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_untrust_empty_args_error() {
        // Test that untrust command with empty args returns an error
        let result = UNTRUST_TOOLS_HANDLER.to_command(vec![]);
        assert!(result.is_err());
    }
}
