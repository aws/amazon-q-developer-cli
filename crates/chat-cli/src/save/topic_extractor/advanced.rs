// topic_extractor/advanced.rs
// Advanced NLP capabilities for topic extraction

use std::collections::{HashMap, HashSet, BTreeMap};
use crate::conversation::Conversation;
use crate::topic_extractor::enhanced::{extract_keywords, analyze_sentiment, detect_language};

/// Extract topics from a conversation using advanced NLP techniques
///
/// Returns a tuple of (main_topic, sub_topic, action_type)
pub fn extract_topics(conversation: &Conversation) -> (String, String, String) {
    // If the conversation is empty, return default values
    if conversation.messages.is_empty() {
        return ("".to_string(), "".to_string(), "Conversation".to_string());
    }
    
    // Detect language to ensure appropriate processing
    let language = detect_language(conversation);
    
    // Extract keywords using enhanced techniques with language context
    let keywords = extract_keywords_with_language(conversation, &language);
    
    // If no keywords were extracted, return default values
    if keywords.is_empty() {
        return ("".to_string(), "".to_string(), "Conversation".to_string());
    }
    
    // Perform topic modeling with language context
    let topics = perform_topic_modeling(conversation, &language);
    
    // Analyze conversation structure to identify context
    let context = analyze_conversation_structure(conversation);
    
    // Determine the main topic with context awareness
    let main_topic = if !topics.is_empty() {
        topics[0].0.clone()
    } else {
        determine_main_topic_with_context(&keywords, &context, &language)
    };
    
    // Determine the sub-topic with context awareness
    let sub_topic = if topics.len() > 1 {
        topics[1].0.clone()
    } else {
        determine_sub_topic_with_context(&keywords, &main_topic, &context, &language)
    };
    
    // Determine the action type with context awareness
    let action_type = determine_action_type_with_context(conversation, &context, &language);
    
    // Apply post-processing to ensure consistency and quality
    let (refined_main_topic, refined_sub_topic, refined_action_type) = 
        refine_topics(main_topic, sub_topic, action_type, conversation);
    
    (refined_main_topic, refined_sub_topic, refined_action_type)
}

/// Extract keywords with language context awareness
fn extract_keywords_with_language(conversation: &Conversation, language: &str) -> Vec<String> {
    let mut keywords = extract_keywords(conversation);
    
    // Apply language-specific processing
    if language != "en" {
        // For non-English content, we need to apply specialized processing
        // This is a simplified implementation - in a real system, we would use
        // language-specific NLP libraries or models
        
        // For now, we'll just add the detected language as a keyword
        keywords.push(format!("lang_{}", language));
    }
    
    // Apply additional filtering for technical terms based on context
    let technical_terms = extract_technical_terms(conversation);
    keywords.extend(technical_terms);
    
    keywords
}

/// Extract technical terms from conversation
fn extract_technical_terms(conversation: &Conversation) -> Vec<String> {
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
    
    // Look for patterns that indicate technical terms
    // This is a simplified implementation - in a real system, we would use
    // more sophisticated pattern recognition or named entity recognition
    
    let mut technical_terms = Vec::new();
    
    // Extract terms that look like:
    // - Commands (starting with '/' or '--')
    // - API endpoints (containing '/')
    // - Function calls (ending with '()')
    // - File paths (containing '.' and '/')
    // - Error codes (all caps with numbers)
    
    // Simple regex-like patterns (simplified implementation)
    for word in text.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-');
        
        if word.starts_with('/') && word.len() > 1 {
            // Likely a command or path
            technical_terms.push(word.to_string());
        } else if word.ends_with("()") || word.contains("::") {
            // Likely a function call or namespace reference
            technical_terms.push(word.to_string());
        } else if word.contains('.') && (word.contains('/') || word.ends_with(".rs") || word.ends_with(".py") || word.ends_with(".js")) {
            // Likely a file path
            technical_terms.push(word.to_string());
        } else if word.chars().all(|c| c.is_uppercase() || c.is_numeric() || c == '_') && word.len() > 2 {
            // Likely an error code or constant
            technical_terms.push(word.to_string());
        }
    }
    
    technical_terms
}

/// Analyze conversation structure to identify context
fn analyze_conversation_structure(conversation: &Conversation) -> HashMap<String, f32> {
    let mut context_scores = HashMap::new();
    
    // Get all messages
    let messages = &conversation.messages;
    if messages.is_empty() {
        return context_scores;
    }
    
    // Calculate message statistics
    let total_messages = messages.len() as f32;
    let user_messages = conversation.user_messages();
    let user_message_count = user_messages.len() as f32;
    let assistant_message_count = total_messages - user_message_count;
    
    // Calculate average message length
    let avg_user_message_length = user_messages.iter()
        .map(|m| m.content.len())
        .sum::<usize>() as f32 / user_message_count.max(1.0);
    
    // Detect conversation patterns
    
    // Short Q&A pattern (short user messages, alternating)
    if avg_user_message_length < 100.0 && user_message_count > 1.0 {
        context_scores.insert("qa_pattern".to_string(), 0.8);
    }
    
    // Technical discussion (code blocks, longer messages)
    let has_code_blocks = user_messages.iter()
        .any(|m| m.content.contains("```") || m.content.contains("`"));
    
    if has_code_blocks {
        context_scores.insert("technical_discussion".to_string(), 0.9);
    }
    
    // Multi-turn conversation (many messages back and forth)
    if total_messages > 6.0 {
        context_scores.insert("multi_turn".to_string(), 0.7);
    }
    
    // Initial query pattern (first message is much longer than others)
    if user_messages.len() > 1 && 
       user_messages[0].content.len() > 2 * user_messages[1..].iter().map(|m| m.content.len()).sum::<usize>() / user_messages[1..].len().max(1) {
        context_scores.insert("initial_query".to_string(), 0.8);
    }
    
    context_scores
}

/// Perform topic modeling on a conversation with language context
///
/// Returns a vector of (topic, score) pairs, sorted by score
fn perform_topic_modeling(conversation: &Conversation, language: &str) -> Vec<(String, f32)> {
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
    
    // Extract keywords and n-grams with language context
    let keywords = extract_keywords_and_ngrams_with_language(&text, language);
    
    // Calculate TF-IDF scores with language-specific corpus
    let tf_idf_scores = calculate_tf_idf(&keywords, &get_corpus_frequencies_for_language(language));
    
    // Apply latent semantic analysis (simplified implementation)
    let lsa_scores = apply_latent_semantic_analysis(&tf_idf_scores);
    
    // Map keywords to topics with language context
    let topic_scores = map_keywords_to_topics_with_language(&lsa_scores, language);
    
    // Sort topics by score
    let mut topic_scores: Vec<(String, f32)> = topic_scores.into_iter().collect();
    topic_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Return the top topics
    topic_scores.into_iter().take(5).collect()
}

/// Extract keywords and n-grams with language context
fn extract_keywords_and_ngrams_with_language(text: &str, language: &str) -> Vec<String> {
    // Base extraction
    let mut terms = extract_keywords_and_ngrams(text);
    
    // Apply language-specific processing
    if language != "en" {
        // For non-English content, we would apply language-specific tokenization
        // This is a simplified implementation
        terms.push(format!("lang_{}", language));
    }
    
    // Apply additional processing for technical content
    if text.contains("```") || text.contains("`") {
        // Extract potential programming language identifiers
        for lang in &["rust", "python", "javascript", "typescript", "java", "c++", "go"] {
            if text.to_lowercase().contains(lang) {
                terms.push(format!("code_{}", lang));
            }
        }
    }
    
    terms
}

/// Apply latent semantic analysis (simplified implementation)
fn apply_latent_semantic_analysis(tf_idf_scores: &HashMap<String, f32>) -> HashMap<String, f32> {
    // In a real implementation, this would use singular value decomposition (SVD)
    // to identify latent topics in the term-document matrix
    
    // For this simplified implementation, we'll just apply some weighting
    // to terms that often appear together
    
    let mut lsa_scores = tf_idf_scores.clone();
    
    // Identify potential term clusters (terms that might be related)
    let term_clusters = identify_term_clusters(tf_idf_scores);
    
    // Boost scores for terms in the same cluster
    for (term, score) in lsa_scores.iter_mut() {
        for (cluster_name, cluster_terms) in &term_clusters {
            if cluster_terms.contains(term.as_str()) {
                // Boost the score for terms in recognized clusters
                *score *= 1.2;
                break;
            }
        }
    }
    
    lsa_scores
}

/// Identify potential term clusters (simplified implementation)
fn identify_term_clusters(tf_idf_scores: &HashMap<String, f32>) -> HashMap<String, HashSet<&'static str>> {
    let mut clusters = HashMap::new();
    
    // AWS services cluster
    clusters.insert("aws_services".to_string(), vec![
        "lambda", "s3", "ec2", "dynamodb", "rds", "sqs", "sns", 
        "cloudformation", "cloudwatch", "iam", "vpc"
    ].into_iter().collect());
    
    // Programming languages cluster
    clusters.insert("programming_languages".to_string(), vec![
        "rust", "python", "javascript", "typescript", "java", "c++", "go"
    ].into_iter().collect());
    
    // Web development cluster
    clusters.insert("web_development".to_string(), vec![
        "html", "css", "javascript", "react", "angular", "vue", "dom", "api"
    ].into_iter().collect());
    
    // Data science cluster
    clusters.insert("data_science".to_string(), vec![
        "python", "numpy", "pandas", "matplotlib", "tensorflow", "pytorch", "data"
    ].into_iter().collect());
    
    // DevOps cluster
    clusters.insert("devops".to_string(), vec![
        "docker", "kubernetes", "terraform", "ci", "cd", "pipeline", "deploy"
    ].into_iter().collect());
    
    clusters
}

/// Map keywords to topics with language context
fn map_keywords_to_topics_with_language(lsa_scores: &HashMap<String, f32>, language: &str) -> HashMap<String, f32> {
    let mut topic_scores = map_keywords_to_topics(lsa_scores);
    
    // Apply language-specific topic mapping
    if language != "en" {
        // For non-English content, we might adjust topic weights
        // This is a simplified implementation
        if topic_scores.contains_key("AmazonQ") {
            topic_scores.insert("AmazonQ".to_string(), topic_scores["AmazonQ"] * 1.1);
        }
    }
    
    // Apply domain-specific boosting
    boost_domain_specific_topics(&mut topic_scores, lsa_scores);
    
    topic_scores
}

/// Boost domain-specific topics based on keyword patterns
fn boost_domain_specific_topics(topic_scores: &mut HashMap<String, f32>, keyword_scores: &HashMap<String, f32>) {
    // Check for AWS service concentration
    let aws_service_count = keyword_scores.keys()
        .filter(|k| ["lambda", "s3", "ec2", "dynamodb", "rds"].iter().any(|s| k.contains(s)))
        .count();
    
    if aws_service_count >= 2 {
        // Boost AWS topic if multiple AWS services are mentioned
        *topic_scores.entry("AWS".to_string()).or_insert(0.0) *= 1.5;
    }
    
    // Check for programming language concentration
    let prog_lang_count = keyword_scores.keys()
        .filter(|k| ["rust", "python", "javascript", "java"].iter().any(|s| k.contains(s)))
        .count();
    
    if prog_lang_count >= 1 {
        // Create or boost Programming topic if languages are mentioned
        let prog_topic = if keyword_scores.keys().any(|k| k.contains("rust")) {
            "Rust"
        } else if keyword_scores.keys().any(|k| k.contains("python")) {
            "Python"
        } else if keyword_scores.keys().any(|k| k.contains("javascript")) {
            "JavaScript"
        } else if keyword_scores.keys().any(|k| k.contains("java")) {
            "Java"
        } else {
            "Programming"
        };
        
        *topic_scores.entry(prog_topic.to_string()).or_insert(0.0) += 0.5;
    }
}

/// Get corpus frequencies for IDF calculation with language context
fn get_corpus_frequencies_for_language(language: &str) -> HashMap<String, f32> {
    let mut frequencies = get_corpus_frequencies();
    
    // Apply language-specific adjustments
    if language != "en" {
        // For non-English content, we might adjust the corpus frequencies
        // This is a simplified implementation
        frequencies.insert(format!("lang_{}", language), 1.5);
    }
    
    frequencies
}

/// Extract keywords and n-grams from text
fn extract_keywords_and_ngrams(text: &str) -> Vec<String> {
    // Tokenize the text
    let tokens = tokenize(text);
    
    // Remove stop words
    let filtered_tokens = remove_stop_words(&tokens);
    
    // Extract n-grams
    let unigrams = filtered_tokens.clone();
    let bigrams = extract_n_grams(&filtered_tokens, 2);
    let trigrams = extract_n_grams(&filtered_tokens, 3);
    
    // Combine all n-grams
    let mut all_terms = Vec::new();
    all_terms.extend(unigrams);
    all_terms.extend(bigrams);
    all_terms.extend(trigrams);
    
    all_terms
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

/// Calculate TF-IDF scores for terms
fn calculate_tf_idf(terms: &[String], corpus_frequencies: &HashMap<String, f32>) -> HashMap<String, f32> {
    // Count term frequencies
    let mut term_counts = HashMap::new();
    for term in terms {
        *term_counts.entry(term.clone()).or_insert(0) += 1;
    }
    
    // Calculate TF-IDF scores
    let mut tf_idf_scores = HashMap::new();
    let total_terms = terms.len() as f32;
    
    for (term, count) in term_counts {
        let tf = count as f32 / total_terms;
        let idf = corpus_frequencies.get(&term).cloned().unwrap_or(1.0);
        let tf_idf = tf * idf;
        tf_idf_scores.insert(term, tf_idf);
    }
    
    tf_idf_scores
}

/// Map keywords to topics
fn map_keywords_to_topics(tf_idf_scores: &HashMap<String, f32>) -> HashMap<String, f32> {
    let mut topic_scores = HashMap::new();
    let topic_keywords = get_topic_keywords();
    
    for (term, score) in tf_idf_scores {
        for (topic, keywords) in &topic_keywords {
            if keywords.contains(term.as_str()) || term.contains(topic) {
                *topic_scores.entry(topic.clone()).or_insert(0.0) += score;
            }
        }
    }
    
    topic_scores
}

/// Determine the main topic from keywords with context awareness
fn determine_main_topic_with_context(keywords: &[String], context: &HashMap<String, f32>, language: &str) -> String {
    // Check for known products and technologies
    let products = get_product_mapping();
    
    // Check if we have a technical discussion context
    let is_technical = context.get("technical_discussion").unwrap_or(&0.0) > &0.5;
    
    // Check if we have a QA pattern context
    let is_qa = context.get("qa_pattern").unwrap_or(&0.0) > &0.5;
    
    // Look for product names in the keywords with context awareness
    for keyword in keywords {
        for (pattern, product) in &products {
            if keyword.contains(pattern) {
                // For technical discussions about a product, use the product name
                if is_technical && (
                    *product == "Rust" || 
                    *product == "Python" || 
                    *product == "JavaScript" || 
                    *product == "Java" || 
                    *product == "CPP" || 
                    *product == "Go"
                ) {
                    return product.to_string();
                }
                
                // For AWS services in technical discussions, use AWS
                if is_technical && (
                    *product == "Lambda" || 
                    *product == "S3" || 
                    *product == "EC2" || 
                    *product == "DynamoDB"
                ) {
                    return "AWS".to_string();
                }
                
                // For Amazon Q discussions, always use AmazonQ
                if *product == "AmazonQ" || *product == "Amazon" || *product == "CLI" {
                    return "AmazonQ".to_string();
                }
                
                // For other products, use the product name
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
        // Default based on context
        if is_technical {
            "Technical".to_string()
        } else if is_qa {
            "Question".to_string()
        } else {
            "Unknown".to_string()
        }
    }
}

/// Determine the sub-topic from keywords with context awareness
fn determine_sub_topic_with_context(keywords: &[String], main_topic: &str, context: &HashMap<String, f32>, language: &str) -> String {
    // Skip the first keyword if it was used as the main topic
    let start_index = if !keywords.is_empty() && keywords[0].to_lowercase() == main_topic.to_lowercase() {
        1
    } else {
        0
    };
    
    // Check for known sub-topics
    let sub_topics = get_sub_topic_mapping();
    
    // Check if we have a technical discussion context
    let is_technical = context.get("technical_discussion").unwrap_or(&0.0) > &0.5;
    
    // Check if we have a multi-turn conversation context
    let is_multi_turn = context.get("multi_turn").unwrap_or(&0.0) > &0.5;
    
    // For AmazonQ main topic, prioritize CLI as sub-topic
    if main_topic == "AmazonQ" {
        for i in start_index..keywords.len() {
            let keyword = &keywords[i];
            if keyword.to_lowercase().contains("cli") {
                return "CLI".to_string();
            }
        }
    }
    
    // For AWS main topic, look for specific services
    if main_topic == "AWS" {
        for i in start_index..keywords.len() {
            let keyword = &keywords[i];
            for service in &["lambda", "s3", "ec2", "dynamodb", "rds"] {
                if keyword.to_lowercase().contains(service) {
                    // Capitalize the first letter
                    let mut chars = service.chars();
                    match chars.next() {
                        None => continue,
                        Some(first) => return first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                }
            }
        }
    }
    
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
        // Default based on context and main topic
        if is_technical && main_topic == "AmazonQ" {
            "Usage".to_string()
        } else if is_technical {
            "Implementation".to_string()
        } else if is_multi_turn {
            "Discussion".to_string()
        } else {
            "General".to_string()
        }
    }
}

/// Determine the action type from a conversation with context awareness
fn determine_action_type_with_context(conversation: &Conversation, context: &HashMap<String, f32>, language: &str) -> String {
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
    
    // Check for code blocks
    let has_code_blocks = text.contains("```") || text.contains("`");
    
    // Check for question marks
    let has_questions = text.contains("?");
    
    // Check for error-related terms
    let has_errors = text.contains("error") || text.contains("issue") || text.contains("problem") || text.contains("bug") || text.contains("fix");
    
    // Check for feature-related terms
    let has_features = text.contains("feature") || text.contains("request") || text.contains("enhancement") || text.contains("improve");
    
    // Check for learning-related terms
    let has_learning = text.contains("learn") || text.contains("tutorial") || text.contains("guide") || text.contains("example") || text.contains("how to");
    
    // Check for sentiment
    let sentiment = analyze_sentiment(conversation);
    
    // Check if we have a technical discussion context
    let is_technical = context.get("technical_discussion").unwrap_or(&0.0) > &0.5;
    
    // Check if we have a QA pattern context
    let is_qa = context.get("qa_pattern").unwrap_or(&0.0) > &0.5;
    
    // Check if we have an initial query context
    let is_initial_query = context.get("initial_query").unwrap_or(&0.0) > &0.5;
    
    // Determine action type based on multiple factors with context awareness
    if has_code_blocks && has_errors {
        "Troubleshooting".to_string()
    } else if has_code_blocks && is_technical {
        "Programming".to_string()
    } else if has_features {
        "FeatureRequest".to_string()
    } else if has_learning || (is_qa && has_questions) {
        "Learning".to_string()
    } else if has_questions && is_initial_query {
        "Help".to_string()
    } else if has_errors || sentiment < 0.3 {
        "Troubleshooting".to_string()
    } else if sentiment > 0.7 {
        "Feedback".to_string()
    } else if is_technical {
        "Technical".to_string()
    } else {
        "Conversation".to_string()
    }
}

/// Refine topics for consistency and quality
fn refine_topics(main_topic: String, sub_topic: String, action_type: String, conversation: &Conversation) -> (String, String, String) {
    // Ensure main topic is not empty
    let main_topic = if main_topic.is_empty() {
        "Unknown".to_string()
    } else {
        main_topic
    };
    
    // Ensure sub-topic is not empty and not the same as main topic
    let sub_topic = if sub_topic.is_empty() || sub_topic == main_topic {
        if action_type != "Conversation" && action_type != main_topic {
            action_type.clone()
        } else {
            "General".to_string()
        }
    } else {
        sub_topic
    };
    
    // Ensure action type is not empty
    let action_type = if action_type.is_empty() {
        "Conversation".to_string()
    } else {
        action_type
    };
    
    // Special case for AmazonQ
    if main_topic.contains("Amazon") && main_topic.contains("Q") {
        return ("AmazonQ".to_string(), sub_topic, action_type);
    }
    
    // Special case for AWS services
    if ["Lambda", "S3", "EC2", "DynamoDB", "RDS"].contains(&main_topic.as_str()) {
        return ("AWS".to_string(), main_topic, action_type);
    }
    
    (main_topic, sub_topic, action_type)
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

/// Get corpus frequencies for IDF calculation
fn get_corpus_frequencies() -> HashMap<String, f32> {
    // This would normally be calculated from a large corpus
    // For simplicity, we'll use a small set of common terms
    let mut frequencies = HashMap::new();
    
    // Common terms have lower IDF (less discriminative)
    frequencies.insert("amazon".to_string(), 0.5);
    frequencies.insert("aws".to_string(), 0.6);
    frequencies.insert("cli".to_string(), 0.7);
    frequencies.insert("help".to_string(), 0.4);
    frequencies.insert("error".to_string(), 0.8);
    frequencies.insert("feature".to_string(), 0.9);
    frequencies.insert("code".to_string(), 0.7);
    frequencies.insert("learn".to_string(), 0.8);
    
    // Less common terms have higher IDF (more discriminative)
    frequencies.insert("lambda".to_string(), 1.2);
    frequencies.insert("dynamodb".to_string(), 1.3);
    frequencies.insert("s3".to_string(), 1.1);
    frequencies.insert("ec2".to_string(), 1.2);
    frequencies.insert("rust".to_string(), 1.4);
    frequencies.insert("python".to_string(), 1.1);
    frequencies.insert("javascript".to_string(), 1.2);
    frequencies.insert("typescript".to_string(), 1.3);
    frequencies.insert("java".to_string(), 1.1);
    frequencies.insert("c++".to_string(), 1.4);
    frequencies.insert("go".to_string(), 1.3);
    
    frequencies
}

/// Get topic keywords mapping
fn get_topic_keywords() -> HashMap<String, HashSet<&'static str>> {
    let mut topic_keywords = HashMap::new();
    
    // Amazon Q
    topic_keywords.insert("AmazonQ".to_string(), vec![
        "amazon", "q", "cli", "amazon_q", "amazonq", "q_cli", "amazon_q_cli",
        "conversation", "chat", "assistant", "ai", "model", "context", "protocol",
        "mcp", "model_context_protocol", "save", "conversation", "automatic", "naming",
    ].into_iter().collect());
    
    // AWS
    topic_keywords.insert("AWS".to_string(), vec![
        "aws", "amazon_web_services", "cloud", "lambda", "s3", "ec2", "dynamodb",
        "rds", "sqs", "sns", "cloudformation", "cloudwatch", "iam", "vpc",
        "serverless", "fargate", "ecs", "eks", "api_gateway", "bedrock", "sagemaker",
    ].into_iter().collect());
    
    // Programming Languages
    topic_keywords.insert("Rust".to_string(), vec![
        "rust", "cargo", "crate", "rustc", "rustup", "ownership", "borrowing",
        "lifetime", "trait", "struct", "enum", "match", "pattern", "macro",
        "unsafe", "async", "await", "tokio", "actix", "rocket", "wasm",
    ].into_iter().collect());
    
    topic_keywords.insert("Python".to_string(), vec![
        "python", "pip", "virtualenv", "conda", "numpy", "pandas", "matplotlib",
        "scipy", "tensorflow", "pytorch", "django", "flask", "fastapi",
        "asyncio", "generator", "decorator", "list_comprehension", "pep8",
    ].into_iter().collect());
    
    topic_keywords.insert("JavaScript".to_string(), vec![
        "javascript", "js", "node", "npm", "yarn", "react", "angular", "vue",
        "express", "webpack", "babel", "eslint", "jest", "mocha", "typescript",
        "promise", "async", "await", "closure", "prototype", "dom", "event",
    ].into_iter().collect());
    
    topic_keywords.insert("TypeScript".to_string(), vec![
        "typescript", "ts", "tsc", "interface", "type", "enum", "namespace",
        "decorator", "generics", "tsconfig", "tslint", "angular", "react",
        "vue", "node", "deno", "compiler_options", "strict_mode",
    ].into_iter().collect());
    
    topic_keywords.insert("Java".to_string(), vec![
        "java", "maven", "gradle", "spring", "hibernate", "jdbc", "jpa",
        "servlet", "tomcat", "jetty", "jvm", "jar", "war", "class", "interface",
        "abstract", "extends", "implements", "annotation", "generics",
    ].into_iter().collect());
    
    // Action Types
    topic_keywords.insert("Help".to_string(), vec![
        "help", "how", "what", "when", "where", "why", "who", "explain",
        "guide", "tutorial", "documentation", "example", "question", "clarify",
        "understand", "meaning", "definition", "describe", "detail", "elaborate",
    ].into_iter().collect());
    
    topic_keywords.insert("Troubleshooting".to_string(), vec![
        "error", "issue", "problem", "bug", "fix", "solve", "troubleshoot",
        "debug", "exception", "crash", "failure", "broken", "not_working",
        "incorrect", "unexpected", "wrong", "failed", "failing", "corrupt",
    ].into_iter().collect());
    
    topic_keywords.insert("FeatureRequest".to_string(), vec![
        "feature", "request", "enhancement", "improve", "add", "implement",
        "suggestion", "idea", "proposal", "new", "functionality", "capability",
        "option", "setting", "preference", "configuration", "customize",
    ].into_iter().collect());
    
    topic_keywords.insert("Programming".to_string(), vec![
        "code", "implement", "function", "class", "method", "programming",
        "development", "software", "application", "library", "framework",
        "algorithm", "data_structure", "pattern", "architecture", "design",
    ].into_iter().collect());
    
    topic_keywords.insert("Learning".to_string(), vec![
        "learn", "tutorial", "guide", "example", "documentation", "course",
        "training", "workshop", "lesson", "study", "understand", "concept",
        "principle", "fundamentals", "basics", "introduction", "beginner",
    ].into_iter().collect());
    
    // Add more domain-specific topics
    topic_keywords.insert("Security".to_string(), vec![
        "security", "authentication", "authorization", "encryption", "decryption",
        "hash", "password", "token", "jwt", "oauth", "permission", "role",
        "access", "firewall", "vulnerability", "exploit", "attack", "protect",
    ].into_iter().collect());
    
    topic_keywords.insert("Database".to_string(), vec![
        "database", "sql", "nosql", "query", "table", "schema", "index",
        "transaction", "acid", "join", "select", "insert", "update", "delete",
        "migration", "orm", "entity", "relationship", "primary_key", "foreign_key",
    ].into_iter().collect());
    
    topic_keywords.insert("DevOps".to_string(), vec![
        "devops", "ci", "cd", "pipeline", "deploy", "deployment", "container",
        "docker", "kubernetes", "k8s", "helm", "terraform", "infrastructure",
        "monitoring", "logging", "alerting", "scaling", "load_balancing",
    ].into_iter().collect());
    
    topic_keywords.insert("Testing".to_string(), vec![
        "test", "testing", "unit", "integration", "e2e", "end_to_end", "mock",
        "stub", "spy", "assertion", "expect", "should", "coverage", "tdd",
        "bdd", "scenario", "case", "suite", "runner", "framework",
    ].into_iter().collect());
    
    topic_keywords
}

/// Get a mapping of product patterns to product names
fn get_product_mapping() -> Vec<(&'static str, &'static str)> {
    vec![
        // Amazon products
        ("amazon q", "AmazonQ"),
        ("amazon", "Amazon"),
        ("aws", "AWS"),
        ("lambda", "Lambda"),
        ("s3", "S3"),
        ("ec2", "EC2"),
        ("dynamodb", "DynamoDB"),
        ("rds", "RDS"),
        ("sqs", "SQS"),
        ("sns", "SNS"),
        ("cloudformation", "CloudFormation"),
        ("cloudwatch", "CloudWatch"),
        ("iam", "IAM"),
        ("vpc", "VPC"),
        ("bedrock", "Bedrock"),
        ("sagemaker", "SageMaker"),
        ("q cli", "AmazonQ"),
        ("cli", "CLI"),
        ("mcp", "MCP"),
        ("model context protocol", "ModelContextProtocol"),
        
        // Programming languages
        ("rust", "Rust"),
        ("python", "Python"),
        ("javascript", "JavaScript"),
        ("typescript", "TypeScript"),
        ("java", "Java"),
        ("c++", "CPP"),
        ("go", "Go"),
        
        // Frameworks and libraries
        ("react", "React"),
        ("angular", "Angular"),
        ("vue", "Vue"),
        ("node", "Node"),
        ("express", "Express"),
        ("django", "Django"),
        ("flask", "Flask"),
        ("spring", "Spring"),
        ("hibernate", "Hibernate"),
        
        // DevOps tools
        ("docker", "Docker"),
        ("kubernetes", "Kubernetes"),
        ("terraform", "Terraform"),
        ("jenkins", "Jenkins"),
        ("gitlab", "GitLab"),
        ("github", "GitHub"),
        
        // Databases
        ("mysql", "MySQL"),
        ("postgresql", "PostgreSQL"),
        ("mongodb", "MongoDB"),
        ("redis", "Redis"),
        ("elasticsearch", "Elasticsearch"),
        
        // Cloud providers
        ("azure", "Azure"),
        ("gcp", "GCP"),
        ("google cloud", "GoogleCloud"),
    ]
}

/// Get a mapping of sub-topic patterns to sub-topic names
fn get_sub_topic_mapping() -> Vec<(&'static str, &'static str)> {
    vec![
        // Interface types
        ("cli", "CLI"),
        ("api", "API"),
        ("sdk", "SDK"),
        ("ui", "UI"),
        ("ux", "UX"),
        ("gui", "GUI"),
        
        // Architecture components
        ("frontend", "Frontend"),
        ("backend", "Backend"),
        ("database", "Database"),
        ("storage", "Storage"),
        ("compute", "Compute"),
        ("network", "Network"),
        ("security", "Security"),
        ("auth", "Authentication"),
        
        // Development processes
        ("deploy", "Deployment"),
        ("ci", "CI"),
        ("cd", "CD"),
        ("test", "Testing"),
        ("monitor", "Monitoring"),
        ("log", "Logging"),
        ("debug", "Debugging"),
        
        // Performance aspects
        ("performance", "Performance"),
        ("optimization", "Optimization"),
        ("concurrency", "Concurrency"),
        ("threading", "Threading"),
        ("async", "Async"),
        ("sync", "Sync"),
        
        // Error handling
        ("error", "ErrorHandling"),
        ("exception", "ExceptionHandling"),
        ("validation", "Validation"),
        
        // Data processing
        ("parsing", "Parsing"),
        ("serialization", "Serialization"),
        ("encoding", "Encoding"),
        ("decoding", "Decoding"),
        ("compression", "Compression"),
        
        // Security aspects
        ("encryption", "Encryption"),
        ("authentication", "Authentication"),
        ("authorization", "Authorization"),
        ("permission", "Permissions"),
        
        // Cloud concepts
        ("serverless", "Serverless"),
        ("container", "Containers"),
        ("microservice", "Microservices"),
        ("scaling", "Scaling"),
        
        // Amazon Q specific
        ("save", "Saving"),
        ("conversation", "Conversations"),
        ("naming", "Naming"),
        ("automatic", "Automation"),
        ("filename", "Filenames"),
        ("generation", "Generation"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::mocks::create_mock_conversation;
    
    #[test]
    fn test_extract_keywords_and_ngrams() {
        let text = "Amazon Q CLI is a command-line interface for Amazon Q";
        let terms = extract_keywords_and_ngrams(text);
        
        // Check unigrams
        assert!(terms.contains(&"amazon".to_string()));
        assert!(terms.contains(&"cli".to_string()));
        assert!(terms.contains(&"command".to_string()));
        assert!(terms.contains(&"line".to_string()));
        assert!(terms.contains(&"interface".to_string()));
        
        // Check bigrams
        assert!(terms.contains(&"amazon_q".to_string()));
        assert!(terms.contains(&"q_cli".to_string()));
        assert!(terms.contains(&"command_line".to_string()));
        assert!(terms.contains(&"line_interface".to_string()));
        
        // Check trigrams
        assert!(terms.contains(&"amazon_q_cli".to_string()));
        assert!(terms.contains(&"q_cli_command".to_string()));
        assert!(terms.contains(&"cli_command_line".to_string()));
        assert!(terms.contains(&"command_line_interface".to_string()));
    }
    
    #[test]
    fn test_calculate_tf_idf() {
        let terms = vec![
            "amazon".to_string(),
            "q".to_string(),
            "cli".to_string(),
            "amazon".to_string(),
            "q".to_string(),
            "command".to_string(),
            "line".to_string(),
            "interface".to_string(),
        ];
        
        let corpus_frequencies = get_corpus_frequencies();
        let tf_idf_scores = calculate_tf_idf(&terms, &corpus_frequencies);
        
        // Check that all terms have scores
        assert!(tf_idf_scores.contains_key("amazon"));
        assert!(tf_idf_scores.contains_key("q"));
        assert!(tf_idf_scores.contains_key("cli"));
        assert!(tf_idf_scores.contains_key("command"));
        assert!(tf_idf_scores.contains_key("line"));
        assert!(tf_idf_scores.contains_key("interface"));
        
        // Check that terms with higher frequency have higher scores
        assert!(tf_idf_scores["amazon"] > tf_idf_scores["command"]);
        assert!(tf_idf_scores["q"] > tf_idf_scores["interface"]);
    }
    
    #[test]
    fn test_map_keywords_to_topics() {
        let mut tf_idf_scores = HashMap::new();
        tf_idf_scores.insert("amazon".to_string(), 0.5);
        tf_idf_scores.insert("q".to_string(), 0.4);
        tf_idf_scores.insert("cli".to_string(), 0.3);
        tf_idf_scores.insert("help".to_string(), 0.2);
        
        let topic_scores = map_keywords_to_topics(&tf_idf_scores);
        
        // Check that relevant topics have scores
        assert!(topic_scores.contains_key("AmazonQ"));
        assert!(topic_scores.contains_key("Help"));
        
        // Check that AmazonQ has a higher score than Help
        assert!(topic_scores["AmazonQ"] > topic_scores["Help"]);
    }
    
    #[test]
    fn test_perform_topic_modeling() {
        let mut conv = Conversation::new("test-id".to_string());
        conv.add_user_message("I need help with Amazon Q CLI".to_string())
            .add_assistant_message("Sure, what do you want to know about Amazon Q CLI?".to_string(), None)
            .add_user_message("How do I save conversations automatically?".to_string());
        
        let topics = perform_topic_modeling(&conv);
        
        // Check that topics were extracted
        assert!(!topics.is_empty());
        
        // Check that AmazonQ is one of the top topics
        let has_amazon_q = topics.iter().any(|(topic, _)| topic == "AmazonQ");
        assert!(has_amazon_q);
    }
    
    #[test]
    fn test_extract_topics_with_mock_conversations() {
        let conversation_types = vec![
            "amazon_q_cli",
            "feature_request",
            "technical",
            "multi_topic",
        ];
        
        for conv_type in conversation_types {
            let conv = create_mock_conversation(conv_type);
            
            // Extract topics using the advanced extractor
            let (main_topic, sub_topic, action_type) = extract_topics(&conv);
            
            // Check that topics are not empty
            assert!(!main_topic.is_empty());
            assert!(!sub_topic.is_empty());
            assert!(!action_type.is_empty());
            
            // Check that the main topic and sub-topic are different
            assert_ne!(main_topic, sub_topic);
            
            // Check specific conversation types
            match conv_type {
                "amazon_q_cli" => {
                    assert_eq!(main_topic, "AmazonQ");
                    assert!(sub_topic == "CLI" || sub_topic == "Saving" || sub_topic == "Conversations");
                },
                "feature_request" => {
                    assert!(action_type == "FeatureRequest");
                },
                "technical" => {
                    assert!(action_type == "Programming" || action_type == "Technical" || action_type == "Troubleshooting");
                },
                _ => {}
            }
        }
    }
    
    #[test]
    fn test_extract_keywords_with_language() {
        let mut conv = Conversation::new("test-id".to_string());
        conv.add_user_message("Amazon Q CLI is a command-line interface for Amazon Q".to_string());
        
        // Test with English
        let keywords = extract_keywords_with_language(&conv, "en");
        assert!(keywords.contains(&"amazon".to_string()));
        assert!(keywords.contains(&"cli".to_string()));
        
        // Test with non-English (simplified implementation)
        let keywords_non_en = extract_keywords_with_language(&conv, "es");
        assert!(keywords_non_en.contains(&"lang_es".to_string()));
    }
    
    #[test]
    fn test_extract_technical_terms() {
        let mut conv = Conversation::new("test-id".to_string());
        conv.add_user_message("I'm having an issue with the /save command in Amazon Q CLI. The error code is ERROR_123.".to_string())
            .add_assistant_message("Let me help you with that.".to_string(), None)
            .add_user_message("I tried using the function save_conversation() but it doesn't work with the file path /Users/me/documents/conversations.json".to_string());
        
        let terms = extract_technical_terms(&conv);
        
        // Check that technical terms were extracted
        assert!(terms.iter().any(|t| t.contains("/save")));
        assert!(terms.iter().any(|t| t.contains("ERROR_123")));
        assert!(terms.iter().any(|t| t.contains("save_conversation()")));
        assert!(terms.iter().any(|t| t.contains("/Users/me/documents/conversations.json")));
    }
    
    #[test]
    fn test_analyze_conversation_structure() {
        // Test Q&A pattern
        let mut qa_conv = Conversation::new("test-id".to_string());
        qa_conv.add_user_message("What is Amazon Q?".to_string())
            .add_assistant_message("Amazon Q is an AI assistant.".to_string(), None)
            .add_user_message("How do I use it?".to_string())
            .add_assistant_message("You can use it via CLI or web interface.".to_string(), None);
        
        let qa_context = analyze_conversation_structure(&qa_conv);
        assert!(qa_context.contains_key("qa_pattern"));
        assert!(qa_context["qa_pattern"] > 0.5);
        
        // Test technical discussion
        let mut tech_conv = Conversation::new("test-id".to_string());
        tech_conv.add_user_message("Here's my code:\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```".to_string())
            .add_assistant_message("That looks good.".to_string(), None);
        
        let tech_context = analyze_conversation_structure(&tech_conv);
        assert!(tech_context.contains_key("technical_discussion"));
        assert!(tech_context["technical_discussion"] > 0.5);
        
        // Test multi-turn conversation
        let mut multi_conv = Conversation::new("test-id".to_string());
        for i in 0..4 {
            multi_conv.add_user_message(format!("Message {}", i))
                .add_assistant_message(format!("Response {}", i), None);
        }
        
        let multi_context = analyze_conversation_structure(&multi_conv);
        assert!(multi_context.contains_key("multi_turn"));
        assert!(multi_context["multi_turn"] > 0.5);
    }
    
    #[test]
    fn test_apply_latent_semantic_analysis() {
        let mut tf_idf_scores = HashMap::new();
        tf_idf_scores.insert("lambda".to_string(), 0.5);
        tf_idf_scores.insert("s3".to_string(), 0.4);
        tf_idf_scores.insert("ec2".to_string(), 0.3);
        tf_idf_scores.insert("aws".to_string(), 0.6);
        
        let lsa_scores = apply_latent_semantic_analysis(&tf_idf_scores);
        
        // Check that all terms have scores
        assert!(lsa_scores.contains_key("lambda"));
        assert!(lsa_scores.contains_key("s3"));
        assert!(lsa_scores.contains_key("ec2"));
        assert!(lsa_scores.contains_key("aws"));
        
        // Check that terms in the same cluster have boosted scores
        assert!(lsa_scores["lambda"] > tf_idf_scores["lambda"]);
        assert!(lsa_scores["s3"] > tf_idf_scores["s3"]);
        assert!(lsa_scores["ec2"] > tf_idf_scores["ec2"]);
    }
    
    #[test]
    fn test_map_keywords_to_topics_with_language() {
        let mut tf_idf_scores = HashMap::new();
        tf_idf_scores.insert("amazon".to_string(), 0.5);
        tf_idf_scores.insert("q".to_string(), 0.4);
        tf_idf_scores.insert("cli".to_string(), 0.3);
        
        // Test with English
        let topic_scores_en = map_keywords_to_topics_with_language(&tf_idf_scores, "en");
        assert!(topic_scores_en.contains_key("AmazonQ"));
        
        // Test with non-English (simplified implementation)
        let topic_scores_non_en = map_keywords_to_topics_with_language(&tf_idf_scores, "es");
        assert!(topic_scores_non_en.contains_key("AmazonQ"));
        // Check that the score is boosted for non-English
        assert!(topic_scores_non_en["AmazonQ"] > topic_scores_en["AmazonQ"]);
    }
    
    #[test]
    fn test_determine_main_topic_with_context() {
        let keywords = vec![
            "amazon".to_string(),
            "q".to_string(),
            "cli".to_string(),
            "save".to_string(),
            "conversation".to_string(),
        ];
        
        // Test with empty context
        let empty_context = HashMap::new();
        let main_topic_empty = determine_main_topic_with_context(&keywords, &empty_context, "en");
        assert_eq!(main_topic_empty, "AmazonQ");
        
        // Test with technical discussion context
        let mut tech_context = HashMap::new();
        tech_context.insert("technical_discussion".to_string(), 0.9);
        let main_topic_tech = determine_main_topic_with_context(&keywords, &tech_context, "en");
        assert_eq!(main_topic_tech, "AmazonQ");
        
        // Test with programming language keywords
        let prog_keywords = vec![
            "rust".to_string(),
            "cargo".to_string(),
            "crate".to_string(),
        ];
        let main_topic_prog = determine_main_topic_with_context(&prog_keywords, &tech_context, "en");
        assert_eq!(main_topic_prog, "Rust");
    }
    
    #[test]
    fn test_determine_sub_topic_with_context() {
        let keywords = vec![
            "amazon".to_string(),
            "q".to_string(),
            "cli".to_string(),
            "save".to_string(),
            "conversation".to_string(),
        ];
        
        // Test with empty context
        let empty_context = HashMap::new();
        let sub_topic_empty = determine_sub_topic_with_context(&keywords, "AmazonQ", &empty_context, "en");
        assert_eq!(sub_topic_empty, "CLI");
        
        // Test with technical discussion context
        let mut tech_context = HashMap::new();
        tech_context.insert("technical_discussion".to_string(), 0.9);
        let sub_topic_tech = determine_sub_topic_with_context(&keywords, "AmazonQ", &tech_context, "en");
        assert_eq!(sub_topic_tech, "CLI");
        
        // Test with AWS main topic
        let aws_keywords = vec![
            "aws".to_string(),
            "lambda".to_string(),
            "function".to_string(),
        ];
        let sub_topic_aws = determine_sub_topic_with_context(&aws_keywords, "AWS", &tech_context, "en");
        assert_eq!(sub_topic_aws, "Lambda");
    }
    
    #[test]
    fn test_determine_action_type_with_context() {
        // Test troubleshooting with code blocks and errors
        let mut trouble_conv = Conversation::new("test-id".to_string());
        trouble_conv.add_user_message("I'm getting an error with this code:\n```\nfn main() {\n    let x = 5;\n    println!(\"{}\", y); // Error: y is not defined\n}\n```".to_string());
        
        let empty_context = HashMap::new();
        let action_type_trouble = determine_action_type_with_context(&trouble_conv, &empty_context, "en");
        assert_eq!(action_type_trouble, "Troubleshooting");
        
        // Test programming with code blocks
        let mut prog_conv = Conversation::new("test-id".to_string());
        prog_conv.add_user_message("Here's my code:\n```\nfn main() {\n    let x = 5;\n    println!(\"{}\", x);\n}\n```".to_string());
        
        let mut tech_context = HashMap::new();
        tech_context.insert("technical_discussion".to_string(), 0.9);
        let action_type_prog = determine_action_type_with_context(&prog_conv, &tech_context, "en");
        assert_eq!(action_type_prog, "Programming");
        
        // Test feature request
        let mut feature_conv = Conversation::new("test-id".to_string());
        feature_conv.add_user_message("I would like to request a feature for automatic naming of saved conversations.".to_string());
        
        let action_type_feature = determine_action_type_with_context(&feature_conv, &empty_context, "en");
        assert_eq!(action_type_feature, "FeatureRequest");
        
        // Test learning
        let mut learn_conv = Conversation::new("test-id".to_string());
        learn_conv.add_user_message("Can you teach me how to use Amazon Q CLI?".to_string());
        
        let mut qa_context = HashMap::new();
        qa_context.insert("qa_pattern".to_string(), 0.9);
        let action_type_learn = determine_action_type_with_context(&learn_conv, &qa_context, "en");
        assert_eq!(action_type_learn, "Learning");
    }
    
    #[test]
    fn test_refine_topics() {
        // Test empty topics
        let (main, sub, action) = refine_topics("".to_string(), "".to_string(), "".to_string(), &Conversation::new("test-id".to_string()));
        assert_eq!(main, "Unknown");
        assert_ne!(sub, "");
        assert_eq!(action, "Conversation");
        
        // Test Amazon Q variations
        let (main, sub, action) = refine_topics("Amazon Q".to_string(), "CLI".to_string(), "Help".to_string(), &Conversation::new("test-id".to_string()));
        assert_eq!(main, "AmazonQ");
        assert_eq!(sub, "CLI");
        assert_eq!(action, "Help");
        
        // Test AWS services
        let (main, sub, action) = refine_topics("Lambda".to_string(), "Function".to_string(), "Programming".to_string(), &Conversation::new("test-id".to_string()));
        assert_eq!(main, "AWS");
        assert_eq!(sub, "Lambda");
        assert_eq!(action, "Programming");
        
        // Test duplicate topics
        let (main, sub, action) = refine_topics("Python".to_string(), "Python".to_string(), "Learning".to_string(), &Conversation::new("test-id".to_string()));
        assert_eq!(main, "Python");
        assert_eq!(sub, "Learning");
        assert_eq!(action, "Learning");
    }
}
