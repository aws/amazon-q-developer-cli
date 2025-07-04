#![feature(test)]

extern crate test;
extern crate amazon_q_cli_auto_naming;

use test::Bencher;
use amazon_q_cli_auto_naming::{
    Conversation,
    SaveConfig,
    filename_generator,
    topic_extractor,
    commands,
    security::{SecuritySettings, redact_sensitive_information},
};
use std::path::Path;
use tempfile::tempdir;

/// Benchmark topic extraction with basic extractor
#[bench]
fn bench_basic_topic_extraction(b: &mut Bencher) {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    b.iter(|| {
        topic_extractor::basic::extract_topics(&conversation)
    });
}

/// Benchmark topic extraction with enhanced extractor
#[bench]
fn bench_enhanced_topic_extraction(b: &mut Bencher) {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    b.iter(|| {
        topic_extractor::enhanced::extract_topics(&conversation)
    });
}

/// Benchmark topic extraction with advanced extractor
#[bench]
fn bench_advanced_topic_extraction(b: &mut Bencher) {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    b.iter(|| {
        topic_extractor::advanced::extract_topics(&conversation)
    });
}

/// Benchmark filename generation
#[bench]
fn bench_filename_generation(b: &mut Bencher) {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    b.iter(|| {
        filename_generator::generate_filename(&conversation)
    });
}

/// Benchmark filename generation with advanced extractor
#[bench]
fn bench_filename_generation_with_advanced_extractor(b: &mut Bencher) {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    b.iter(|| {
        filename_generator::generate_filename_with_extractor(&conversation, &topic_extractor::advanced::extract_topics)
    });
}

/// Benchmark filename generation with custom format
#[bench]
fn bench_filename_generation_with_custom_format(b: &mut Bencher) {
    // Create a conversation
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("I need help with Amazon Q CLI automatic naming feature".to_string());
    conversation.add_assistant_message("Sure, I can help you with that. What would you like to know?".to_string(), None);
    conversation.add_user_message("How do I save a conversation with an automatically generated filename?".to_string());
    
    // Create a save config with a custom format
    let mut config = SaveConfig::new("/tmp/config.json");
    config.set_filename_format(amazon_q_cli_auto_naming::save_config::FilenameFormat::Custom(
        String::from("{main_topic}-{sub_topic}-{action_type}-{date}")
    )).unwrap();
    
    b.iter(|| {
        filename_generator::generate_filename_with_config(&conversation, &config)
    });
}

/// Benchmark sensitive information redaction
#[bench]
fn bench_sensitive_information_redaction(b: &mut Bencher) {
    // Create a text with sensitive information
    let text = "My credit card is 1234-5678-9012-3456, my SSN is 123-45-6789, \
                my API key is abcdefghijklmnopqrstuvwxyz1234567890abcdef, \
                my AWS key is AKIAIOSFODNN7EXAMPLE, \
                password = secret123";
    
    b.iter(|| {
        redact_sensitive_information(text)
    });
}

/// Benchmark save command
#[bench]
fn bench_save_command(b: &mut Bencher) {
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
    
    // Create a unique path for each iteration
    let mut counter = 0;
    
    b.iter(|| {
        // Create a unique path for this iteration
        let path = temp_dir.path().join(format!("test{}.q.json", counter));
        counter += 1;
        
        // Call the save command with the path
        let args = vec![path.to_string_lossy().to_string()];
        commands::save::handle_save_command(&args, &conversation, &config)
    });
}

/// Benchmark save command with redaction
#[bench]
fn bench_save_command_with_redaction(b: &mut Bencher) {
    // Create a temporary directory for testing
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("config.json");
    
    // Create a save config with a default path
    let mut config = SaveConfig::new(&config_path);
    let default_path = temp_dir.path().join("qChats").to_string_lossy().to_string();
    config.set_default_path(&default_path).unwrap();
    
    // Create a conversation with sensitive information
    let mut conversation = Conversation::new("test-id".to_string());
    conversation.add_user_message("My credit card is 1234-5678-9012-3456".to_string());
    conversation.add_assistant_message("I'll help you with that.".to_string(), None);
    
    // Create a unique path for each iteration
    let mut counter = 0;
    
    b.iter(|| {
        // Create a unique path for this iteration
        let path = temp_dir.path().join(format!("test{}.q.json", counter));
        counter += 1;
        
        // Call the save command with the path and redaction option
        let args = vec![path.to_string_lossy().to_string(), "--redact".to_string()];
        commands::save::handle_save_command(&args, &conversation, &config)
    });
}
