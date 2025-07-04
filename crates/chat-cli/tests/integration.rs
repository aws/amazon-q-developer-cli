//! Integration tests for Amazon Q CLI automatic naming feature

use amazon_q_cli_auto_naming::{
    Conversation,
    SaveConfig,
    filename_generator,
    topic_extractor,
    commands,
    security::{SecuritySettings, validate_path, write_secure_file},
    integration_checkpoint_1,
    integration_checkpoint_2,
    integration_checkpoint_3,
};
use std::path::{Path, PathBuf};
use std::fs;
use tempfile::tempdir;

/// Test the entire feature end-to-end
#[test]
fn test_end_to_end() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with a default path
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    // Save the conversation with automatic naming
    let args = Vec::<String>::new();
    let result = commands::save::handle_save_command(&args, &conversation, &config);
    
    // Check that the save was successful
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert!(Path::new(&save_path).exists());
    
    // Check that the filename contains the expected topics
    assert!(save_path.contains("AmazonQ") || save_path.contains("CLI") || save_path.contains("Help"));
    
    // Check that the file contains the conversation
    let content = fs::read_to_string(save_path).unwrap();
    let saved_conv: Conversation = serde_json::from_str(&content).unwrap();
    assert_eq!(saved_conv.id, conversation.id);
    assert_eq!(saved_conv.messages.len(), conversation.messages.len());
}

/// Test all topic extractors
#[test]
fn test_all_topic_extractors() {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    
    // Test basic extractor
    let (basic_main, basic_sub, basic_action) = topic_extractor::basic::extract_topics(&conversation);
    assert!(!basic_main.is_empty());
    assert!(!basic_sub.is_empty());
    assert!(!basic_action.is_empty());
    
    // Test enhanced extractor
    let (enhanced_main, enhanced_sub, enhanced_action) = topic_extractor::enhanced::extract_topics(&conversation);
    assert!(!enhanced_main.is_empty());
    assert!(!enhanced_sub.is_empty());
    assert!(!enhanced_action.is_empty());
    
    // Test advanced extractor
    let (advanced_main, advanced_sub, advanced_action) = topic_extractor::advanced::extract_topics(&conversation);
    assert!(!advanced_main.is_empty());
    assert!(!advanced_sub.is_empty());
    assert!(!advanced_action.is_empty());
    
    // Generate filenames with each extractor
    let basic_filename = filename_generator::generate_filename_with_extractor(&conversation, &topic_extractor::basic::extract_topics);
    let enhanced_filename = filename_generator::generate_filename_with_extractor(&conversation, &topic_extractor::enhanced::extract_topics);
    let advanced_filename = filename_generator::generate_filename_with_extractor(&conversation, &topic_extractor::advanced::extract_topics);
    
    // Check that all filenames are valid
    assert!(basic_filename.starts_with("Q_"));
    assert!(enhanced_filename.starts_with("Q_"));
    assert!(advanced_filename.starts_with("Q_"));
}

/// Test all configuration options
#[test]
fn test_all_configuration_options() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let mut config = SaveConfig::new(&config_path);
    
    // Test default path
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    assert_eq!(config.get_default_path(), default_path);
    
    // Test filename format
    use amazon_q_cli_auto_naming::save_config::FilenameFormat;
    config.set_filename_format(FilenameFormat::Custom(String::from("{main_topic}-{date}"))).unwrap();
    match config.get_filename_format() {
        FilenameFormat::Custom(format) => assert_eq!(format, "{main_topic}-{date}"),
        _ => panic!("Expected Custom format"),
    }
    
    // Test prefix
    config.set_prefix("Custom_").unwrap();
    assert_eq!(config.get_prefix(), "Custom_");
    
    // Test separator
    config.set_separator("-").unwrap();
    assert_eq!(config.get_separator(), "-");
    
    // Test date format
    config.set_date_format("YYYY-MM-DD").unwrap();
    assert_eq!(config.get_date_format(), "YYYY-MM-DD");
    
    // Test topic extractor name
    config.set_topic_extractor_name("advanced").unwrap();
    assert_eq!(config.get_topic_extractor_name(), "advanced");
    
    // Test templates
    config.add_template("technical", FilenameFormat::Custom(String::from("Tech_{main_topic}"))).unwrap();
    let template = config.get_template("technical").expect("Template not found");
    match template {
        FilenameFormat::Custom(format) => assert_eq!(format, "Tech_{main_topic}"),
        _ => panic!("Expected Custom format"),
    }
    
    // Test metadata
    config.add_metadata("category", "test").unwrap();
    assert_eq!(config.get_metadata().get("category"), Some(&String::from("test")));
    
    // Test serialization and deserialization
    let json = config.to_json().expect("Failed to serialize");
    let deserialized = SaveConfig::from_json(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.get_prefix(), "Custom_");
    assert_eq!(deserialized.get_separator(), "-");
    assert_eq!(deserialized.get_date_format(), "YYYY-MM-DD");
    assert_eq!(deserialized.get_topic_extractor_name(), "advanced");
    assert_eq!(deserialized.get_metadata().get("category"), Some(&String::from("test")));
}

/// Test all security features
#[test]
fn test_all_security_features() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    
    // Create security settings
    let mut settings = SecuritySettings::default();
    settings.redact_sensitive = true;
    settings.prevent_overwrite = true;
    settings.file_permissions = 0o644;
    settings.directory_permissions = 0o755;
    
    // Test path validation
    let valid_path = temp_dir.path().join("test.txt");
    let validated_path = validate_path(&valid_path, &settings).expect("Path validation failed");
    assert_eq!(validated_path, valid_path);
    
    // Test secure file writing
    write_secure_file(&valid_path, "test content", &settings).expect("Secure file writing failed");
    assert!(valid_path.exists());
    
    // Check file content
    let content = fs::read_to_string(&valid_path).expect("Failed to read file");
    assert_eq!(content, "test content");
    
    // Check file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&valid_path).expect("Failed to get metadata");
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, settings.file_permissions);
    }
}

/// Test integration checkpoints
#[test]
fn test_integration_checkpoints() {
    // Test integration checkpoint 1
    let result1 = integration_checkpoint_1::run_integration_checkpoint();
    assert!(result1.is_ok());
    
    // Test integration checkpoint 2
    let result2 = integration_checkpoint_2::run_integration_checkpoint();
    assert!(result2.is_ok());
    
    // Test integration checkpoint 3
    let result3 = integration_checkpoint_3::run_integration_checkpoint();
    assert!(result3.is_ok());
}

/// Test backward compatibility
#[test]
fn test_backward_compatibility() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Save with a specific filename (backward compatibility)
    let specific_path = temp_dir.path().join("specific-filename.q.json").to_string_lossy().to_string();
    let args = vec![specific_path.clone()];
    let result = commands::save::handle_save_command(&args, &conversation, &config);
    
    // Check that the file was saved to the specified path
    assert!(result.is_ok());
    let save_path = result.unwrap();
    assert_eq!(save_path, specific_path);
    assert!(Path::new(&save_path).exists());
}

/// Test error handling
#[test]
fn test_error_handling() {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config
    let mut config = SaveConfig::new(&config_path);
    
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Test invalid path
    let args = vec!["\0invalid".to_string()];
    let result = commands::save::handle_save_command(&args, &conversation, &config);
    assert!(result.is_err());
    
    // Test permission denied
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        
        // Create a directory with no write permissions
        let no_write_dir = temp_dir.path().join("no_write");
        fs::create_dir(&no_write_dir).unwrap();
        let mut perms = fs::metadata(&no_write_dir).unwrap().permissions();
        perms.set_mode(0o500); // r-x------
        fs::set_permissions(&no_write_dir, perms).unwrap();
        
        // Try to save to the directory
        let args = vec![no_write_dir.join("test.q.json").to_string_lossy().to_string()];
        let result = commands::save::handle_save_command(&args, &conversation, &config);
        
        // Reset permissions for cleanup
        let mut perms = fs::metadata(&no_write_dir).unwrap().permissions();
        perms.set_mode(0o700); // rwx------
        fs::set_permissions(&no_write_dir, perms).unwrap();
        
        // Check that the save failed
        assert!(result.is_err());
    }
}
