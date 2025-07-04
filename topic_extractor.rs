// topic_extractor.rs
// Topic extractor for Amazon Q CLI automatic naming feature

use std::collections::{HashMap, HashSet};
use crate::conversation::Conversation;

/// Extract topics from a conversation
///
/// Returns a tuple of (main_topic, sub_topic, action_type)
pub fn extract_topics(conversation: &Conversation) -> (String, String, String) {
    // Handle empty conversations
    if conversation.messages.is_empty() {
        return ("".to_string(), "".to_string(), "Conversation".to_string());
    }
    
    // Get the first few user messages
    let user_messages = conversation.first_user_messages(5);
    if user_messages.is_empty() {
        return ("".to_string(), "".to_string(), "Conversation".to_string());
    }
    
    // Combine the messages into a single text
    let text = user_messages
        .iter()
        .map(|m| m.content.clone())
        .collect::<Vec<String>>()
        .join(" ");
    
    // Extract keywords
    let keywords = extract_keywords(&text);
    
    // Determine the main topic
    let main_topic = determine_main_topic(&keywords);
    
    // Determine the sub-topic
    let sub_topic = determine_sub_topic(&keywords, &main_topic);
    
    // Determine the action type
    let action_type = determine_action_type(&text);
    
    (main_topic, sub_topic, action_type)
}

/// Extract keywords from text
fn extract_keywords(text: &str) -> Vec<String> {
    // Convert to lowercase
    let text = text.to_lowercase();
    
    // Split into words
    let words: Vec<&str> = text
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|s| !s.is_empty())
        .collect();
    
    // Remove stop words
    let stop_words = get_stop_words();
    let filtered_words: Vec<&str> = words
        .iter()
        .filter(|w| !stop_words.contains(*w))
        .cloned()
        .collect();
    
    // Count word frequencies
    let mut word_counts: HashMap<&str, usize> = HashMap::new();
    for word in filtered_words {
        *word_counts.entry(word).or_insert(0) += 1;
    }
    
    // Sort by frequency
    let mut word_counts: Vec<(&str, usize)> = word_counts.into_iter().collect();
    word_counts.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Convert to strings
    word_counts
        .into_iter()
        .map(|(word, _)| word.to_string())
        .collect()
}

/// Determine the main topic from keywords
fn determine_main_topic(keywords: &[String]) -> String {
    // Check for known products
    let products = vec![
        ("amazon", "Amazon"),
        ("aws", "AWS"),
        ("lambda", "Lambda"),
        ("s3", "S3"),
        ("ec2", "EC2"),
        ("dynamodb", "DynamoDB"),
        ("q", "AmazonQ"),
        ("cli", "CLI"),
        ("rust", "Rust"),
        ("python", "Python"),
        ("javascript", "JavaScript"),
        ("typescript", "TypeScript"),
        ("java", "Java"),
        ("c++", "CPP"),
        ("go", "Go"),
    ];
    
    // Look for product names in the keywords
    for keyword in keywords {
        for (pattern, product) in &products {
            if keyword.contains(pattern) {
                return product.to_string();
            }
        }
    }
    
    // If no product is found, use the first keyword if available
    if !keywords.is_empty() {
        // Capitalize the first letter
        let mut chars = keywords[0].chars();
        match chars.next() {
            None => "Unknown".to_string(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    } else {
        "Unknown".to_string()
    }
}

/// Determine the sub-topic from keywords
fn determine_sub_topic(keywords: &[String], main_topic: &str) -> String {
    // Skip the first keyword if it was used as the main topic
    let start_index = if !keywords.is_empty() && keywords[0].to_lowercase() == main_topic.to_lowercase() {
        1
    } else {
        0
    };
    
    // Use the next keyword as the sub-topic
    if keywords.len() > start_index {
        // Capitalize the first letter
        let mut chars = keywords[start_index].chars();
        match chars.next() {
            None => "General".to_string(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    } else {
        "General".to_string()
    }
}

/// Determine the action type from text
fn determine_action_type(text: &str) -> String {
    let text = text.to_lowercase();
    
    // Check for common action types
    let action_types = vec![
        (vec!["how", "what", "when", "where", "why", "who", "explain"], "Help"),
        (vec!["error", "issue", "problem", "bug", "fix", "solve"], "Troubleshooting"),
        (vec!["feature", "request", "enhancement", "improve", "add"], "FeatureRequest"),
        (vec!["code", "implement", "function", "class", "method"], "Code"),
        (vec!["learn", "tutorial", "guide", "example"], "Learning"),
    ];
    
    for (patterns, action) in action_types {
        for pattern in patterns {
            if text.contains(pattern) {
                return action.to_string();
            }
        }
    }
    
    // Default action type
    "Conversation".to_string()
}

/// Get a list of common stop words
fn get_stop_words() -> HashSet<&'static str> {
    vec![
        "a", "an", "the", "and", "or", "but", "if", "then", "else", "when",
        "at", "from", "by", "on", "off", "for", "in", "out", "over", "under",
        "again", "further", "then", "once", "here", "there", "when", "where", "why",
        "how", "all", "any", "both", "each", "few", "more", "most", "other",
        "some", "such", "no", "nor", "not", "only", "own", "same", "so",
        "than", "too", "very", "s", "t", "can", "will", "just", "don", "should", "now",
        "i", "me", "my", "myself", "we", "our", "ours", "ourselves", "you", "your",
        "yours", "yourself", "yourselves", "he", "him", "his", "himself", "she", "her",
        "hers", "herself", "it", "its", "itself", "they", "them", "their", "theirs",
        "themselves", "what", "which", "who", "whom", "this", "that", "these", "those",
        "am", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "having", "do", "does", "did", "doing", "would", "should", "could", "ought",
        "i'm", "you're", "he's", "she's", "it's", "we're", "they're", "i've", "you've",
        "we've", "they've", "i'd", "you'd", "he'd", "she'd", "we'd", "they'd", "i'll",
        "you'll", "he'll", "she'll", "we'll", "they'll", "isn't", "aren't", "wasn't",
        "weren't", "hasn't", "haven't", "hadn't", "doesn't", "don't", "didn't", "won't",
        "wouldn't", "shan't", "shouldn't", "can't", "cannot", "couldn't", "mustn't",
        "let's", "that's", "who's", "what's", "here's", "there's", "when's", "where's",
        "why's", "how's",
    ].into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::mocks::create_mock_conversation;
    
    #[test]
    fn test_extract_keywords() {
        let text = "How do I use Amazon Q CLI to save conversations?";
        let keywords = extract_keywords(text);
        
        assert!(keywords.contains(&"amazon".to_string()));
        assert!(keywords.contains(&"cli".to_string()));
        assert!(keywords.contains(&"save".to_string()));
        assert!(keywords.contains(&"conversations".to_string()));
        
        // Stop words should be removed
        assert!(!keywords.contains(&"how".to_string()));
        assert!(!keywords.contains(&"do".to_string()));
        assert!(!keywords.contains(&"i".to_string()));
        assert!(!keywords.contains(&"to".to_string()));
    }
    
    #[test]
    fn test_determine_main_topic() {
        // Test with product names
        assert_eq!(determine_main_topic(&vec!["amazon".to_string(), "cli".to_string()]), "Amazon");
        assert_eq!(determine_main_topic(&vec!["aws".to_string(), "lambda".to_string()]), "AWS");
        assert_eq!(determine_main_topic(&vec!["q".to_string(), "cli".to_string()]), "AmazonQ");
        assert_eq!(determine_main_topic(&vec!["rust".to_string(), "code".to_string()]), "Rust");
        
        // Test with unknown keywords
        assert_eq!(determine_main_topic(&vec!["hello".to_string(), "world".to_string()]), "Hello");
        
        // Test with empty keywords
        assert_eq!(determine_main_topic(&Vec::new()), "Unknown");
    }
    
    #[test]
    fn test_determine_sub_topic() {
        // Test with main topic as first keyword
        assert_eq!(
            determine_sub_topic(&vec!["amazon".to_string(), "cli".to_string()], "Amazon"),
            "Cli"
        );
        
        // Test with main topic not as first keyword
        assert_eq!(
            determine_sub_topic(&vec!["hello".to_string(), "amazon".to_string()], "Amazon"),
            "Hello"
        );
        
        // Test with empty keywords
        assert_eq!(determine_sub_topic(&Vec::new(), "Amazon"), "General");
    }
    
    #[test]
    fn test_determine_action_type() {
        // Test help questions
        assert_eq!(determine_action_type("How do I use Amazon Q CLI?"), "Help");
        assert_eq!(determine_action_type("What is Amazon Q?"), "Help");
        
        // Test troubleshooting
        assert_eq!(determine_action_type("I'm getting an error when using Amazon Q CLI"), "Troubleshooting");
        assert_eq!(determine_action_type("Fix this issue with my code"), "Troubleshooting");
        
        // Test feature requests
        assert_eq!(determine_action_type("Can you add a feature to automatically name saved conversations?"), "FeatureRequest");
        assert_eq!(determine_action_type("I request an enhancement to the save command"), "FeatureRequest");
        
        // Test code
        assert_eq!(determine_action_type("How do I implement a function in Rust?"), "Code");
        assert_eq!(determine_action_type("Write a class for parsing JSON"), "Code");
        
        // Test learning
        assert_eq!(determine_action_type("I want to learn about AWS Lambda"), "Learning");
        assert_eq!(determine_action_type("Show me a tutorial on Amazon Q CLI"), "Learning");
        
        // Test default
        assert_eq!(determine_action_type("Hello there"), "Conversation");
    }
    
    #[test]
    fn test_extract_topics_with_mock_conversations() {
        // Test with various mock conversations
        let test_cases = vec![
            ("empty", "", "", "Conversation"),
            ("simple", "Hello", "General", "Conversation"),
            ("amazon_q_cli", "AmazonQ", "CLI", "Help"),
            ("feature_request", "AmazonQ", "CLI", "FeatureRequest"),
            ("technical", "Rust", "JSON", "Code"),
            ("multi_topic", "AmazonQ", "CLI", "Help"),
        ];
        
        for (conv_type, expected_main, expected_sub, expected_action) in test_cases {
            let conv = create_mock_conversation(conv_type);
            let (main_topic, sub_topic, action_type) = extract_topics(&conv);
            
            assert_eq!(main_topic, expected_main);
            assert_eq!(sub_topic, expected_sub);
            assert_eq!(action_type, expected_action);
        }
    }
}
