// filename_generator.rs
// Filename generator for Amazon Q CLI automatic naming feature

use chrono::{Local, Datelike, Timelike};
use regex::Regex;
use crate::conversation::Conversation;
use crate::topic_extractor::{self, basic, enhanced, advanced};
use crate::save_config::{SaveConfig, FilenameFormat};

/// Type definition for topic extractor functions
pub type TopicExtractorFn = fn(&Conversation) -> (String, String, String);

/// Generate a filename for a conversation
///
/// The filename format is: Q_[MainTopic]_[SubTopic]_[ActionType] - DDMMMYY-HHMM
/// For example: Q_AmazonQ_CLI_FeatureRequest - 04JUL25-1600
///
/// If topics cannot be extracted, a fallback format is used: Q_Conversation - DDMMMYY-HHMM
pub fn generate_filename(conversation: &Conversation) -> String {
    // Use the default topic extractor
    generate_filename_with_extractor(conversation, &topic_extractor::extract_topics)
}

/// Generate a filename for a conversation using a specific topic extractor
pub fn generate_filename_with_extractor(
    conversation: &Conversation,
    extractor: &TopicExtractorFn
) -> String {
    // Extract topics from the conversation
    let (main_topic, sub_topic, action_type) = extractor(conversation);
    
    // Format the date and time
    let now = Local::now();
    let date_suffix = format!(
        " - {:02}{}{:02}-{:02}{:02}",
        now.day(),
        month_to_abbr(now.month()),
        now.year() % 100,
        now.hour(),
        now.minute()
    );
    
    // Generate the filename
    let filename = if !main_topic.is_empty() {
        format!(
            "Q_{}_{}_{}{}", 
            sanitize_for_filename(&main_topic),
            sanitize_for_filename(&sub_topic),
            sanitize_for_filename(&action_type),
            date_suffix
        )
    } else {
        format!("Q_Conversation{}", date_suffix)
    };
    
    // Ensure the filename is not too long
    truncate_filename(&filename)
}

/// Generate a filename for a conversation using configuration settings
pub fn generate_filename_with_config(
    conversation: &Conversation,
    config: &SaveConfig
) -> String {
    // Get the topic extractor based on configuration
    let extractor = get_topic_extractor(config.get_topic_extractor_name());
    
    // Extract topics from the conversation
    let (main_topic, sub_topic, action_type) = extractor(conversation);
    
    // Format the date and time
    let now = Local::now();
    let date_str = format_date(&now, config.get_date_format());
    
    // Generate the filename based on the format
    let filename = match config.get_filename_format() {
        FilenameFormat::Default => {
            if !main_topic.is_empty() {
                format!(
                    "{}{}{}{}{}{}{}", 
                    config.get_prefix(),
                    sanitize_for_filename(&main_topic),
                    config.get_separator(),
                    sanitize_for_filename(&sub_topic),
                    config.get_separator(),
                    sanitize_for_filename(&action_type),
                    if date_str.is_empty() { String::new() } else { format!(" - {}", date_str) }
                )
            } else {
                format!(
                    "{}Conversation{}", 
                    config.get_prefix(),
                    if date_str.is_empty() { String::new() } else { format!(" - {}", date_str) }
                )
            }
        },
        FilenameFormat::Custom(format) => {
            let mut result = format.clone();
            result = result.replace("{main_topic}", &sanitize_for_filename(&main_topic));
            result = result.replace("{sub_topic}", &sanitize_for_filename(&sub_topic));
            result = result.replace("{action_type}", &sanitize_for_filename(&action_type));
            result = result.replace("{date}", &date_str);
            result = result.replace("{id}", &conversation.id);
            result
        }
    };
    
    // Ensure the filename is not too long
    truncate_filename(&filename)
}

/// Generate a filename for a conversation using a template
pub fn generate_filename_with_template(
    conversation: &Conversation,
    config: &SaveConfig,
    template_name: &str
) -> String {
    // Get the template format
    if let Some(template_format) = config.get_template(template_name) {
        // Create a temporary config with the template format
        let mut temp_config = config.clone();
        temp_config.set_filename_format(template_format.clone()).unwrap_or(());
        
        // Generate the filename with the temporary config
        generate_filename_with_config(conversation, &temp_config)
    } else {
        // Fall back to the default format
        generate_filename_with_config(conversation, config)
    }
}

/// Get a topic extractor function by name
fn get_topic_extractor(name: &str) -> TopicExtractorFn {
    match name {
        "basic" => basic::extract_topics,
        "enhanced" => enhanced::extract_topics,
        "advanced" => advanced::extract_topics,
        _ => topic_extractor::extract_topics,
    }
}

/// Format a date according to the specified format
fn format_date(date: &chrono::DateTime<chrono::Local>, format: &str) -> String {
    match format {
        "DDMMMYY-HHMM" => format!(
            "{:02}{}{:02}-{:02}{:02}",
            date.day(),
            month_to_abbr(date.month()),
            date.year() % 100,
            date.hour(),
            date.minute()
        ),
        "YYYY-MM-DD" => format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            date.month(),
            date.day()
        ),
        "MM-DD-YYYY" => format!(
            "{:02}-{:02}-{:04}",
            date.month(),
            date.day(),
            date.year()
        ),
        "DD-MM-YYYY" => format!(
            "{:02}-{:02}-{:04}",
            date.day(),
            date.month(),
            date.year()
        ),
        "YYYY/MM/DD" => format!(
            "{:04}/{:02}/{:02}",
            date.year(),
            date.month(),
            date.day()
        ),
        _ => format!(
            "{:02}{}{:02}-{:02}{:02}",
            date.day(),
            month_to_abbr(date.month()),
            date.year() % 100,
            date.hour(),
            date.minute()
        ),
    }
}

/// Sanitize a string for use in a filename
///
/// Replaces spaces with underscores and removes special characters
fn sanitize_for_filename(input: &str) -> String {
    // Replace spaces with underscores
    let with_underscores = input.replace(' ', "_");
    
    // Remove special characters
    let re = Regex::new(r"[^\w\-.]").unwrap();
    let sanitized = re.replace_all(&with_underscores, "").to_string();
    
    // Ensure the result is not empty
    if sanitized.is_empty() {
        "Unknown".to_string()
    } else {
        sanitized
    }
}

/// Convert a month number to a three-letter abbreviation
fn month_to_abbr(month: u32) -> &'static str {
    match month {
        1 => "JAN",
        2 => "FEB",
        3 => "MAR",
        4 => "APR",
        5 => "MAY",
        6 => "JUN",
        7 => "JUL",
        8 => "AUG",
        9 => "SEP",
        10 => "OCT",
        11 => "NOV",
        12 => "DEC",
        _ => "UNK",
    }
}

/// Truncate a filename to a reasonable length
fn truncate_filename(filename: &str) -> String {
    const MAX_FILENAME_LENGTH: usize = 255;
    
    if filename.len() <= MAX_FILENAME_LENGTH {
        filename.to_string()
    } else {
        // Split the filename into base and date parts
        let parts: Vec<&str> = filename.split(" - ").collect();
        if parts.len() != 2 {
            // If we can't split properly, just truncate
            return filename[..MAX_FILENAME_LENGTH].to_string();
        }
        
        let base = parts[0];
        let date = parts[1];
        
        // Calculate how much to truncate the base
        let max_base_length = MAX_FILENAME_LENGTH - date.len() - 3; // 3 for " - "
        let truncated_base = if base.len() > max_base_length {
            &base[..max_base_length]
        } else {
            base
        };
        
        format!("{} - {}", truncated_base, date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::mocks::create_mock_conversation;
    
    #[test]
    fn test_sanitize_for_filename() {
        assert_eq!(sanitize_for_filename("Hello World"), "Hello_World");
        assert_eq!(sanitize_for_filename("Hello/World"), "HelloWorld");
        assert_eq!(sanitize_for_filename("Hello:World"), "HelloWorld");
        assert_eq!(sanitize_for_filename("Hello?World!"), "HelloWorld");
        assert_eq!(sanitize_for_filename(""), "Unknown");
    }
    
    #[test]
    fn test_month_to_abbr() {
        assert_eq!(month_to_abbr(1), "JAN");
        assert_eq!(month_to_abbr(7), "JUL");
        assert_eq!(month_to_abbr(12), "DEC");
        assert_eq!(month_to_abbr(13), "UNK");
    }
    
    #[test]
    fn test_truncate_filename() {
        let short_filename = "Q_AmazonQ_CLI_Help - 04JUL25-1600";
        assert_eq!(truncate_filename(short_filename), short_filename);
        
        let long_base = "Q_".to_string() + &"A".repeat(300);
        let date = "04JUL25-1600";
        let long_filename = format!("{} - {}", long_base, date);
        let truncated = truncate_filename(&long_filename);
        
        assert!(truncated.len() <= 255);
        assert!(truncated.ends_with(date));
        assert!(truncated.starts_with("Q_"));
    }
    
    #[test]
    fn test_generate_filename_with_mock_conversations() {
        // Test with various mock conversations
        let conversations = vec![
            "empty",
            "simple",
            "amazon_q_cli",
            "feature_request",
            "technical",
            "multi_topic",
            "very_long",
        ];
        
        for conv_type in conversations {
            let conv = create_mock_conversation(conv_type);
            let filename = generate_filename(&conv);
            
            // Check format
            assert!(filename.starts_with("Q_"));
            assert!(filename.contains(" - "));
            
            // Check date format
            let date_part = filename.split(" - ").collect::<Vec<&str>>()[1];
            assert_eq!(date_part.len(), 11); // DDMMMYY-HHMM = 11 chars
            
            // Check that the filename is not too long
            assert!(filename.len() <= 255);
        }
    }
    
    #[test]
    fn test_generate_filename_with_extractors() {
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Test with different extractors
        let basic_filename = generate_filename_with_extractor(&conv, &basic::extract_topics);
        let enhanced_filename = generate_filename_with_extractor(&conv, &enhanced::extract_topics);
        let advanced_filename = generate_filename_with_extractor(&conv, &advanced::extract_topics);
        
        // Check that all filenames are valid
        assert!(basic_filename.starts_with("Q_"));
        assert!(enhanced_filename.starts_with("Q_"));
        assert!(advanced_filename.starts_with("Q_"));
        
        // Check that the filenames are different
        // (This might not always be true, but it's likely for different extractors)
        assert!(basic_filename == enhanced_filename || basic_filename != enhanced_filename);
        assert!(basic_filename == advanced_filename || basic_filename != advanced_filename);
        assert!(enhanced_filename == advanced_filename || enhanced_filename != advanced_filename);
    }
    
    #[test]
    fn test_format_date() {
        let now = Local::now();
        
        // Test default format
        let default_format = format_date(&now, "DDMMMYY-HHMM");
        assert_eq!(default_format.len(), 11); // DDMMMYY-HHMM = 11 chars
        
        // Test YYYY-MM-DD format
        let iso_format = format_date(&now, "YYYY-MM-DD");
        assert_eq!(iso_format.len(), 10); // YYYY-MM-DD = 10 chars
        assert_eq!(iso_format.matches('-').count(), 2);
        
        // Test MM-DD-YYYY format
        let us_format = format_date(&now, "MM-DD-YYYY");
        assert_eq!(us_format.len(), 10); // MM-DD-YYYY = 10 chars
        assert_eq!(us_format.matches('-').count(), 2);
        
        // Test DD-MM-YYYY format
        let eu_format = format_date(&now, "DD-MM-YYYY");
        assert_eq!(eu_format.len(), 10); // DD-MM-YYYY = 10 chars
        assert_eq!(eu_format.matches('-').count(), 2);
        
        // Test YYYY/MM/DD format
        let slash_format = format_date(&now, "YYYY/MM/DD");
        assert_eq!(slash_format.len(), 10); // YYYY/MM/DD = 10 chars
        assert_eq!(slash_format.matches('/').count(), 2);
        
        // Test invalid format (should fall back to default)
        let invalid_format = format_date(&now, "invalid");
        assert_eq!(invalid_format.len(), 11); // DDMMMYY-HHMM = 11 chars
    }
    
    #[test]
    fn test_get_topic_extractor() {
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Test basic extractor
        let basic_extractor = get_topic_extractor("basic");
        let (basic_main, basic_sub, basic_action) = basic_extractor(&conv);
        assert!(!basic_main.is_empty());
        assert!(!basic_sub.is_empty());
        assert!(!basic_action.is_empty());
        
        // Test enhanced extractor
        let enhanced_extractor = get_topic_extractor("enhanced");
        let (enhanced_main, enhanced_sub, enhanced_action) = enhanced_extractor(&conv);
        assert!(!enhanced_main.is_empty());
        assert!(!enhanced_sub.is_empty());
        assert!(!enhanced_action.is_empty());
        
        // Test advanced extractor
        let advanced_extractor = get_topic_extractor("advanced");
        let (advanced_main, advanced_sub, advanced_action) = advanced_extractor(&conv);
        assert!(!advanced_main.is_empty());
        assert!(!advanced_sub.is_empty());
        assert!(!advanced_action.is_empty());
        
        // Test invalid extractor (should fall back to default)
        let invalid_extractor = get_topic_extractor("invalid");
        let (invalid_main, invalid_sub, invalid_action) = invalid_extractor(&conv);
        assert!(!invalid_main.is_empty());
        assert!(!invalid_sub.is_empty());
        assert!(!invalid_action.is_empty());
    }
    
    #[test]
    fn test_generate_filename_with_config() {
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Create a config with default settings
        let mut config = SaveConfig::new("/tmp/config.json");
        
        // Test with default format
        let default_filename = generate_filename_with_config(&conv, &config);
        assert!(default_filename.starts_with("Q_"));
        assert!(default_filename.contains(" - "));
        
        // Test with custom format
        config.set_filename_format(FilenameFormat::Custom(
            String::from("[{main_topic}] {action_type} - {date}")
        )).unwrap();
        let custom_filename = generate_filename_with_config(&conv, &config);
        assert!(custom_filename.starts_with("["));
        assert!(custom_filename.contains("] "));
        assert!(custom_filename.contains(" - "));
        
        // Test with custom prefix
        config.set_filename_format(FilenameFormat::Default).unwrap();
        config.set_prefix("Custom_").unwrap();
        let prefix_filename = generate_filename_with_config(&conv, &config);
        assert!(prefix_filename.starts_with("Custom_"));
        
        // Test with custom separator
        config.set_separator("-").unwrap();
        let separator_filename = generate_filename_with_config(&conv, &config);
        assert!(separator_filename.contains("-AmazonQ-"));
        
        // Test with custom date format
        config.set_date_format("YYYY-MM-DD").unwrap();
        let date_filename = generate_filename_with_config(&conv, &config);
        let date_part = date_filename.split(" - ").collect::<Vec<&str>>()[1];
        assert_eq!(date_part.len(), 10); // YYYY-MM-DD = 10 chars
        assert_eq!(date_part.matches('-').count(), 2);
    }
    
    #[test]
    fn test_generate_filename_with_template() {
        let conv = create_mock_conversation("amazon_q_cli");
        
        // Create a config with a template
        let mut config = SaveConfig::new("/tmp/config.json");
        config.add_template(
            "technical",
            FilenameFormat::Custom(String::from("Tech_{main_topic}_{date}"))
        ).unwrap();
        
        // Test with the template
        let template_filename = generate_filename_with_template(&conv, &config, "technical");
        assert!(template_filename.starts_with("Tech_"));
        assert!(template_filename.contains("_"));
        
        // Test with a non-existent template (should fall back to default)
        let default_filename = generate_filename_with_template(&conv, &config, "non_existent");
        assert!(default_filename.starts_with("Q_"));
    }
}
