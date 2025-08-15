// topic_extractor/mod.rs
// Topic extractor module for Amazon Q CLI automatic naming feature

pub mod enhanced;

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
