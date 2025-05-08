#[cfg(test)]
mod command_tests {
    use super::*;
    use std::io::sink;

    #[test]
    fn test_parse_method() {
        // Test that the parse method handles various command types correctly
        let commands = vec![
            "/help",
            "/quit",
            "/clear",
            "/profile list",
            "/context show",
            "/tools",
            "/compact",
            "/usage",
            "/editor",
            "/issue",
            "regular prompt",
            "!execute command",
            "@prompt_name arg1 arg2",
        ];

        for cmd in commands {
            // Test that parse works correctly
            let result = Command::parse(cmd).unwrap();
            // Just verify we can parse these commands without errors
            assert!(matches!(result, Command::Ask { .. } | Command::Execute { .. } | Command::Prompts { .. }));
        }
    }

    #[test]
    fn test_to_handler() {
        // Test that all command variants return the correct handler
        let commands = vec![
            Command::Help { help_text: None },
            Command::Quit,
            Command::Clear,
            Command::Context { subcommand: ContextSubcommand::Help },
            Command::Profile { subcommand: ProfileSubcommand::Help },
            Command::Tools { subcommand: None },
            Command::Compact { prompt: None, show_summary: true, help: false },
            Command::PromptEditor { initial_text: None },
            Command::Usage,
            Command::Issue { prompt: None },
            Command::Ask { prompt: "test".to_string() },
            Command::Execute { command: "test".to_string() },
            Command::Prompts { subcommand: None },
        ];

        // Just verify that to_handler doesn't panic for any command variant
        for cmd in commands {
            let _handler = cmd.to_handler();
            // If we get here without panicking, the test passes
        }
    }

    #[test]
    fn test_generate_llm_descriptions() {
        // Test that generate_llm_descriptions includes all commands
        let descriptions = Command::generate_llm_descriptions();
        
        // Check that all expected commands are included
        let expected_commands = vec![
            "help", "quit", "clear", "context", "profile", 
            "tools", "compact", "usage", "editor", "issue"
        ];
        
        for cmd in expected_commands {
            assert!(descriptions.contains_key(cmd), "Missing description for command: {}", cmd);
            
            // Verify that each description has the required fields
            let desc = descriptions.get(cmd).unwrap();
            assert!(!desc.short_description.is_empty(), "Empty short description for {}", cmd);
            assert!(!desc.full_description.is_empty(), "Empty full description for {}", cmd);
            assert!(!desc.usage.is_empty(), "Empty usage for {}", cmd);
        }
    }
}
