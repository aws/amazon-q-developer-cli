use std::{
    env,
    fs,
};

use semantic_search_client::config;
use semantic_search_client::processing::text_chunker::chunk_text;

#[test]
fn test_chunk_text() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_chunk_text");
    fs::create_dir_all(&temp_dir).unwrap();

    // Initialize config
    config::init_config(&temp_dir).unwrap();

    let text = "This is a test text. It has multiple sentences. We want to split it into chunks.";

    // Test with chunk size larger than text
    let chunks = chunk_text(text, Some(100), Some(0));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], text);

    // Test with smaller chunk size
    let chunks = chunk_text(text, Some(5), Some(0));
    assert!(chunks.len() > 1);

    // Verify all text is preserved when joined
    let combined = chunks.join(" ");
    assert_eq!(combined, text);

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_chunk_text_with_overlap() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_chunk_text_overlap");
    fs::create_dir_all(&temp_dir).unwrap();

    // Initialize config
    config::init_config(&temp_dir).unwrap();

    let text = "This is a test text. It has multiple sentences. We want to split it into chunks.";

    // Test with chunk size larger than text
    let chunks = chunk_text(text, Some(100), Some(10));
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], text);

    // Test with smaller chunk size and overlap
    let chunks = chunk_text(text, Some(5), Some(2));
    assert!(chunks.len() > 1);

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}
