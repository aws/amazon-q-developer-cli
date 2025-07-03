use std::path::Path;
use std::{
    env,
    fs,
};

use semantic_search_client::config;
use semantic_search_client::processing::file_processor::process_file;

#[test]
fn test_process_text_file() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_process_file");
    fs::create_dir_all(&temp_dir).unwrap();

    // Initialize config
    config::init_config(&temp_dir).unwrap();

    // Create a test text file
    let test_file = temp_dir.join("test.txt");
    fs::write(
        &test_file,
        "This is a test file\nwith multiple lines\nfor testing file processing",
    )
    .unwrap();

    // Process the file
    let items = process_file(&test_file).unwrap();

    // Verify the file was processed correctly
    assert!(!items.is_empty());

    // Check that the text content is present
    let text = items[0].get("text").and_then(|v| v.as_str()).unwrap_or("");
    assert!(text.contains("This is a test file"));

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_process_markdown_file() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_process_markdown");
    fs::create_dir_all(&temp_dir).unwrap();

    // Initialize config
    config::init_config(&temp_dir).unwrap();

    // Create a test markdown file
    let test_file = temp_dir.join("test.md");
    fs::write(
        &test_file,
        "# Test Markdown\n\nThis is a **markdown** file\n\n## Section\n\nWith formatting",
    )
    .unwrap();

    // Process the file
    let items = process_file(&test_file).unwrap();

    // Verify the file was processed correctly
    assert!(!items.is_empty());

    // Check that the text content is present and markdown is preserved
    let text = items[0].get("text").and_then(|v| v.as_str()).unwrap_or("");
    assert!(text.contains("# Test Markdown"));
    assert!(text.contains("**markdown**"));

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_process_nonexistent_file() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_nonexistent");
    fs::create_dir_all(&temp_dir).unwrap();

    // Initialize config
    config::init_config(&temp_dir).unwrap();

    // Try to process a file that doesn't exist
    let nonexistent_file = Path::new("nonexistent_file.txt");
    let result = process_file(nonexistent_file);

    // Verify the operation failed
    assert!(result.is_err());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_process_binary_file() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_process_binary");
    fs::create_dir_all(&temp_dir).unwrap();

    // Initialize config
    config::init_config(&temp_dir).unwrap();

    // Create a test binary file (just some non-UTF8 bytes)
    let test_file = temp_dir.join("test.bin");
    fs::write(&test_file, [0xff, 0xfe, 0x00, 0x01, 0x02]).unwrap();

    // Process the file - this should still work but might not extract meaningful text
    let result = process_file(&test_file);

    // The processor should handle binary files gracefully
    // Either by returning an empty result or by extracting what it can
    if let Ok(items) = result {
        if !items.is_empty() {
            let text = items[0].get("text").and_then(|v| v.as_str()).unwrap_or("");
            // The text might be empty or contain replacement characters
            assert!(text.is_empty() || text.contains("�"));
        }
    }

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}
