// tests/error_handling_tests.rs
// Tests for error handling in the save command

use std::fs;
use std::path::Path;
use tempfile::tempdir;
use crate::conversation::Conversation;
use crate::save_config::SaveConfig;
use crate::commands::save::handle_save_command;

#[test]
fn test_permission_error() {
    // Skip this test on Windows as permission handling is different
    if cfg!(windows) {
        return;
    }
    
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Create a directory with no write permissions
    let no_write_dir = temp_dir.path().join("no-write");
    fs::create_dir(&no_write_dir).unwrap();
    fs::set_permissions(&no_write_dir, fs::Permissions::from_mode(0o555)).unwrap();
    
    // Call the save command with the no-write directory
    let args = vec![format!("{}/test.q.json", no_write_dir.to_string_lossy())];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that a permission error was returned
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("permission denied"));
    
    // Clean up
    fs::set_permissions(&no_write_dir, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn test_invalid_path() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with an invalid path
    let args = vec!["\0invalid".to_string()];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that an invalid path error was returned
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("invalid path"));
}

#[test]
fn test_path_too_long() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with a path that's too long
    let long_path = "a".repeat(1000);
    let args = vec![long_path];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that a path too long error was returned
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("too long") || err.to_string().contains("name too long"));
}

#[test]
fn test_directory_is_file() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Create a file that will be used as a directory
    let file_path = temp_dir.path().join("file");
    fs::write(&file_path, "test").unwrap();
    
    // Call the save command with a path that tries to use a file as a directory
    let args = vec![format!("{}/test.q.json", file_path.to_string_lossy())];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that an appropriate error was returned
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not a directory") || err.to_string().contains("directory"));
}

#[test]
fn test_disk_full_simulation() {
    // This test is a simulation as we can't easily make the disk full
    // Create a mock implementation that simulates a disk full error
    
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with a mock file system
    let mut config = SaveConfig::new(&config_path);
    config.set_mock_fs_error(Some(std::io::Error::new(
        std::io::ErrorKind::Other,
        "No space left on device"
    )));
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command
    let args = vec![temp_dir.path().join("test.q.json").to_string_lossy().to_string()];
    let result = handle_save_command(&args, &conv, &config);
    
    // Check that a disk full error was returned
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("No space left on device"));
}

#[test]
fn test_error_feedback() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let conv = Conversation::new("test-id".to_string());
    
    // Call the save command with a path that doesn't exist and can't be created
    let non_existent_dir = "/non/existent/directory";
    if !Path::new(non_existent_dir).exists() {
        let args = vec![format!("{}/test.q.json", non_existent_dir)];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that an appropriate error was returned with a helpful message
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No such file or directory") || 
               err.to_string().contains("cannot find") ||
               err.to_string().contains("not found"));
    }
}
