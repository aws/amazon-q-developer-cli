// tests/enhanced_topic_extractor_tests.rs
// Tests for the enhanced topic extractor module

use crate::conversation::Conversation;
use crate::topic_extractor::enhanced::{extract_topics, extract_keywords, analyze_sentiment, detect_language};
use crate::tests::mocks::create_mock_conversation;

#[test]
fn test_enhanced_keyword_extraction() {
    // Create a conversation about AWS services
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I'm trying to set up an AWS Lambda function that processes data from an S3 bucket and stores the results in DynamoDB.".to_string())
        .add_assistant_message("That's a common serverless pattern. Let me help you with that.".to_string(), None)
        .add_user_message("I'm having trouble with the IAM permissions for the Lambda function.".to_string());
    
    // Extract keywords using the enhanced extractor
    let keywords = extract_keywords(&conv);
    
    // Check that important technical terms are extracted
    assert!(keywords.contains(&"aws".to_string()) || keywords.contains(&"AWS".to_string()));
    assert!(keywords.contains(&"lambda".to_string()) || keywords.contains(&"Lambda".to_string()));
    assert!(keywords.contains(&"s3".to_string()) || keywords.contains(&"S3".to_string()));
    assert!(keywords.contains(&"dynamodb".to_string()) || keywords.contains(&"DynamoDB".to_string()));
    assert!(keywords.contains(&"iam".to_string()) || keywords.contains(&"IAM".to_string()));
    assert!(keywords.contains(&"permissions".to_string()));
    
    // Check that common words are not extracted
    assert!(!keywords.contains(&"the".to_string()));
    assert!(!keywords.contains(&"and".to_string()));
    assert!(!keywords.contains(&"with".to_string()));
}

#[test]
fn test_technical_conversation_extraction() {
    // Create a technical conversation
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I'm trying to implement a concurrent hash map in Rust using atomic operations.".to_string())
        .add_assistant_message("That's an interesting challenge. You'll need to use std::sync::atomic and possibly RwLock for certain operations.".to_string(), None)
        .add_user_message("How can I ensure thread safety while maintaining good performance?".to_string());
    
    // Extract topics using the enhanced extractor
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Check that the technical topics are correctly identified
    assert_eq!(main_topic, "Rust");
    assert!(sub_topic == "Concurrency" || sub_topic == "Threading" || sub_topic == "HashMaps");
    assert!(action_type == "Programming" || action_type == "Implementation" || action_type == "Help");
}

#[test]
fn test_multi_topic_conversation() {
    // Create a conversation that covers multiple topics
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("How do I use Amazon Q CLI?".to_string())
        .add_assistant_message("Here's how to use Amazon Q CLI...".to_string(), None)
        .add_user_message("What about AWS Lambda functions?".to_string())
        .add_assistant_message("AWS Lambda is a serverless compute service...".to_string(), None)
        .add_user_message("Can I use Amazon Q CLI with Lambda?".to_string());
    
    // Extract topics using the enhanced extractor
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Check that both topics are identified (either as main and sub, or combined)
    assert!(main_topic == "AmazonQ" || main_topic == "AWS");
    assert!(sub_topic == "CLI" || sub_topic == "Lambda" || main_topic == "Lambda" && sub_topic == "CLI");
    assert!(action_type == "Help" || action_type == "Integration");
}

#[test]
fn test_conversation_with_code_blocks() {
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
    
    // Extract topics using the enhanced extractor
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Check that the code language is identified
    assert_eq!(main_topic, "Rust");
    assert!(sub_topic == "Programming" || sub_topic == "Code");
    assert_eq!(action_type, "Help");
    
    // Check that code blocks are properly handled
    let keywords = extract_keywords(&conv);
    assert!(keywords.contains(&"rust".to_string()) || keywords.contains(&"Rust".to_string()));
    assert!(keywords.contains(&"println".to_string()));
    assert!(keywords.contains(&"main".to_string()));
}

#[test]
fn test_sentiment_analysis() {
    // Create conversations with different sentiments
    let mut positive_conv = Conversation::new("positive".to_string());
    positive_conv.add_user_message("I love using Amazon Q CLI! It's so helpful and intuitive.".to_string());
    
    let mut negative_conv = Conversation::new("negative".to_string());
    negative_conv.add_user_message("I'm frustrated with Amazon Q CLI. It keeps giving me errors.".to_string());
    
    let mut neutral_conv = Conversation::new("neutral".to_string());
    neutral_conv.add_user_message("How do I use Amazon Q CLI to save conversations?".to_string());
    
    // Analyze sentiment
    let positive_sentiment = analyze_sentiment(&positive_conv);
    let negative_sentiment = analyze_sentiment(&negative_conv);
    let neutral_sentiment = analyze_sentiment(&neutral_conv);
    
    // Check that sentiments are correctly identified
    assert!(positive_sentiment > 0.5);
    assert!(negative_sentiment < 0.3);
    assert!(neutral_sentiment >= 0.3 && neutral_sentiment <= 0.7);
}

#[test]
fn test_language_detection() {
    // Create conversations in different languages
    let mut english_conv = Conversation::new("english".to_string());
    english_conv.add_user_message("How do I use Amazon Q CLI to save conversations?".to_string());
    
    let mut spanish_conv = Conversation::new("spanish".to_string());
    spanish_conv.add_user_message("¿Cómo puedo usar Amazon Q CLI para guardar conversaciones?".to_string());
    
    let mut french_conv = Conversation::new("french".to_string());
    french_conv.add_user_message("Comment puis-je utiliser Amazon Q CLI pour enregistrer des conversations?".to_string());
    
    // Detect languages
    let english_lang = detect_language(&english_conv);
    let spanish_lang = detect_language(&spanish_conv);
    let french_lang = detect_language(&french_conv);
    
    // Check that languages are correctly identified
    assert_eq!(english_lang, "en");
    assert_eq!(spanish_lang, "es");
    assert_eq!(french_lang, "fr");
}

#[test]
fn test_specialized_terminology() {
    // Create a conversation with specialized terminology
    let mut conv = Conversation::new("test-id".to_string());
    conv.add_user_message("I'm trying to understand how to use the Model Context Protocol with Amazon Q CLI.".to_string())
        .add_assistant_message("The Model Context Protocol (MCP) is a way to extend Amazon Q CLI with additional capabilities.".to_string(), None)
        .add_user_message("How do I create an MCP server?".to_string());
    
    // Extract topics using the enhanced extractor
    let (main_topic, sub_topic, action_type) = extract_topics(&conv);
    
    // Check that specialized terminology is correctly identified
    assert!(main_topic == "AmazonQ" || main_topic == "MCP");
    assert!(sub_topic == "CLI" || sub_topic == "ModelContextProtocol");
    assert!(action_type == "Help" || action_type == "Development");
    
    // Check that specialized terms are extracted
    let keywords = extract_keywords(&conv);
    assert!(keywords.contains(&"mcp".to_string()) || keywords.contains(&"MCP".to_string()));
    assert!(keywords.contains(&"model".to_string()) || keywords.contains(&"Model".to_string()));
    assert!(keywords.contains(&"context".to_string()) || keywords.contains(&"Context".to_string()));
    assert!(keywords.contains(&"protocol".to_string()) || keywords.contains(&"Protocol".to_string()));
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
        
        // Extract topics using the enhanced extractor
        let (main_topic, sub_topic, action_type) = extract_topics(&conv);
        
        // Check that topics are not empty (except for empty conversations)
        if conv_type != "empty" {
            assert!(!main_topic.is_empty());
            assert!(!sub_topic.is_empty());
            assert!(!action_type.is_empty());
        }
        
        // Check that keywords are extracted
        let keywords = extract_keywords(&conv);
        if conv_type != "empty" {
            assert!(!keywords.is_empty());
        }
    }
}
