// integration_checkpoint_2.rs
// Integration checkpoint for Phase 2 of Amazon Q CLI automatic naming feature

use std::path::Path;
use tempfile::tempdir;
use crate::conversation::Conversation;
use crate::filename_generator::generate_filename;
use crate::topic_extractor::extract_topics;
use crate::save_config::SaveConfig;
use crate::commands::CommandRegistry;

/// Integration test for the save command implementation
pub fn test_integration() -> Result<(), String> {
    println!("Running integration checkpoint 2...");
    
    // Create a temporary directory for testing
    let temp_dir = match tempdir() {
        Ok(dir) => dir,
        Err(e) => return Err(format!("Failed to create temporary directory: {}", e)),
    };
    
    // Create a save config with a default path
    let config_path = temp_dir.path().join("config.json");
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    if let Err(e) = config.set_default_path(&default_path) {
        return Err(format!("Failed to set default path: {}", e));
    }
    
    // Create a command registry
    let registry = CommandRegistry::new(config);
    
    // Create a test conversation
    let mut conv = Conversation::new("test-integration".to_string());
    conv.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), Some("gpt-4".to_string()))
        .add_user_message("How do I save conversations automatically?".to_string())
        .add_assistant_message("Currently, you need to use the /save command with a filename.".to_string(), None)
        .add_user_message("Can we make it automatic?".to_string())
        .add_assistant_message("That would require implementing a new feature. Let me explain how it could work...".to_string(), None);
    
    // Extract topics
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    println!("Extracted topics: {} / {} / {}", main_topic, sub_topic, action_type);
    
    // Generate filename
    let filename = generate_filename(&conv);
    println!("Generated filename: {}", filename);
    
    // Execute the save command with no arguments (auto-generated filename)
    let result = registry.execute_command("save", &[], &conv);
    if let Err(e) = result {
        return Err(format!("Failed to execute save command: {}", e));
    }
    
    let save_path = result.unwrap();
    println!("Saved to: {}", save_path);
    
    // Check that the file exists
    if !Path::new(&save_path).exists() {
        return Err(format!("File does not exist: {}", save_path));
    }
    
    // Execute the save command with a directory path
    let custom_dir = temp_dir.path().join("custom").to_string_lossy().to_string();
    let result = registry.execute_command("save", &[format!("{}/", custom_dir)], &conv);
    if let Err(e) = result {
        return Err(format!("Failed to execute save command with directory path: {}", e));
    }
    
    let save_path = result.unwrap();
    println!("Saved to: {}", save_path);
    
    // Check that the file exists
    if !Path::new(&save_path).exists() {
        return Err(format!("File does not exist: {}", save_path));
    }
    
    // Execute the save command with a full path
    let full_path = temp_dir.path().join("my-conversation.q.json").to_string_lossy().to_string();
    let result = registry.execute_command("save", &[full_path.clone()], &conv);
    if let Err(e) = result {
        return Err(format!("Failed to execute save command with full path: {}", e));
    }
    
    let save_path = result.unwrap();
    println!("Saved to: {}", save_path);
    
    // Check that the file exists
    if !Path::new(&save_path).exists() {
        return Err(format!("File does not exist: {}", save_path));
    }
    
    println!("Integration checkpoint 2 passed!");
    Ok(())
}

/// Example usage of all components implemented so far
pub fn example_usage() {
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
    let mut conv = Conversation::new("example".to_string());
    conv.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
        .add_user_message("How do I save conversations automatically?".to_string());
    
    // Extract topics
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    println!("Main topic: {}", main_topic);
    println!("Sub topic: {}", sub_topic);
    println!("Action type: {}", action_type);
    
    // Generate filename
    let filename = generate_filename(&conv);
    println!("Generated filename: {}", filename);
    
    // Save the conversation
    let result = registry.execute_command("save", &[], &conv);
    if let Ok(save_path) = result {
        println!("Saved to: {}", save_path);
    } else {
        println!("Failed to save: {}", result.unwrap_err());
    }
    
    // Get help text for the save command
    let help_text = registry.get_help_text("save").unwrap();
    println!("Help text:\n{}", help_text);
}

/// Document integration issues and edge cases
pub fn document_issues() -> Vec<String> {
    vec![
        "Edge case: Empty conversations - Handled by providing default values".to_string(),
        "Edge case: Very short conversations - May not have enough content for accurate topic extraction".to_string(),
        "Edge case: Very long conversations - Only the first few user messages are used for topic extraction".to_string(),
        "Edge case: Multi-topic conversations - Only the first topic is used".to_string(),
        "Edge case: Conversations with code blocks - Code is included in topic extraction".to_string(),
        "Integration issue: The topic extractor uses a simple keyword-based approach, which may not always capture the true topic".to_string(),
        "Integration issue: The filename generator truncates long filenames, which may result in loss of information".to_string(),
        "Integration issue: The save command creates directories as needed, but this may fail due to permission issues".to_string(),
        "Integration issue: The save command does not check if a file already exists before saving, which may result in overwriting existing files".to_string(),
        "Integration issue: The command registry does not support command aliases, which may be confusing for users".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::mocks::create_mock_conversation;
    
    #[test]
    fn test_integration_checkpoint() {
        assert!(test_integration().is_ok());
    }
    
    #[test]
    fn test_with_all_mock_conversations() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create a save config with a default path
        let config_path = temp_dir.path().join("config.json");
        let mut config = SaveConfig::new(&config_path);
        let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
        config.set_default_path(&default_path).unwrap();
        
        // Create a command registry
        let registry = CommandRegistry::new(config);
        
        let conversation_types = vec![
            "empty",
            "simple",
            "amazon_q_cli",
            "feature_request",
            "technical",
            "multi_topic",
            "very_long",
        ];
        
        for conv_type in conversation_types {
            let conv = create_mock_conversation(conv_type);
            
            // Execute the save command
            let result = registry.execute_command("save", &[], &conv);
            assert!(result.is_ok());
            
            let save_path = result.unwrap();
            assert!(Path::new(&save_path).exists());
        }
    }
}
