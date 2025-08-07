// integration_checkpoint_1.rs
// Integration checkpoint for Phase 1 of Amazon Q CLI automatic naming feature

use crate::conversation::Conversation;
use crate::filename_generator::generate_filename;
use crate::topic_extractor::extract_topics;

/// Integration test for the filename generator and topic extractor
pub fn test_integration() -> Result<(), String> {
    println!("Running integration checkpoint 1...");
    
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
    
    // Verify the filename contains the extracted topics
    if !filename.contains(&main_topic) {
        return Err(format!("Filename does not contain main topic: {}", main_topic));
    }
    
    if !filename.contains(&sub_topic) {
        return Err(format!("Filename does not contain sub topic: {}", sub_topic));
    }
    
    if !filename.contains(&action_type) {
        return Err(format!("Filename does not contain action type: {}", action_type));
    }
    
    // Verify the filename format
    let parts: Vec<&str> = filename.split(" - ").collect();
    if parts.len() != 2 {
        return Err(format!("Filename does not have the correct format: {}", filename));
    }
    
    let base = parts[0];
    let date = parts[1];
    
    if !base.starts_with("Q_") {
        return Err(format!("Filename does not start with 'Q_': {}", filename));
    }
    
    if date.len() != 11 {
        return Err(format!("Date part does not have the correct length: {}", date));
    }
    
    println!("Integration checkpoint 1 passed!");
    Ok(())
}

/// Example usage of the filename generator and topic extractor
pub fn example_usage() {
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
            
            // Extract topics
            let (main_topic, sub_topic, action_type) = extract_topics(&conv);
            
            // Generate filename
            let filename = generate_filename(&conv);
            
            // Verify the filename format
            let parts: Vec<&str> = filename.split(" - ").collect();
            assert_eq!(parts.len(), 2);
            
            let base = parts[0];
            let date = parts[1];
            
            assert!(base.starts_with("Q_"));
            assert_eq!(date.len(), 11);
            
            // If topics were extracted, verify they're in the filename
            if !main_topic.is_empty() {
                assert!(filename.contains(&main_topic) || filename.contains(&main_topic.to_lowercase()));
            }
            
            if !sub_topic.is_empty() && sub_topic != "General" {
                assert!(filename.contains(&sub_topic) || filename.contains(&sub_topic.to_lowercase()));
            }
            
            if action_type != "Conversation" {
                assert!(filename.contains(&action_type) || filename.contains(&action_type.to_lowercase()));
            }
        }
    }
}
