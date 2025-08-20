// commands/save.rs
// Enhanced save command for Amazon Q CLI automatic naming feature

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use crate::conversation::Conversation;
use crate::filename_generator::{generate_filename, generate_filename_with_config, generate_filename_with_template, generate_filename_with_extractor};
use crate::save_config::SaveConfig;
use crate::topic_extractor::{self, basic, enhanced, advanced};
use crate::security::{SecuritySettings, SecurityError, validate_path, write_secure_file, redact_sensitive_information, generate_unique_filename};

/// Error type for save command operations
#[derive(Debug)]
pub enum SaveError {
    /// I/O error
    Io(io::Error),
    /// Invalid path
    InvalidPath(String),
    /// Serialization error
    Serialization(serde_json::Error),
    /// Configuration error
    Config(String),
    /// Security error
    Security(SecurityError),
}

impl From<io::Error> for SaveError {
    fn from(err: io::Error) -> Self {
        SaveError::Io(err)
    }
}

impl From<serde_json::Error> for SaveError {
    fn from(err: serde_json::Error) -> Self {
        SaveError::Serialization(err)
    }
}

impl From<SecurityError> for SaveError {
    fn from(err: SecurityError) -> Self {
        SaveError::Security(err)
    }
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(err) => write!(f, "I/O error: {}", err),
            SaveError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            SaveError::Serialization(err) => write!(f, "Serialization error: {}", err),
            SaveError::Config(err) => write!(f, "Configuration error: {}", err),
            SaveError::Security(err) => write!(f, "Security error: {}", err),
        }
    }
}

impl std::error::Error for SaveError {}

/// Handle the save command
///
/// Supports three usage patterns:
/// - `/save` (auto-generate filename and use default location)
/// - `/save <directory_path>` (use directory with auto-generated filename)
/// - `/save <full_path_with_filename>` (backward compatibility)
pub fn handle_save_command(
    args: &[String],
    conversation: &Conversation,
    config: &SaveConfig,
) -> Result<String, SaveError> {
    handle_save_command_with_extractor(args, conversation, config, &topic_extractor::extract_topics)
}

/// Handle the save command with a specific topic extractor
pub fn handle_save_command_with_extractor(
    args: &[String],
    conversation: &Conversation,
    config: &SaveConfig,
    extractor: &fn(&Conversation) -> (String, String, String),
) -> Result<String, SaveError> {
    // Parse additional options
    let (args, options) = parse_save_options(args);
    
    // Create security settings
    let security_settings = create_security_settings(&options, config);
    
    // Determine the save path
    let save_path = if args.is_empty() {
        // Auto-generate filename and use default path
        let default_dir = config.get_default_path();
        
        // Ensure directory exists
        let default_dir_path = Path::new(&default_dir);
        if !default_dir_path.exists() {
            fs::create_dir_all(default_dir_path)?;
        }
        
        // Generate filename based on options
        let filename = if let Some(template) = options.get("template") {
            generate_filename_with_template(conversation, config, template)
        } else if options.contains_key("config") {
            generate_filename_with_config(conversation, config)
        } else {
            generate_filename_with_extractor(conversation, extractor)
        };
        
        let mut path = PathBuf::from(&default_dir);
        path.push(format!("{}.q.json", filename));
        path
    } else if args[0].ends_with('/') || Path::new(&args[0]).is_dir() {
        // Custom directory with auto-generated filename
        let custom_dir = &args[0];
        
        // Ensure directory exists
        let custom_dir_path = Path::new(custom_dir);
        if !custom_dir_path.exists() {
            fs::create_dir_all(custom_dir_path)?;
        }
        
        // Generate filename based on options
        let filename = if let Some(template) = options.get("template") {
            generate_filename_with_template(conversation, config, template)
        } else if options.contains_key("config") {
            generate_filename_with_config(conversation, config)
        } else {
            generate_filename_with_extractor(conversation, extractor)
        };
        
        let mut path = PathBuf::from(custom_dir);
        path.push(format!("{}.q.json", filename));
        path
    } else {
        // Full path specified (backward compatibility)
        let path = PathBuf::from(&args[0]);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        
        path
    };
    
    // Validate the path
    let validated_path = validate_path(&save_path, &security_settings)?;
    
    // Generate a unique filename if needed
    let final_path = if security_settings.prevent_overwrite && validated_path.exists() {
        generate_unique_filename(&validated_path)
    } else {
        validated_path
    };
    
    // Save the conversation
    save_conversation_to_file(conversation, &final_path, config, &options, &security_settings)?;
    
    Ok(final_path.to_string_lossy().to_string())
}

/// Parse save command options
fn parse_save_options(args: &[String]) -> (Vec<String>, HashMap<String, String>) {
    let mut options = HashMap::new();
    let mut filtered_args = Vec::new();
    
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            let option = args[i][2..].to_string();
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                options.insert(option, args[i + 1].clone());
                i += 2;
            } else {
                options.insert(option, String::new());
                i += 1;
            }
        } else {
            filtered_args.push(args[i].clone());
            i += 1;
        }
    }
    
    (filtered_args, options)
}

/// Create security settings from options and config
fn create_security_settings(options: &HashMap<String, String>, config: &SaveConfig) -> SecuritySettings {
    let mut settings = SecuritySettings::default();
    
    // Set redact_sensitive from options or config
    settings.redact_sensitive = options.contains_key("redact") || 
        config.get_metadata().get("redact_sensitive").map_or(false, |v| v == "true");
    
    // Set prevent_overwrite from options or config
    settings.prevent_overwrite = options.contains_key("no-overwrite") || 
        config.get_metadata().get("prevent_overwrite").map_or(false, |v| v == "true");
    
    // Set follow_symlinks from options or config
    settings.follow_symlinks = options.contains_key("follow-symlinks") || 
        config.get_metadata().get("follow_symlinks").map_or(false, |v| v == "true");
    
    // Set file_permissions from options or config
    if let Some(perms) = options.get("file-permissions") {
        if let Ok(mode) = u32::from_str_radix(perms, 8) {
            settings.file_permissions = mode;
        }
    } else if let Some(perms) = config.get_metadata().get("file_permissions") {
        if let Ok(mode) = u32::from_str_radix(perms, 8) {
            settings.file_permissions = mode;
        }
    }
    
    // Set directory_permissions from options or config
    if let Some(perms) = options.get("dir-permissions") {
        if let Ok(mode) = u32::from_str_radix(perms, 8) {
            settings.directory_permissions = mode;
        }
    } else if let Some(perms) = config.get_metadata().get("directory_permissions") {
        if let Ok(mode) = u32::from_str_radix(perms, 8) {
            settings.directory_permissions = mode;
        }
    }
    
    settings
}

/// Save a conversation to a file
pub fn save_conversation_to_file(
    conversation: &Conversation,
    path: &Path,
    config: &SaveConfig,
    options: &HashMap<String, String>,
    security_settings: &SecuritySettings,
) -> Result<(), SaveError> {
    // Add custom metadata if specified
    let mut conversation_with_metadata = conversation.clone();
    
    // Add metadata from config
    for (key, value) in config.get_metadata() {
        conversation_with_metadata.add_metadata(key, value);
    }
    
    // Add metadata from options
    if let Some(metadata) = options.get("metadata") {
        for pair in metadata.split(',') {
            let parts: Vec<&str> = pair.split('=').collect();
            if parts.len() == 2 {
                conversation_with_metadata.add_metadata(parts[0], parts[1]);
            }
        }
    }
    
    // Redact sensitive information if enabled
    if security_settings.redact_sensitive {
        conversation_with_metadata = redact_conversation(&conversation_with_metadata);
    }
    
    // Serialize the conversation
    let content = serde_json::to_string_pretty(&conversation_with_metadata)?;
    
    // Write to file securely
    write_secure_file(path, &content, security_settings)?;
    
    Ok(())
}

/// Redact sensitive information from a conversation
fn redact_conversation(conversation: &Conversation) -> Conversation {
    let mut redacted = conversation.clone();
    
    // Redact user messages
    for message in &mut redacted.messages {
        if message.role == "user" {
            message.content = redact_sensitive_information(&message.content);
        }
    }
    
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::tests::mocks::create_mock_conversation;
    use crate::save_config::FilenameFormat;
    
    #[test]
    fn test_auto_generate_filename() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config with a default path
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Call the save command with no arguments
        let args = Vec::<String>::new();
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that the file was saved to the default path
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert!(save_path.starts_with(&default_path));
        assert!(Path::new(&save_path).exists());
        
        // Check that the file contains the conversation
        let content = fs::read_to_string(save_path).unwrap();
        let saved_conv: Conversation = serde_json::from_str(&content).unwrap();
        assert_eq!(saved_conv.id, conv.id);
        assert_eq!(saved_conv.messages.len(), conv.messages.len());
    }
    
    #[test]
    fn test_custom_directory() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let config = SaveConfig::new(&config_path);
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
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
    fn test_full_path() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let config = SaveConfig::new(&config_path);
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
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
    fn test_create_nested_directories() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let config = SaveConfig::new(&config_path);
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
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
    fn test_invalid_path() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let config = SaveConfig::new(&config_path);
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Call the save command with an invalid path
        let args = vec!["\0invalid".to_string()];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that an invalid path error was returned
        assert!(result.is_err());
        match result.unwrap_err() {
            SaveError::InvalidPath(_) => (),
            SaveError::Security(_) => (),
            err => panic!("Unexpected error: {:?}", err),
        }
    }
    
    #[test]
    fn test_save_with_template() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config with a template
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        config.add_template(
            "technical",
            FilenameFormat::Custom(String::from("Tech_{main_topic}_{date}"))
        ).unwrap();
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Call the save command with the template option
        let args = vec!["--template".to_string(), "technical".to_string()];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that the file was saved with the template format
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert!(save_path.contains("Tech_"));
        assert!(Path::new(&save_path).exists());
    }
    
    #[test]
    fn test_save_with_config() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config with custom settings
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        config.set_prefix("Custom_").unwrap();
        config.set_separator("-").unwrap();
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Call the save command with the config option
        let args = vec!["--config".to_string()];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that the file was saved with the config settings
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert!(save_path.contains("Custom_"));
        assert!(save_path.contains("-"));
        assert!(Path::new(&save_path).exists());
    }
    
    #[test]
    fn test_save_with_metadata() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        
        // Add metadata to config
        config.add_metadata("category", "test").unwrap();
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Call the save command with metadata option
        let args = vec!["--metadata".to_string(), "priority=high,tag=important".to_string()];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that the file was saved with metadata
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert!(Path::new(&save_path).exists());
        
        // Check that the file contains the metadata
        let content = fs::read_to_string(save_path).unwrap();
        let saved_conv: Conversation = serde_json::from_str(&content).unwrap();
        assert_eq!(saved_conv.get_metadata("category"), Some("test"));
        assert_eq!(saved_conv.get_metadata("priority"), Some("high"));
        assert_eq!(saved_conv.get_metadata("tag"), Some("important"));
    }
    
    #[test]
    fn test_save_with_redaction() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        
        // Create a conversation with sensitive information
        let mut conv = Conversation::new("test-id".to_string());
        conv.add_user_message("My credit card is 1234-5678-9012-3456".to_string());
        
        // Call the save command with redaction option
        let args = vec!["--redact".to_string()];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that the file was saved with redacted content
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert!(Path::new(&save_path).exists());
        
        // Check that the file contains redacted content
        let content = fs::read_to_string(save_path).unwrap();
        assert!(!content.contains("1234-5678-9012-3456"));
        assert!(content.contains("[REDACTED CREDIT CARD]"));
    }
    
    #[test]
    fn test_save_with_no_overwrite() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let config = SaveConfig::new(&config_path);
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Create a file that we don't want to overwrite
        let file_path = temp_dir.path().join("existing.q.json");
        fs::write(&file_path, "Original content").unwrap();
        
        // Call the save command with no-overwrite option
        let args = vec![file_path.to_string_lossy().to_string(), "--no-overwrite".to_string()];
        let result = handle_save_command(&args, &conv, &config);
        
        // Check that the file was saved with a different name
        assert!(result.is_ok());
        let save_path = result.unwrap();
        assert_ne!(save_path, file_path.to_string_lossy().to_string());
        assert!(Path::new(&save_path).exists());
        
        // Check that the original file is unchanged
        let original_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(original_content, "Original content");
    }
    
    #[test]
    fn test_save_with_different_extractors() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.json");
        
        // Create a save config
        let config = SaveConfig::new(&config_path);
        
        // Create a conversation
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Test with basic extractor
        let basic_result = handle_save_command_with_extractor(
            &Vec::<String>::new(),
            &conv,
            &config,
            &basic::extract_topics
        );
        
        // Test with enhanced extractor
        let enhanced_result = handle_save_command_with_extractor(
            &Vec::<String>::new(),
            &conv,
            &config,
            &enhanced::extract_topics
        );
        
        // Test with advanced extractor
        let advanced_result = handle_save_command_with_extractor(
            &Vec::<String>::new(),
            &conv,
            &config,
            &advanced::extract_topics
        );
        
        // Check that all saves were successful
        assert!(basic_result.is_ok());
        assert!(enhanced_result.is_ok());
        assert!(advanced_result.is_ok());
        
        // Check that the files exist
        assert!(Path::new(&basic_result.unwrap()).exists());
        assert!(Path::new(&enhanced_result.unwrap()).exists());
        assert!(Path::new(&advanced_result.unwrap()).exists());
    }
    
    #[test]
    fn test_parse_save_options() {
        // Test with no options
        let args = vec!["path".to_string()];
        let (filtered_args, options) = parse_save_options(&args);
        assert_eq!(filtered_args, vec!["path".to_string()]);
        assert!(options.is_empty());
        
        // Test with options
        let args = vec![
            "--template".to_string(),
            "technical".to_string(),
            "path".to_string(),
            "--config".to_string(),
        ];
        let (filtered_args, options) = parse_save_options(&args);
        assert_eq!(filtered_args, vec!["path".to_string()]);
        assert_eq!(options.get("template"), Some(&"technical".to_string()));
        assert!(options.contains_key("config"));
        
        // Test with options and no value
        let args = vec![
            "--template".to_string(),
            "--config".to_string(),
            "path".to_string(),
        ];
        let (filtered_args, options) = parse_save_options(&args);
        assert_eq!(filtered_args, vec!["path".to_string()]);
        assert_eq!(options.get("template"), Some(&String::new()));
        assert!(options.contains_key("config"));
    }
    
    #[test]
    fn test_create_security_settings() {
        // Create a save config
        let mut config = SaveConfig::new("/tmp/config.json");
        
        // Test default settings
        let options = HashMap::new();
        let settings = create_security_settings(&options, &config);
        assert!(!settings.redact_sensitive);
        assert!(!settings.prevent_overwrite);
        assert!(!settings.follow_symlinks);
        assert_eq!(settings.file_permissions, 0o600);
        assert_eq!(settings.directory_permissions, 0o700);
        
        // Test settings from options
        let mut options = HashMap::new();
        options.insert("redact".to_string(), String::new());
        options.insert("no-overwrite".to_string(), String::new());
        options.insert("follow-symlinks".to_string(), String::new());
        options.insert("file-permissions".to_string(), "644".to_string());
        options.insert("dir-permissions".to_string(), "755".to_string());
        
        let settings = create_security_settings(&options, &config);
        assert!(settings.redact_sensitive);
        assert!(settings.prevent_overwrite);
        assert!(settings.follow_symlinks);
        assert_eq!(settings.file_permissions, 0o644);
        assert_eq!(settings.directory_permissions, 0o755);
        
        // Test settings from config
        config.add_metadata("redact_sensitive", "true").unwrap();
        config.add_metadata("prevent_overwrite", "true").unwrap();
        config.add_metadata("follow_symlinks", "true").unwrap();
        config.add_metadata("file_permissions", "644").unwrap();
        config.add_metadata("directory_permissions", "755").unwrap();
        
        let options = HashMap::new();
        let settings = create_security_settings(&options, &config);
        assert!(settings.redact_sensitive);
        assert!(settings.prevent_overwrite);
        assert!(settings.follow_symlinks);
        assert_eq!(settings.file_permissions, 0o644);
        assert_eq!(settings.directory_permissions, 0o755);
    }
    
    #[test]
    fn test_redact_conversation() {
        // Create a conversation with sensitive information
        let mut conv = Conversation::new("test-id".to_string());
        conv.add_user_message("My credit card is 1234-5678-9012-3456".to_string());
        conv.add_assistant_message("I'll help you with that.".to_string(), None);
        
        // Redact the conversation
        let redacted = redact_conversation(&conv);
        
        // Check that user messages are redacted
        assert!(!redacted.messages[0].content.contains("1234-5678-9012-3456"));
        assert!(redacted.messages[0].content.contains("[REDACTED CREDIT CARD]"));
        
        // Check that assistant messages are not redacted
        assert_eq!(redacted.messages[1].content, "I'll help you with that.");
    }
}
