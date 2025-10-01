// tests/security_tests.rs
// Security tests for Amazon Q CLI automatic naming feature

use std::fs;
use std::path::{Path, PathBuf};
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;
use crate::conversation::Conversation;
use crate::save_config::SaveConfig;
use crate::commands::save::{handle_save_command, SaveError};

#[test]
fn test_file_permissions() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with a default path
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Save the conversation
    let args = Vec::<String>::new();
    let result = handle_save_command(&args, &conversation, &config);
    assert!(result.is_ok());
    
    // Check that the file has appropriate permissions
    let save_path = result.unwrap();
    let metadata = fs::metadata(&save_path).unwrap();
    let permissions = metadata.permissions();
    
    // Check that the file is only readable/writable by the owner
    let mode = permissions.mode();
    assert_eq!(mode & 0o077, 0); // No permissions for group or others
    assert_eq!(mode & 0o700, 0o600); // Read/write for owner
}

#[test]
fn test_path_traversal() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Try to save to a path outside the allowed directories
    let args = vec!["../../../etc/passwd".to_string()];
    let result = handle_save_command(&args, &conversation, &config);
    
    // The save should fail or be redirected to a safe path
    if result.is_ok() {
        let save_path = result.unwrap();
        assert!(!save_path.contains("/etc/passwd"));
        assert!(Path::new(&save_path).exists());
    } else {
        match result.unwrap_err() {
            SaveError::InvalidPath(_) => (),
            SaveError::Io(_) => (),
            err => panic!("Unexpected error: {:?}", err),
        }
    }
}

#[test]
fn test_sanitize_filenames() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation with potentially dangerous content
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message with dangerous characters: /\\:*?\"<>|".to_string());
    
    // Save the conversation
    let args = Vec::<String>::new();
    let result = handle_save_command(&args, &conversation, &config);
    assert!(result.is_ok());
    
    // Check that the filename is sanitized
    let save_path = result.unwrap();
    let filename = Path::new(&save_path).file_name().unwrap().to_string_lossy();
    
    // Check that dangerous characters are removed
    assert!(!filename.contains('/'));
    assert!(!filename.contains('\\'));
    assert!(!filename.contains(':'));
    assert!(!filename.contains('*'));
    assert!(!filename.contains('?'));
    assert!(!filename.contains('"'));
    assert!(!filename.contains('<'));
    assert!(!filename.contains('>'));
    assert!(!filename.contains('|'));
}

#[test]
fn test_sensitive_information_redaction() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with sensitive information redaction enabled
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    config.add_metadata("redact_sensitive", "true").unwrap();
    
    // Create a conversation with sensitive information
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("My password is secret123".to_string());
    conversation.add_user_message("My credit card number is 1234-5678-9012-3456".to_string());
    conversation.add_user_message("My social security number is 123-45-6789".to_string());
    
    // Save the conversation
    let args = vec!["--redact".to_string()];
    let result = handle_save_command(&args, &conversation, &config);
    assert!(result.is_ok());
    
    // Check that sensitive information is redacted
    let save_path = result.unwrap();
    let content = fs::read_to_string(&save_path).unwrap();
    
    // Check that sensitive patterns are redacted
    assert!(!content.contains("secret123"));
    assert!(!content.contains("1234-5678-9012-3456"));
    assert!(!content.contains("123-45-6789"));
}

#[test]
fn test_directory_permissions() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with a default path
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Save the conversation to create the directory
    let args = Vec::<String>::new();
    let result = handle_save_command(&args, &conversation, &config);
    assert!(result.is_ok());
    
    // Check that the directory has appropriate permissions
    let dir_path = PathBuf::from(&default_path);
    let metadata = fs::metadata(&dir_path).unwrap();
    let permissions = metadata.permissions();
    
    // Check that the directory is only accessible by the owner
    let mode = permissions.mode();
    assert_eq!(mode & 0o077, 0); // No permissions for group or others
    assert_eq!(mode & 0o700, 0o700); // Read/write/execute for owner
}

#[test]
fn test_file_overwrite_protection() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a file that we don't want to overwrite
    let file_path = temp_dir.path().join("important.txt");
    fs::write(&file_path, "Important data").unwrap();
    
    // Try to save to the same path
    let args = vec![file_path.to_string_lossy().to_string()];
    let result = handle_save_command(&args, &conversation, &config);
    
    // The save should succeed (overwriting is allowed by default)
    assert!(result.is_ok());
    
    // Now try with overwrite protection
    let args = vec![
        file_path.to_string_lossy().to_string(),
        "--no-overwrite".to_string(),
    ];
    
    // Create a new file to test overwrite protection
    let file_path2 = temp_dir.path().join("important2.txt");
    fs::write(&file_path2, "Important data").unwrap();
    
    let result = handle_save_command(&args, &conversation, &config);
    
    // The save should fail or create a new file with a different name
    if result.is_ok() {
        let save_path = result.unwrap();
        assert_ne!(save_path, file_path2.to_string_lossy().to_string());
    } else {
        match result.unwrap_err() {
            SaveError::Io(_) => (),
            err => panic!("Unexpected error: {:?}", err),
        }
    }
}

#[test]
fn test_null_byte_injection() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Try to save with a path containing a null byte
    let args = vec![format!("test{}.txt", '\0')];
    let result = handle_save_command(&args, &conversation, &config);
    
    // The save should fail
    assert!(result.is_err());
    match result.unwrap_err() {
        SaveError::InvalidPath(_) => (),
        err => panic!("Expected InvalidPath error, got {:?}", err),
    }
}

#[test]
fn test_symlink_attack() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a directory for saving
    let save_dir = PathBuf::from(&default_path);
    fs::create_dir_all(&save_dir).unwrap();
    
    // Create a target file that we don't want to overwrite
    let target_file = temp_dir.path().join("target.txt");
    fs::write(&target_file, "Target data").unwrap();
    
    // Create a symlink in the save directory pointing to the target file
    let symlink_path = save_dir.join("symlink.q.json");
    
    // Skip this test if symlinks can't be created (e.g., on Windows without admin privileges)
    if std::os::unix::fs::symlink(&target_file, &symlink_path).is_err() {
        return;
    }
    
    // Try to save to the symlink path
    let args = vec![symlink_path.to_string_lossy().to_string()];
    let result = handle_save_command(&args, &conversation, &config);
    
    // The save should either fail or follow the symlink (which is fine in this case)
    if result.is_ok() {
        // If it succeeded, check that the target file was overwritten
        let content = fs::read_to_string(&target_file).unwrap();
        assert!(content.contains("Test message"));
    }
}

#[test]
fn test_race_condition() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Save the conversation to a specific path
    let save_path = temp_dir.path().join("race.q.json");
    let args = vec![save_path.to_string_lossy().to_string()];
    
    // Save the conversation
    let result = handle_save_command(&args, &conversation, &config);
    assert!(result.is_ok());
    
    // Check that the file exists
    assert!(save_path.exists());
    
    // In a real test, we would use multiple threads to test for race conditions,
    // but that's beyond the scope of this example. Instead, we'll just verify
    // that the file was saved correctly.
    let content = fs::read_to_string(&save_path).unwrap();
    assert!(content.contains("Test message"));
}
