// tests/topic_extractor_tests.rs
// Tests for the topic extractor module

use crate::conversation::Conversation;
use crate::topic_extractor::extract_topics;

#[test]
fn test_main_topic_identification() {
    // Create a conversation about Amazon Q CLI
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
        .add_user_message("How do I save conversations automatically?".to_string());
    
    let (main_topic, _, _) = extract_topics(&conv);
    
    assert_eq!(main_topic, "AmazonQ");
}

#[test]
fn test_subtopic_identification() {
    // Create a conversation about Amazon Q CLI save feature
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I need help with Amazon Q CLI".to_string())
        .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
        .add_user_message("How do I save conversations automatically?".to_string())
        .add_assistant_message("Currently, you need to use the /save command with a filename.".to_string(), None)
        .add_user_message("Can we make it automatic?".to_string());
    
    let (_, sub_topic, _) = extract_topics(&conv);
    
    assert_eq!(sub_topic, "CLI");
}

#[test]
fn test_action_type_identification() {
    // Create a conversation about a feature request
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I think Amazon Q CLI should automatically name saved conversations".to_string())
        .add_assistant_message("That's an interesting feature request. How would you like it to work?".to_string(), None)
        .add_user_message("It should generate names based on the conversation content".to_string());
    
    let (_, _, action_type) = extract_topics(&conv);
    
    assert_eq!(action_type, "FeatureRequest");
}

#[test]
fn test_technical_conversation() {
    // Create a technical conversation
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("How do I implement a Rust function to parse JSON?".to_string())
        .add_assistant_message("You can use the serde_json crate. Here's an example:".to_string(), None)
        .add_user_message("Can you show me how to handle errors?".to_string());
    
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    assert_eq!(main_topic, "Rust");
    assert_eq!(sub_topic, "JSON");
    assert!(action_type == "Help" || action_type == "Code");
}

#[test]
fn test_multi_topic_conversation() {
    // Create a conversation that covers multiple topics
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("How do I use Amazon Q CLI?".to_string())
        .add_assistant_message("Here's how to use Amazon Q CLI...".to_string(), None)
        .add_user_message("What about AWS Lambda functions?".to_string())
        .add_assistant_message("AWS Lambda is a serverless compute service...".to_string(), None);
    
    let (main_topic, sub_topic, _) = extract_topics(&conv);
    
    // The main topic should be from the first few messages
    assert_eq!(main_topic, "AmazonQ");
    assert_eq!(sub_topic, "CLI");
}

#[test]
fn test_empty_conversation() {
    // Create an empty conversation
    let conv = Conversation::new("test-id".to_string());
    
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Should return empty strings or default values
    assert!(main_topic.is_empty());
    assert!(sub_topic.is_empty());
    assert_eq!(action_type, "Conversation");
}

#[test]
fn test_very_short_conversation() {
    // Create a very short conversation
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("Hi".to_string());
    
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Should handle minimal content gracefully
    assert!(!main_topic.is_empty() || !sub_topic.is_empty() || !action_type.is_empty());
}

#[test]
fn test_conversation_with_code() {
    // Create a conversation with code blocks
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("How do I write a hello world program in Rust?".to_string())
        .add_assistant_message(r#"
Here's a simple hello world program in Rust:

```rust
fn main() {
    println!("Hello, world!");
}
```

You can compile and run it with `rustc` and then execute the binary.
"#.to_string(), None);
    
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    assert_eq!(main_topic, "Rust");
    assert!(sub_topic == "Programming" || sub_topic == "Code");
    assert_eq!(action_type, "Help");
}
