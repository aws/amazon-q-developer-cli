//! Security integration tests for Amazon Q CLI automatic naming feature

use amazon_q_cli_auto_naming::{
    Conversation,
    SaveConfig,
    commands,
    security::{SecuritySettings, validate_path, write_secure_file, redact_sensitive_information},
};
use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use tempfile::tempdir;

/// Test security settings creation
#[test]
fn test_security_settings_creation() {
    // Create a save config
    let mut config = SaveConfig::new("/tmp/config.json");
    
    // Add security-related metadata
    config.add_metadata("redact_sensitive", "true").unwrap();
    config.add_metadata("prevent_overwrite", "true").unwrap();
    config.add_metadata("file_permissions", "644").unwrap();
    config.add_metadata("directory_permissions", "755").unwrap();
    
    // Create options
    let mut options = HashMap::new();
    options.insert("redact".to_string(), String::new());
    
    // Create security settings
    let settings = commands::save::create_security_settings(&options, &config);
    
    // Check settings
    assert!(settings.redact_sensitive);
    assert!(settings.prevent_overwrite);
    assert_eq!(settings.file_permissions, 0o644);
    assert_eq!(settings.directory_permissions, 0o755);
}

/// Test path validation
#[test]
fn test_path_validation() {
    // Create security settings
    let settings = SecuritySettings::default();
    
    // Test valid path
    let valid_path = Path::new("/tmp/test.txt");
    let result = validate_path(valid_path, &settings);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), valid_path);
    
    // Test path with null byte
    let null_path = Path::new("/tmp/test\0.txt");
    let result = validate_path(null_path, &settings);
    assert!(result.is_err());
    
    // Test path too deep
    let mut deep_path = PathBuf::new();
    for i in 0..20 {
        deep_path.push(format!("dir{}", i));
    }
    deep_path.push("file.txt");
    let result = validate_path(&deep_path, &settings);
    assert!(result.is_err());
}

/// Test secure file writing
#[test]
fn test_secure_file_writing() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create security settings
    let mut settings = SecuritySettings::default();
    settings.file_permissions = 0o644;
    settings.directory_permissions = 0o755;
    
    // Test writing to a file
    let file_path = temp_dir.path().join("test.txt");
    let result = write_secure_file(&file_path, "test content", &settings);
    assert!(result.is_ok());
    assert!(file_path.exists());
    
    // Check file content
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "test content");
    
    // Check file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&file_path).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, settings.file_permissions);
    }
    
    // Test writing to a nested directory
    let nested_path = temp_dir.path().join("a/b/c/test.txt");
    let result = write_secure_file(&nested_path, "nested content", &settings);
    assert!(result.is_ok());
    assert!(nested_path.exists());
    
    // Check directory permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = nested_path.parent().unwrap();
        let metadata = fs::metadata(parent).unwrap();
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, settings.directory_permissions);
    }
}

/// Test sensitive information redaction
#[test]
fn test_sensitive_information_redaction() {
    // Test credit card redaction
    let text_with_cc = "My credit card is 1234-5678-9012-3456";
    let redacted_cc = redact_sensitive_information(text_with_cc);
    assert!(!redacted_cc.contains("1234-5678-9012-3456"));
    assert!(redacted_cc.contains("[REDACTED CREDIT CARD]"));
    
    // Test SSN redaction
    let text_with_ssn = "My SSN is 123-45-6789";
    let redacted_ssn = redact_sensitive_information(text_with_ssn);
    assert!(!redacted_ssn.contains("123-45-6789"));
    assert!(redacted_ssn.contains("[REDACTED SSN]"));
    
    // Test API key redaction
    let text_with_api_key = "My API key is abcdefghijklmnopqrstuvwxyz1234567890abcdef";
    let redacted_api_key = redact_sensitive_information(text_with_api_key);
    assert!(!redacted_api_key.contains("abcdefghijklmnopqrstuvwxyz1234567890abcdef"));
    assert!(redacted_api_key.contains("[REDACTED API KEY]"));
    
    // Test AWS key redaction
    let text_with_aws_key = "My AWS key is AKIAIOSFODNN7EXAMPLE";
    let redacted_aws_key = redact_sensitive_information(text_with_aws_key);
    assert!(!redacted_aws_key.contains("AKIAIOSFODNN7EXAMPLE"));
    assert!(redacted_aws_key.contains("[REDACTED AWS KEY]"));
    
    // Test password redaction
    let text_with_password = "password = secret123";
    let redacted_password = redact_sensitive_information(text_with_password);
    assert!(!redacted_password.contains("secret123"));
    assert!(redacted_password.contains("[REDACTED]"));
}

/// Test conversation redaction
#[test]
fn test_conversation_redaction() {
    // Create a conversation with sensitive information
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("My credit card is 1234-5678-9012-3456".to_string());
    conversation.add_assistant_message("I'll help you with that.".to_string(), None);
    
    // Redact the conversation
    let redacted = commands::save::redact_conversation(&conversation);
    
    // Check that user messages are redacted
    assert!(!redacted.messages[0].content.contains("1234-5678-9012-3456"));
    assert!(redacted.messages[0].content.contains("[REDACTED CREDIT CARD]"));
    
    // Check that assistant messages are not redacted
    assert_eq!(redacted.messages[1].content, "I'll help you with that.");
}

/// Test file overwrite protection
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
    let file_path = temp_dir.path().join("existing.q.json");
    fs::write(&file_path, "Original content").unwrap();
    
    // Call the save command with no-overwrite option
    let args = vec![file_path.to_string_lossy().to_string(), "--no-overwrite".to_string()];
    let result = commands::save::handle_save_command(&args, &conversation, &config);
    
    // Check that the file was saved with a different name
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert_ne!(save_path, file_path.to_string_lossy().to_string());
    assert!(Path::new(&save_path).exists());
    
    // Check that the original file is unchanged
    let original_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(original_content, "Original content");
}

/// Test symlink protection
#[test]
fn test_symlink_protection() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create security settings
    let mut settings = SecuritySettings::default();
    settings.follow_symlinks = false;
    
    // Create a target file
    let target_file = temp_dir.path().join("target.txt");
    fs::write(&target_file, "Target content").unwrap();
    
    // Create a symlink
    let symlink_path = temp_dir.path().join("symlink.txt");
    
    // Skip this test if symlinks can't be created (e.g., on Windows without admin privileges)
    if std::os::unix::fs::symlink(&target_file, &symlink_path).is_err() {
        return;
    }
    
    // Try to write to the symlink
    let result = write_secure_file(&symlink_path, "New content", &settings);
    
    // Check that the write failed
    assert!(result.is_err());
    
    // Check that the target file is unchanged
    let target_content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(target_content, "Target content");
    
    // Now allow symlinks
    settings.follow_symlinks = true;
    
    // Try to write to the symlink again
    let result = write_secure_file(&symlink_path, "New content", &settings);
    
    // Check that the write succeeded
    assert!(result.is_ok());
    
    // Check that the target file was updated
    let target_content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(target_content, "New content");
}

/// Test path traversal protection
#[test]
fn test_path_traversal_protection() {
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
    let result = commands::save::handle_save_command(&args, &conversation, &config);
    
    // The save should fail or be redirected to a safe path
    if result.is_ok() {
        let save_path = result.unwrap();
        assert!(!save_path.contains("/etc/passwd"));
        assert!(Path::new(&save_path).exists());
    } else {
        // Check that the error is a security error
        match result.unwrap_err() {
            commands::save::SaveError::Security(_) => (),
            commands::save::SaveError::InvalidPath(_) => (),
            err => panic!("Unexpected error: {:?}", err),
        }
    }
}
