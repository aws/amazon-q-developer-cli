// tests/integration_tests.rs
// Integration tests for Amazon Q CLI automatic naming feature

use std::fs;
use std::path::Path;
use tempfile::tempdir;
use crate::conversation::Conversation;
use crate::filename_generator::generate_filename;
use crate::topic_extractor::extract_topics;
use crate::save_config::SaveConfig;
use crate::commands::CommandRegistry;
use crate::tests::mocks::create_mock_conversation;

#[test]
fn test_end_to_end_auto_filename() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config with a default path
    let config_path = temp_dir.path().join("config.json");
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("amazon_q_cli");
    
    // Execute the save command with no arguments (auto-generated filename)
    let result = registry.execute_command("save", &[], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file exists
    assert!(Path::new(&save_path).exists());
    
    // Check that the file contains the conversation
    let content = fs::read_to_string(&save_path).unwrap();
    let saved_conv: Conversation = serde_json::from_str(&content).unwrap();
    assert_eq!(saved_conv.id, conv.id);
    assert_eq!(saved_conv.messages.len(), conv.messages.len());
    
    // Check that the filename follows the expected format
    let filename = Path::new(&save_path).file_name().unwrap().to_string_lossy();
    assert!(filename.starts_with("Q_AmazonQ_CLI_"));
    assert!(filename.contains(" - "));
    assert!(filename.ends_with(".q.json"));
}

#[test]
fn test_end_to_end_custom_directory() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let config = SaveConfig::new(&config_path);
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("feature_request");
    
    // Execute the save command with a directory path
    let custom_dir = temp_dir.path().join("custom").to_string_lossy().to_string();
    let result = registry.execute_command("save", &[format!("{}/", custom_dir)], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file exists
    assert!(Path::new(&save_path).exists());
    
    // Check that the file is in the custom directory
    assert!(save_path.starts_with(&custom_dir));
    
    // Check that the filename follows the expected format
    let filename = Path::new(&save_path).file_name().unwrap().to_string_lossy();
    assert!(filename.starts_with("Q_AmazonQ_CLI_"));
    assert!(filename.contains(" - "));
    assert!(filename.ends_with(".q.json"));
}

#[test]
fn test_end_to_end_full_path() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let config = SaveConfig::new(&config_path);
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("technical");
    
    // Execute the save command with a full path
    let full_path = temp_dir.path().join("my-conversation.q.json").to_string_lossy().to_string();
    let result = registry.execute_command("save", &[full_path.clone()], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file exists
    assert!(Path::new(&save_path).exists());
    
    // Check that the file is at the specified path
    assert_eq!(save_path, full_path);
}

#[test]
fn test_configuration_changes() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config with a default path
    let config_path = temp_dir.path().join("config.json");
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a command registry
    let mut registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("amazon_q_cli");
    
    // Execute the save command with no arguments
    let result = registry.execute_command("save", &[], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file is in the default directory
    assert!(save_path.starts_with(&default_path));
    
    // Change the default path
    let new_default_path = temp_dir.path().join("new-qChats").to_string_lossy().to_string();
    registry.get_config_mut().set_default_path(&new_default_path).unwrap();
    
    // Execute the save command again
    let result = registry.execute_command("save", &[], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file is in the new default directory
    assert!(save_path.starts_with(&new_default_path));
}

#[test]
fn test_backward_compatibility() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let config = SaveConfig::new(&config_path);
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("amazon_q_cli");
    
    // Execute the save command with a full path (original format)
    let full_path = temp_dir.path().join("my-conversation.q.json").to_string_lossy().to_string();
    let result = registry.execute_command("save", &[full_path.clone()], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file exists
    assert!(Path::new(&save_path).exists());
    
    // Check that the file is at the specified path
    assert_eq!(save_path, full_path);
}

#[test]
fn test_error_handling() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let mut config = SaveConfig::new(&config_path);
    
    // Set a mock file system error
    config.set_mock_fs_error(Some(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "Mock permission denied"
    )));
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("amazon_q_cli");
    
    // Execute the save command
    let result = registry.execute_command("save", &[], &conv);
    
    // Check that an error was returned
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("permission denied"));
}

#[test]
fn test_help_text() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let config = SaveConfig::new(&config_path);
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Get help text for the save command
    let help_text = registry.get_help_text("save");
    assert!(help_text.is_some());
    
    let help_text = help_text.unwrap();
    
    // Check that the help text contains the expected information
    assert!(help_text.contains("/save [path]"));
    assert!(help_text.contains("Without arguments:"));
    assert!(help_text.contains("With directory path:"));
    assert!(help_text.contains("With full path:"));
    assert!(help_text.contains("Examples:"));
}

#[test]
fn test_unknown_command() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let config = SaveConfig::new(&config_path);
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a conversation
    let conv = create_mock_conversation("amazon_q_cli");
    
    // Execute an unknown command
    let result = registry.execute_command("unknown", &[], &conv);
    
    // Check that an error was returned
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "Unknown command: unknown");
}

#[test]
fn test_component_integration() {
    // Test that all components work together correctly
    
    // Create a conversation
    let conv = create_mock_conversation("amazon_q_cli");
    
    // Extract topics
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Generate filename
    let filename = generate_filename(&conv);
    
    // Check that the filename contains the topics
    assert!(filename.contains(&main_topic));
    assert!(filename.contains(&sub_topic));
    assert!(filename.contains(&action_type));
    
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create a save config
    let config_path = temp_dir.path().join("config.json");
    let config = SaveConfig::new(&config_path);
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Execute the save command
    let result = registry.execute_command("save", &[], &conv);
    assert!(result.is_ok());
    
    let save_path = result.unwrap();
    
    // Check that the file exists
    assert!(Path::new(&save_path).exists());
    
    // Check that the filename in the save path matches the generated filename
    let saved_filename = Path::new(&save_path).file_stem().unwrap().to_string_lossy();
    assert!(saved_filename.starts_with(&filename));
}
