use std::collections::HashMap;
use std::sync::OnceLock;

use eyre::Result;

use crate::cli::chat::commands::{
    ClearCommand,
    CommandHandler,
    CompactCommand,
    ContextCommand,
    HelpCommand,
    ProfileCommand,
    QuitCommand,
};
use crate::cli::chat::{
    ChatContext,
    ChatState,
    QueuedTool,
};

/// A registry of available commands that can be executed
pub struct CommandRegistry {
    /// Map of command names to their handlers
    commands: HashMap<String, Box<dyn CommandHandler>>,
}

impl CommandRegistry {
    /// Create a new command registry with all built-in commands
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
        };

        // Register built-in commands
        registry.register("quit", Box::new(QuitCommand::new()));
        registry.register("clear", Box::new(ClearCommand::new()));
        registry.register("help", Box::new(HelpCommand::new()));
        registry.register("compact", Box::new(CompactCommand::new()));

        // Register context command and its subcommands
        registry.register("context", Box::new(ContextCommand::new()));

        // Register profile command and its subcommands
        registry.register("profile", Box::new(ProfileCommand::new()));

        // We'll need to update these once we implement the modules
        // registry.register("tools", Box::new(ToolsCommand::new()));

        registry
    }

    /// Get the global instance of the command registry
    pub fn global() -> &'static CommandRegistry {
        static INSTANCE: OnceLock<CommandRegistry> = OnceLock::new();
        INSTANCE.get_or_init(CommandRegistry::new)
    }

    /// Register a new command handler
    pub fn register(&mut self, name: &str, handler: Box<dyn CommandHandler>) {
        self.commands.insert(name.to_string(), handler);
    }

    /// Get a command handler by name
    pub fn get(&self, name: &str) -> Option<&dyn CommandHandler> {
        self.commands.get(name).map(|h| h.as_ref())
    }

    /// Check if a command exists
    #[allow(dead_code)]
    pub fn command_exists(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    /// Get all command names
    #[allow(dead_code)]
    pub fn command_names(&self) -> Vec<&String> {
        self.commands.keys().collect()
    }

    /// Generate a description of all available commands for help text
    #[allow(dead_code)]
    pub fn generate_commands_description(&self) -> String {
        let mut description = String::new();

        for name in self.command_names() {
            if let Some(handler) = self.get(name) {
                description.push_str(&format!("{} - {}\n", handler.usage(), handler.description()));
            }
        }

        description
    }

    /// Generate structured command information for LLM reference
    pub fn generate_llm_descriptions(&self) -> serde_json::Value {
        let mut commands = serde_json::Map::new();

        for name in self.command_names() {
            if let Some(handler) = self.get(name) {
                commands.insert(
                    name.to_string(),
                    serde_json::json!({
                        "description": handler.llm_description(),
                        "usage": handler.usage(),
                        "help": handler.help()
                    }),
                );
            }
        }

        serde_json::json!(commands)
    }

    /// Parse and execute a command string
    pub async fn parse_and_execute(
        &self,
        input: &str,
        chat_context: &mut ChatContext<impl std::io::Write>,
        tool_uses: Option<Vec<QueuedTool>>,
        pending_tool_index: Option<usize>,
    ) -> Result<ChatState> {
        let (name, args) = Self::parse_command(input)?;

        if let Some(handler) = self.get(name) {
            let parsed_args = handler.parse_args(args)?;
            handler.execute(parsed_args, chat_context, tool_uses, pending_tool_index).await
        } else {
            // If not a registered command, treat as a question to the AI
            Ok(ChatState::HandleInput {
                input: input.to_string(),
                tool_uses,
                pending_tool_index,
            })
        }
    }

    /// Parse a command string into name and arguments
    fn parse_command(input: &str) -> Result<(&str, Vec<&str>)> {
        let input = input.trim();

        // Handle slash commands
        if let Some(stripped) = input.strip_prefix('/') {
            let parts: Vec<&str> = stripped.splitn(2, ' ').collect();
            let command = parts[0];
            let args = if parts.len() > 1 {
                parts[1].split_whitespace().collect()
            } else {
                Vec::new()
            };

            Ok((command, args))
        } else {
            // Not a slash command
            Err(eyre::eyre!("Not a command: {}", input))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fig_os_shim::Context;

    #[test]
    fn test_command_registry_register_and_get() {
        let mut registry = CommandRegistry::new();

        // Create a simple command handler
        struct TestCommand;
        impl CommandHandler for TestCommand {
            fn name(&self) -> &'static str {
                "test"
            }

            fn description(&self) -> &'static str {
                "Test command"
            }

            fn usage(&self) -> &'static str {
                "/test"
            }

            fn help(&self) -> String {
                "Test command help".to_string()
            }

            fn execute<'a>(
                &'a self,
                _args: Vec<&'a str>,
                _chat_context: &'a mut ChatContext<impl std::io::Write>,
                _tool_uses: Option<Vec<QueuedTool>>,
                _pending_tool_index: Option<usize>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ChatState>> + Send + 'a>> {
                Box::pin(async { Ok(ChatState::Exit) })
            }

            fn requires_confirmation(&self, _args: &[&str]) -> bool {
                false
            }

            fn parse_args<'a>(&self, args: Vec<&'a str>) -> Result<Vec<&'a str>> {
                Ok(args)
            }
        }

        registry.register("test", Box::new(TestCommand));

        assert!(registry.command_exists("test"));
        assert!(!registry.command_exists("nonexistent"));

        let handler = registry.get("test");
        assert!(handler.is_some());
        assert_eq!(handler.unwrap().name(), "test");
    }

    #[test]
    fn test_parse_command() {
        let _registry = CommandRegistry::new();

        // Test valid command
        let result = CommandRegistry::parse_command("/test arg1 arg2");
        assert!(result.is_ok());
        let (name, args) = result.unwrap();
        assert_eq!(name, "test");
        assert_eq!(args, vec!["arg1", "arg2"]);

        // Test command with no args
        let result = CommandRegistry::parse_command("/test");
        assert!(result.is_ok());
        let (name, args) = result.unwrap();
        assert_eq!(name, "test");
        assert_eq!(args, Vec::<&str>::new());

        // Test invalid command (no slash)
        let result = CommandRegistry::parse_command("test arg1 arg2");
        assert!(result.is_err());
    }
}
