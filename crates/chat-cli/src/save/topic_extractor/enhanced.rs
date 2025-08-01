// topic_extractor/enhanced.rs
// Enhanced topic extractor with basic NLP capabilities

use std::collections::{HashMap, HashSet};
use crate::conversation::Conversation;
use crate::topic_extractor::extract_topics as basic_extract_topics;

/// Extract topics from a conversation using enhanced NLP techniques
///
/// Returns a tuple of (main_topic, sub_topic, action_type)
pub fn extract_topics(conversation: &Conversation) -> (String, String, String) {
    // If the conversation is empty, use the basic extractor
    if conversation.messages.is_empty() {
        return basic_extract_topics(conversation);
    }
    
    // Extract keywords using enhanced techniques
    let keywords = extract_keywords(conversation);
    
    // If no keywords were extracted, use the basic extractor
    if keywords.is_empty() {
        return basic_extract_topics(conversation);
    }
    
    // Determine the main topic using enhanced techniques
    let main_topic = determine_main_topic(&keywords);
    
    // Determine the sub-topic using enhanced techniques
    let sub_topic = determine_sub_topic(&keywords, &main_topic);
    
    // Determine the action type using enhanced techniques
    let action_type = determine_action_type(conversation);
    
    (main_topic, sub_topic, action_type)
}

/// Extract keywords from a conversation using enhanced NLP techniques
pub fn extract_keywords(conversation: &Conversation) -> Vec<String> {
    // Get the user messages
    let user_messages = conversation.user_messages();
    if user_messages.is_empty() {
        return Vec::new();
    }
    
    // Combine the messages into a single text
    let text = user_messages
        .iter()
        .map(|m| m.content.clone())
        .collect::<Vec<String>>()
        .join(" ");
    
    // Tokenize the text
    let tokens = tokenize(&text);
    
    // Remove stop words
    let filtered_tokens = remove_stop_words(&tokens);
    
    // Extract n-grams
    let n_grams = extract_n_grams(&filtered_tokens, 2);
    
    // Combine tokens and n-grams
    let mut all_terms = filtered_tokens.clone();
    all_terms.extend(n_grams);
    
    // Count term frequencies
    let term_counts = count_term_frequencies(&all_terms);
    
    // Sort by frequency
    let mut term_counts: Vec<(String, usize)> = term_counts.into_iter().collect();
    term_counts.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Extract the top terms
    term_counts
        .into_iter()
        .map(|(term, _)| term)
        .collect()
}

/// Tokenize text into words
fn tokenize(text: &str) -> Vec<String> {
    // Convert to lowercase
    let text = text.to_lowercase();
    
    // Split into words
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Remove stop words from tokens
fn remove_stop_words(tokens: &[String]) -> Vec<String> {
    let stop_words = get_stop_words();
    tokens
        .iter()
        .filter(|t| !stop_words.contains(t.as_str()))
        .cloned()
        .collect()
}

/// Extract n-grams from tokens
fn extract_n_grams(tokens: &[String], n: usize) -> Vec<String> {
    if tokens.len() < n {
        return Vec::new();
    }
    
    let mut n_grams = Vec::new();
    for i in 0..=tokens.len() - n {
        let n_gram = tokens[i..i + n].join("_");
        n_grams.push(n_gram);
    }
    
    n_grams
}

/// Count term frequencies
fn count_term_frequencies(terms: &[String]) -> HashMap<String, usize> {
    let mut term_counts = HashMap::new();
    for term in terms {
        *term_counts.entry(term.clone()).or_insert(0) += 1;
    }
    term_counts
}

/// Determine the main topic from keywords
fn determine_main_topic(keywords: &[String]) -> String {
    // Check for known products and technologies
    let products = get_product_mapping();
    
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
    
    // Check for known sub-topics
    let sub_topics = get_sub_topic_mapping();
    
    // Look for sub-topic names in the keywords
    for i in start_index..keywords.len() {
        let keyword = &keywords[i];
        for (pattern, sub_topic) in &sub_topics {
            if keyword.contains(pattern) {
                return sub_topic.to_string();
            }
        }
    }
    
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

/// Determine the action type from a conversation
fn determine_action_type(conversation: &Conversation) -> String {
    // Get the user messages
    let user_messages = conversation.user_messages();
    if user_messages.is_empty() {
        return "Conversation".to_string();
    }
    
    // Combine the messages into a single text
    let text = user_messages
        .iter()
        .map(|m| m.content.clone())
        .collect::<Vec<String>>()
        .join(" ")
        .to_lowercase();
    
    // Check for common action types
    let action_types = get_action_type_mapping();
    
    // Look for action type patterns in the text
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

/// Analyze the sentiment of a conversation
///
/// Returns a value between 0 (negative) and 1 (positive)
pub fn analyze_sentiment(conversation: &Conversation) -> f32 {
    // Get the user messages
    let user_messages = conversation.user_messages();
    if user_messages.is_empty() {
        return 0.5; // Neutral
    }
    
    // Combine the messages into a single text
    let text = user_messages
        .iter()
        .map(|m| m.content.clone())
        .collect::<Vec<String>>()
        .join(" ")
        .to_lowercase();
    
    // Simple sentiment analysis using word lists
    let positive_words = get_positive_words();
    let negative_words = get_negative_words();
    
    // Count positive and negative words
    let tokens = tokenize(&text);
    let mut positive_count = 0;
    let mut negative_count = 0;
    
    for token in tokens {
        if positive_words.contains(token.as_str()) {
            positive_count += 1;
        } else if negative_words.contains(token.as_str()) {
            negative_count += 1;
        }
    }
    
    // Calculate sentiment score
    let total_count = positive_count + negative_count;
    if total_count == 0 {
        return 0.5; // Neutral
    }
    
    positive_count as f32 / total_count as f32
}

/// Detect the language of a conversation
///
/// Returns a language code (e.g., "en", "es", "fr")
pub fn detect_language(conversation: &Conversation) -> &'static str {
    // Get the user messages
    let user_messages = conversation.user_messages();
    if user_messages.is_empty() {
        return "en"; // Default to English
    }
    
    // Combine the messages into a single text
    let text = user_messages
        .iter()
        .map(|m| m.content.clone())
        .collect::<Vec<String>>()
        .join(" ");
    
    // Simple language detection using common words
    let language_words = get_language_words();
    
    // Count words for each language
    let tokens = tokenize(&text);
    let mut language_counts: HashMap<&str, usize> = HashMap::new();
    
    for token in tokens {
        for (lang, words) in &language_words {
            if words.contains(token.as_str()) {
                *language_counts.entry(lang).or_insert(0) += 1;
            }
        }
    }
    
    // Find the language with the most matches
    language_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(lang, _)| lang)
        .unwrap_or("en") // Default to English
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

/// Get a mapping of product patterns to product names
fn get_product_mapping() -> Vec<(&'static str, &'static str)> {
    vec![
        ("amazon", "Amazon"),
        ("aws", "AWS"),
        ("lambda", "Lambda"),
        ("s3", "S3"),
        ("ec2", "EC2"),
        ("dynamodb", "DynamoDB"),
        ("q", "AmazonQ"),
        ("cli", "CLI"),
        ("mcp", "MCP"),
        ("model context protocol", "ModelContextProtocol"),
        ("rust", "Rust"),
        ("python", "Python"),
        ("javascript", "JavaScript"),
        ("typescript", "TypeScript"),
        ("java", "Java"),
        ("c++", "CPP"),
        ("go", "Go"),
        ("react", "React"),
        ("angular", "Angular"),
        ("vue", "Vue"),
        ("node", "Node"),
        ("docker", "Docker"),
        ("kubernetes", "Kubernetes"),
        ("terraform", "Terraform"),
        ("cloudformation", "CloudFormation"),
    ]
}

/// Get a mapping of sub-topic patterns to sub-topic names
fn get_sub_topic_mapping() -> Vec<(&'static str, &'static str)> {
    vec![
        ("cli", "CLI"),
        ("api", "API"),
        ("sdk", "SDK"),
        ("ui", "UI"),
        ("ux", "UX"),
        ("frontend", "Frontend"),
        ("backend", "Backend"),
        ("database", "Database"),
        ("storage", "Storage"),
        ("compute", "Compute"),
        ("network", "Network"),
        ("security", "Security"),
        ("auth", "Authentication"),
        ("deploy", "Deployment"),
        ("ci", "CI"),
        ("cd", "CD"),
        ("test", "Testing"),
        ("monitor", "Monitoring"),
        ("log", "Logging"),
        ("debug", "Debugging"),
        ("performance", "Performance"),
        ("optimization", "Optimization"),
        ("concurrency", "Concurrency"),
        ("threading", "Threading"),
        ("async", "Async"),
        ("sync", "Sync"),
        ("error", "ErrorHandling"),
        ("exception", "ExceptionHandling"),
    ]
}

/// Get a mapping of action type patterns to action type names
fn get_action_type_mapping() -> Vec<(Vec<&'static str>, &'static str)> {
    vec![
        (vec!["how", "what", "when", "where", "why", "who", "explain"], "Help"),
        (vec!["error", "issue", "problem", "bug", "fix", "solve", "troubleshoot", "debug"], "Troubleshooting"),
        (vec!["feature", "request", "enhancement", "improve", "add", "implement"], "FeatureRequest"),
        (vec!["code", "implement", "function", "class", "method", "programming"], "Programming"),
        (vec!["learn", "tutorial", "guide", "example", "documentation"], "Learning"),
        (vec!["integrate", "integration", "connect", "connecting", "connection"], "Integration"),
        (vec!["deploy", "deployment", "release", "publish", "publishing"], "Deployment"),
        (vec!["test", "testing", "unit test", "integration test", "e2e test"], "Testing"),
        (vec!["configure", "configuration", "setup", "setting", "settings"], "Configuration"),
        (vec!["optimize", "optimization", "performance", "improve", "speed"], "Optimization"),
    ]
}

/// Get a list of positive sentiment words
fn get_positive_words() -> HashSet<&'static str> {
    vec![
        "good", "great", "excellent", "amazing", "wonderful", "fantastic",
        "awesome", "brilliant", "outstanding", "superb", "terrific", "fabulous",
        "love", "like", "enjoy", "happy", "pleased", "satisfied", "delighted",
        "helpful", "useful", "effective", "efficient", "reliable", "intuitive",
        "easy", "simple", "clear", "clean", "elegant", "beautiful", "nice",
    ].into_iter().collect()
}

/// Get a list of negative sentiment words
fn get_negative_words() -> HashSet<&'static str> {
    vec![
        "bad", "terrible", "awful", "horrible", "poor", "disappointing",
        "frustrating", "annoying", "irritating", "confusing", "complicated",
        "difficult", "hard", "complex", "messy", "ugly", "broken", "buggy",
        "hate", "dislike", "unhappy", "dissatisfied", "angry", "upset",
        "useless", "ineffective", "inefficient", "unreliable", "unintuitive",
    ].into_iter().collect()
}

/// Get common words for different languages
fn get_language_words() -> HashMap<&'static str, HashSet<&'static str>> {
    let mut language_words = HashMap::new();
    
    // English
    language_words.insert("en", vec![
        "the", "be", "to", "of", "and", "a", "in", "that", "have", "i",
        "it", "for", "not", "on", "with", "he", "as", "you", "do", "at",
    ].into_iter().collect());
    
    // Spanish
    language_words.insert("es", vec![
        "el", "la", "de", "que", "y", "a", "en", "un", "ser", "se",
        "no", "haber", "por", "con", "su", "para", "como", "estar", "tener", "le",
    ].into_iter().collect());
    
    // French
    language_words.insert("fr", vec![
        "le", "la", "de", "et", "à", "un", "être", "avoir", "que", "pour",
        "dans", "ce", "il", "qui", "ne", "sur", "se", "pas", "plus", "par",
    ].into_iter().collect());
    
    // German
    language_words.insert("de", vec![
        "der", "die", "und", "in", "den", "von", "zu", "das", "mit", "sich",
        "des", "auf", "für", "ist", "im", "dem", "nicht", "ein", "eine", "als",
    ].into_iter().collect());
    
    language_words
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tokenize() {
        let text = "Hello, world! This is a test.";
        let tokens = tokenize(text);
        
        assert_eq!(tokens, vec!["hello", "world", "this", "is", "a", "test"]);
    }
    
    #[test]
    fn test_remove_stop_words() {
        let tokens = vec![
            "hello".to_string(),
            "the".to_string(),
            "world".to_string(),
            "is".to_string(),
            "beautiful".to_string(),
        ];
        
        let filtered = remove_stop_words(&tokens);
        
        assert_eq!(filtered, vec!["hello".to_string(), "world".to_string(), "beautiful".to_string()]);
    }
    
    #[test]
    fn test_extract_n_grams() {
        let tokens = vec![
            "hello".to_string(),
            "world".to_string(),
            "this".to_string(),
            "is".to_string(),
            "a".to_string(),
            "test".to_string(),
        ];
        
        let bigrams = extract_n_grams(&tokens, 2);
        
        assert_eq!(bigrams, vec![
            "hello_world".to_string(),
            "world_this".to_string(),
            "this_is".to_string(),
            "is_a".to_string(),
            "a_test".to_string(),
        ]);
    }
    
    #[test]
    fn test_count_term_frequencies() {
        let terms = vec![
            "hello".to_string(),
            "world".to_string(),
            "hello".to_string(),
            "test".to_string(),
            "world".to_string(),
            "hello".to_string(),
        ];
        
        let counts = count_term_frequencies(&terms);
        
        assert_eq!(counts.get("hello"), Some(&3));
        assert_eq!(counts.get("world"), Some(&2));
        assert_eq!(counts.get("test"), Some(&1));
    }
    
    #[test]
    fn test_analyze_sentiment() {
        let mut positive_conv = Conversation::new("positive".to_string());
        positive_conv.add_user_message("I love this product! It's amazing and helpful.".to_string());
        
        let mut negative_conv = Conversation::new("negative".to_string());
        negative_conv.add_user_message("This is terrible and frustrating. I hate it.".to_string());
        
        let mut neutral_conv = Conversation::new("neutral".to_string());
        neutral_conv.add_user_message("This is a product that exists.".to_string());
        
        let positive_sentiment = analyze_sentiment(&positive_conv);
        let negative_sentiment = analyze_sentiment(&negative_conv);
        let neutral_sentiment = analyze_sentiment(&neutral_conv);
        
        assert!(positive_sentiment > 0.7);
        assert!(negative_sentiment < 0.3);
        assert!(neutral_sentiment >= 0.4 && neutral_sentiment <= 0.6);
    }
    
    #[test]
    fn test_detect_language() {
        let mut english_conv = Conversation::new("english".to_string());
        english_conv.add_user_message("The quick brown fox jumps over the lazy dog.".to_string());
        
        let mut spanish_conv = Conversation::new("spanish".to_string());
        spanish_conv.add_user_message("El zorro marrón rápido salta sobre el perro perezoso.".to_string());
        
        let mut french_conv = Conversation::new("french".to_string());
        french_conv.add_user_message("Le renard brun rapide saute par-dessus le chien paresseux.".to_string());
        
        assert_eq!(detect_language(&english_conv), "en");
        assert_eq!(detect_language(&spanish_conv), "es");
        assert_eq!(detect_language(&french_conv), "fr");
    }
}
