// tests/path_handling_tests.rs
// Tests for path handling in the save command

use std::path::Path;
use tempfile::tempdir;
use crate::conversation::Conversation;
use crate::save_config::SaveConfig;
use crate::commands::save::handle_save_command;

#[test]
fn test_default_path_resolution() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with a default path
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with no arguments
    let args = Vec::<String>::new();
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that the file was saved to the default path
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert!(save_path.starts_with(&default_path));
    assert!(Path::new(&save_path).exists());
}

#[test]
fn test_directory_path_with_auto_filename() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with a directory path
    let custom_dir = temp_dir.path().join("custom").to_string_lossy().to_string();
    let args = vec![format!("{}/", custom_dir)];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that the file was saved to the custom directory with an auto-generated filename
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert!(save_path.starts_with(&custom_dir));
    assert!(Path::new(&save_path).exists());
}

#[test]
fn test_full_path_backward_compatibility() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with a full path
    let full_path = temp_dir.path().join("my-conversation.q.json").to_string_lossy().to_string();
    let args = vec![full_path.clone()];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that the file was saved to the specified path
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert_eq!(save_path, full_path);
    assert!(Path::new(&save_path).exists());
}

#[test]
fn test_path_creation() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with a nested directory path that doesn't exist
    let nested_dir = temp_dir.path().join("a/b/c").to_string_lossy().to_string();
    let args = vec![format!("{}/", nested_dir)];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that the directories were created and the file was saved
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert!(save_path.starts_with(&nested_dir));
    assert!(Path::new(&save_path).exists());
}

#[test]
fn test_home_directory_expansion() {
    // Create a save config with a default path that includes ~
    let config = SaveConfig::new("~/test-config.json");
    
    // Get the default path
    let default_path = config.get_default_path();
    
    // Check that ~ was expanded to the home directory
    assert!(!default_path.contains("~"));
    assert!(default_path.contains("/home/") || default_path.contains("/Users/"));
}

#[test]
fn test_relative_path_resolution() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with a relative path
    let args = vec!["./relative-path.q.json".to_string()];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that the file was saved to the current directory
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert!(save_path.ends_with("relative-path.q.json"));
    assert!(Path::new(&save_path).exists());
}
