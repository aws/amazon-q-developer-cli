// tests/user_config_tests.rs
// Tests for user configuration options

use crate::conversation::Conversation;
use crate::filename_generator;
use crate::save_config::{SaveConfig, FilenameFormat};
use crate::topic_extractor::advanced;
use std::path::PathBuf;

#[test]
fn test_custom_filename_format() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
        .add_user_message("How do I save conversations automatically?".to_string());
    
    // Create a save config with default format
    let default_config = SaveConfig::new();
    
    // Generate filename with default format
    let default_filename = filename_generator::generate_filename_with_config(&conversation, &default_config);
    
    // Create a save config with custom format
    let mut custom_config = SaveConfig::new();
    custom_config.set_filename_format(FilenameFormat::Custom(
        String::from("[{main_topic}] {action_type} - {date}")
    ));
    
    // Generate filename with custom format
    let custom_filename = filename_generator::generate_filename_with_config(&conversation, &custom_config);
    
    // Verify that the custom format is applied
    assert_ne!(default_filename, custom_filename);
    assert!(custom_filename.starts_with("[AmazonQ]"));
    assert!(custom_filename.contains("Help") || custom_filename.contains("Learning"));
}

#[test]
fn test_custom_date_format() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a save config with custom date format
    let mut custom_config = SaveConfig::new();
    custom_config.set_date_format(String::from("YYYY-MM-DD"));
    
    // Generate filename with custom date format
    let custom_filename = filename_generator::generate_filename_with_config(&conversation, &custom_config);
    
    // Extract the date part
    let parts: Vec<&str> = custom_filename.split(" - ").collect();
    assert_eq!(parts.len(), 2);
    
    // Verify that the date format is YYYY-MM-DD
    let date_part = parts[1].replace(".q.json", "");
    assert!(date_part.matches('-').count() == 2);
    assert_eq!(date_part.len(), 10); // YYYY-MM-DD = 10 chars
}

#[test]
fn test_custom_separator() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a save config with custom separator
    let mut custom_config = SaveConfig::new();
    custom_config.set_separator(String::from("__"));
    
    // Generate filename with custom separator
    let custom_filename = filename_generator::generate_filename_with_config(&conversation, &custom_config);
    
    // Verify that the custom separator is applied
    assert!(custom_filename.contains("__"));
    assert!(!custom_filename.contains("_"));
}

#[test]
fn test_custom_prefix() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a save config with custom prefix
    let mut custom_config = SaveConfig::new();
    custom_config.set_prefix(String::from("Chat_"));
    
    // Generate filename with custom prefix
    let custom_filename = filename_generator::generate_filename_with_config(&conversation, &custom_config);
    
    // Verify that the custom prefix is applied
    assert!(custom_filename.starts_with("Chat_"));
    assert!(!custom_filename.starts_with("Q_"));
}

#[test]
fn test_save_templates() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a save config with a template
    let mut config_with_templates = SaveConfig::new();
    config_with_templates.add_template(
        String::from("technical"),
        FilenameFormat::Custom(String::from("Tech_{main_topic}_{sub_topic}_{date}"))
    );
    
    // Generate filename with the template
    let template_filename = filename_generator::generate_filename_with_template(
        &conversation,
        &config_with_templates,
        "technical"
    );
    
    // Verify that the template is applied
    assert!(template_filename.starts_with("Tech_"));
    
    // Test with a non-existent template (should fall back to default)
    let default_filename = filename_generator::generate_filename_with_template(
        &conversation,
        &config_with_templates,
        "non_existent"
    );
    
    // Verify that the default format is used
    assert!(default_filename.starts_with("Q_"));
}

#[test]
fn test_custom_metadata() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("Test message".to_string());
    
    // Create a save config with custom metadata
    let mut custom_config = SaveConfig::new();
    custom_config.add_metadata("category", String::from("test"));
    custom_config.add_metadata("priority", String::from("high"));
    
    // Get the metadata
    let metadata = custom_config.get_metadata();
    
    // Verify that the custom metadata is present
    assert_eq!(metadata.get("category"), Some(&String::from("test")));
    assert_eq!(metadata.get("priority"), Some(&String::from("high")));
}

#[test]
fn test_default_save_path() {
    // Create a save config with default path
    let mut config = SaveConfig::new();
    config.set_default_path(PathBuf::from("/custom/path"));
    
    // Get the default path
    let default_path = config.get_default_path();
    
    // Verify that the default path is set correctly
    assert_eq!(default_path, PathBuf::from("/custom/path"));
}

#[test]
fn test_config_persistence() {
    // Create a save config with custom settings
    let mut config = SaveConfig::new();
    config.set_prefix(String::from("Custom_"));
    config.set_separator(String::from("-"));
    config.set_date_format(String::from("YYYY/MM/DD"));
    config.add_metadata("category", String::from("test"));
    
    // Serialize the config to JSON
    let json = config.to_json().expect("Failed to serialize config");
    
    // Deserialize the JSON back to a config
    let deserialized_config = SaveConfig::from_json(&json).expect("Failed to deserialize config");
    
    // Verify that the deserialized config has the same settings
    assert_eq!(deserialized_config.get_prefix(), "Custom_");
    assert_eq!(deserialized_config.get_separator(), "-");
    assert_eq!(deserialized_config.get_date_format(), "YYYY/MM/DD");
    assert_eq!(
        deserialized_config.get_metadata().get("category"),
        Some(&String::from("test"))
    );
}

#[test]
fn test_filename_with_extractor_override() {
    // Create a test conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI".to_string());
    
    // Create a save config with a custom topic extractor
    let mut custom_config = SaveConfig::new();
    custom_config.set_topic_extractor_name(String::from("advanced"));
    
    // Generate filename with the custom topic extractor
    let filename = filename_generator::generate_filename_with_config(&conversation, &custom_config);
    
    // Generate filename with the advanced extractor directly
    let advanced_filename = filename_generator::generate_filename_with_extractor(
        &conversation,
        &advanced::extract_topics
    );
    
    // Verify that the filenames are the same
    assert_eq!(filename, advanced_filename);
}
