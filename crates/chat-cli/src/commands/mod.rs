// commands/mod.rs
// Command registration for Amazon Q CLI automatic naming feature

pub mod save;

use std::collections::HashMap;
use crate::conversation::Conversation;
use crate::save_config::SaveConfig;
use self::save::handle_save_command;

/// Command handler function type
pub type CommandHandler = fn(&[String], &Conversation, &SaveConfig) -> Result<String, Box<dyn std::error::Error>>;

/// Command registry
pub struct CommandRegistry {
    /// Registered commands
    commands: HashMap<String, CommandHandler>,
    
    /// Save configuration
    config: SaveConfig,
}

impl CommandRegistry {
    /// Create a new command registry
    pub fn new(config: SaveConfig) -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
            config,
        };
        
        // Register the save command
        registry.register_save_command();
        
        registry
    }
    
    /// Register the save command
    fn register_save_command(&mut self) {
        self.commands.insert("save".to_string(), |args, conv, config| {
            handle_save_command(args, conv, config)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        });
    }
    
    /// Execute a command
    pub fn execute_command(
        &self,
        command: &str,
        args: &[String],
        conversation: &Conversation,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(handler) = self.commands.get(command) {
            handler(args, conversation, &self.config)
        } else {
            Err(format!("Unknown command: {}", command).into())
        }
    }
    
    /// Get help text for a command
    pub fn get_help_text(&self, command: &str) -> Option<String> {
        match command {
            "save" => Some(
                "/save [path]\n  Save the current conversation.\n\n  \
                Without arguments: Automatically generates a filename and saves to the default location.\n  \
                With directory path: Saves to the specified directory with an auto-generated filename.\n  \
                With full path: Saves to the specified path with the given filename.\n\n  \
                Examples:\n    \
                /save\n    \
                /save ~/my-conversations/\n    \
                /save ~/my-conversations/important-chat.q.json".to_string()
            ),
            _ => None,
        }
    }
    
    /// Get the list of available commands
    pub fn get_commands(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }
    
    /// Get the save configuration
    pub fn get_config(&self) -> &SaveConfig {
        &self.config
    }
    
    /// Get a mutable reference to the save configuration
    pub fn get_config_mut(&mut self) -> &mut SaveConfig {
        &mut self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::tests::mocks::create_mock_conversation;
    
    #[test]
    fn test_register_save_command() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config = SaveConfig::new(&config_path);
        
        let registry = CommandRegistry::new(config);
        
        assert!(registry.commands.contains_key("save"));
    }
    
    #[test]
    fn test_execute_save_command() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        
        let registry = CommandRegistry::new(config);
        let conv = create_mock_conversation("amazon_q_cli");
        
        let result = registry.execute_command("save", &[], &conv);
        
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert!(save_path.starts_with(&default_path));
    }
    
    #[test]
    fn test_unknown_command() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config = SaveConfig::new(&config_path);
        
        let registry = CommandRegistry::new(config);
        let conv = create_mock_conversation("amazon_q_cli");
        
        let result = registry.execute_command("unknown", &[], &conv);
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Unknown command: unknown");
    }
    
    #[test]
    fn test_get_help_text() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config = SaveConfig::new(&config_path);
        
        let registry = CommandRegistry::new(config);
        
        let help_text = registry.get_help_text("save");
        assert!(help_text.is_some());
        assert!(help_text.unwrap().contains("/save [path]"));
        
        let help_text = registry.get_help_text("unknown");
        assert!(help_text.is_none());
    }
    
    #[test]
    fn test_get_commands() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let config = SaveConfig::new(&config_path);
        
        let registry = CommandRegistry::new(config);
        
        let commands = registry.get_commands();
        assert!(commands.contains(&"save".to_string()));
    }
}
