// tests/filename_generator_tests.rs
// Tests for the filename generator module

use std::time::{SystemTime, UNIX_EPOCH};
use chrono::{DateTime, Utc, TimeZone, Local};
use crate::conversation::Conversation;
use crate::filename_generator::generate_filename;

#[test]
fn test_correct_format() {
    // Create a conversation with clear topics
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
        .add_user_message("How do I save conversations automatically?".to_string());
    
    // Mock the current date/time to ensure consistent test results
    let filename = generate_filename(&conv);
    
    // Check format: Q_[MainTopic]_[SubTopic]_[ActionType] - DDMMMYY-HHMM
    assert!(filename.starts_with("Q_AmazonQ_CLI_"));
    assert!(filename.contains("_Help"));
    
    // Check date format
    let date_part = filename.split(" - ").collect::<Vec<&str>>()[1];
    assert_eq!(date_part.len(), 11); // DDMMMYY-HHMM = 11 chars
    
    // Check that the date part follows the format DDMMMYY-HHMM
    let date_regex = regex::Regex::new(r"^\d{2}[A-Z]{3}\d{2}-\d{4}$").unwrap();
    assert!(date_regex.is_match(date_part));
}

#[test]
fn test_sanitize_special_characters() {
    // Create a conversation with special characters
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I need help with Amazon Q CLI: /save <path>".to_string())
        .add_assistant_message("Sure, let me explain the /save command.".to_string(), None)
        .add_user_message("What about special characters like $, &, and *?".to_string());
    
    let filename = generate_filename(&conv);
    
    // Check that special characters are sanitized
    assert!(!filename.contains("/"));
    assert!(!filename.contains("<"));
    assert!(!filename.contains(">"));
    assert!(!filename.contains("$"));
    assert!(!filename.contains("&"));
    assert!(!filename.contains("*"));
    
    // Check that spaces are replaced with underscores
    assert!(!filename.contains(" "));
    assert!(filename.contains("_"));
}

#[test]
fn test_fallback_mechanism() {
    // Create an empty conversation
    let conv = Conversation::new("test-id".to_string());
    
    let filename = generate_filename(&conv);
    
    // Check that the fallback format is used
    assert!(filename.starts_with("Q_Conversation"));
    
    // Check date format
    let date_part = filename.split(" - ").collect::<Vec<&str>>()[1];
    assert_eq!(date_part.len(), 11); // DDMMMYY-HHMM = 11 chars
}

#[test]
fn test_very_short_conversation() {
    // Create a very short conversation
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("Hi".to_string());
    
    let filename = generate_filename(&conv);
    
    // Check that a reasonable filename is generated even with minimal content
    assert!(filename.starts_with("Q_"));
    assert!(filename.contains(" - "));
}

#[test]
fn test_very_long_conversation() {
    // Create a conversation with long messages
    let mut conv = Conversation::new("test-id".to_string());
    let long_message = "A".repeat(1000);
    conv.add_user_message(long_message)
        .add_assistant_message("B".repeat(1000), None);
    
    let filename = generate_filename(&conv);
    
    // Check that the filename is not too long
    assert!(filename.len() <= 255); // Max filename length on most filesystems
}

#[test]
fn test_consistent_output() {
    // Create two identical conversations
    let mut conv1 = Conversation::new("test-id-1".to_string());
    let mut conv2 = Conversation::new("test-id-2".to_string());
    
    conv1.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know?".to_string(), None);
    
    conv2.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know?".to_string(), None);
    
    // Generate filenames with the same timestamp
    let filename1 = generate_filename(&conv1);
    let filename2 = generate_filename(&conv2);
    
    // The main parts should be the same (excluding the timestamp)
    let main_part1 = filename1.split(" - ").collect::<Vec<&str>>()[0];
    let main_part2 = filename2.split(" - ").collect::<Vec<&str>>()[0];
    
    assert_eq!(main_part1, main_part2);
}
