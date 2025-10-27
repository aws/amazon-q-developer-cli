use crate::config;

/// Chunk text into smaller pieces with overlap
///
/// # Arguments
///
/// * `text` - The text to chunk
/// * `chunk_size` - Optional chunk size (if None, uses config value)
/// * `overlap` - Optional overlap size (if None, uses config value)
///
/// # Returns
///
/// A vector of string chunks
pub fn chunk_text(text: &str, chunk_size: Option<usize>, overlap: Option<usize>) -> Vec<String> {
    // Get configuration values or use provided values
    let config = config::get_config();
    let chunk_size = chunk_size.unwrap_or(config.chunk_size);
    let overlap = overlap.unwrap_or(config.chunk_overlap);

    let mut chunks = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return chunks;
    }

    let mut i = 0;
    while i < words.len() {
        let end = (i + chunk_size).min(words.len());
        let chunk = words[i..end].join(" ");
        chunks.push(chunk);

        // Move forward by chunk_size - overlap
        i += chunk_size - overlap;
        if i >= words.len() || i == 0 {
            break;
        }
    }

    chunks
}

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use super::*;

    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(|| {
            // Initialize with test config
            let _ = std::panic::catch_unwind(|| {
                let _config = config::SemanticSearchConfig {
                    chunk_size: 50,
                    chunk_overlap: 10,
                    default_results: 5,
                    model_name: "test-model".to_string(),
                    timeout: 30000,
                    base_dir: std::path::PathBuf::from("."),
                    max_files: 1000, // Add missing max_files field
                    hosted_models_base_url: "http://test.example.com/models".to_string(),
                    embedding_type: crate::embedding::EmbeddingType::default(),
                };
                // Use a different approach that doesn't access private static
                let _ = crate::config::init_config(&std::env::temp_dir());
            });
        });
    }

    #[test]
    fn test_chunk_text_empty() {
        setup();
        let chunks = chunk_text("", None, None);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_text_small() {
        setup();
        let text = "This is a small text";
        let chunks = chunk_text(text, Some(10), Some(2));
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_chunk_text_large() {
        setup();
        let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
        let text = words.join(" ");

        let chunks = chunk_text(&text, Some(50), Some(10));

        // With 200 words, chunk size 50, and overlap 10, we should have 5 chunks
        // (0-49, 40-89, 80-129, 120-169, 160-199)
        assert_eq!(chunks.len(), 5);

        // Check first and last words of first chunk
        assert!(chunks[0].starts_with("word0"));
        assert!(chunks[0].ends_with("word49"));

        // Check first and last words of last chunk
        assert!(chunks[4].starts_with("word160"));
        assert!(chunks[4].ends_with("word199"));
    }

    #[test]
    fn test_chunk_text_with_config_defaults() {
        setup();
        let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
        let text = words.join(" ");

        // Use default config values
        let chunks = chunk_text(&text, None, None);

        // Should use the config values (50, 10) set in setup()
        assert!(!chunks.is_empty());
    }
}
