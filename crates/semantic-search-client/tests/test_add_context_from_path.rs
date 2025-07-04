use std::path::Path;
use std::{
    env,
    fs,
};

use semantic_search_client::SemanticSearchClient;
use semantic_search_client::types::ProgressStatus;

#[test]
fn test_add_context_from_path_with_directory() {
    if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
        println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
        return;
    }
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_dir");
    let base_dir = temp_dir.join("semantic_search");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a test directory with a file
    let test_dir = temp_dir.join("test_dir");
    fs::create_dir_all(&test_dir).unwrap();
    let test_file = test_dir.join("test.txt");
    fs::write(&test_file, "This is a test file").unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Add a context from the directory
    let _context_id = client
        .add_context_from_path(
            &test_dir,
            "Test Context",
            "Test Description",
            true,
            None::<fn(ProgressStatus)>,
        )
        .unwrap();

    // Verify the context was created
    let contexts = client.get_contexts();
    assert!(!contexts.is_empty());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_add_context_from_path_with_file() {
    // Skip this test in CI environments
    if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
        println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
        return;
    }

    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_file");
    let base_dir = temp_dir.join("memory_bank");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a test file
    let test_file = temp_dir.join("test.txt");
    fs::write(&test_file, "This is a test file").unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Add a context from the file
    let _context_id = client
        .add_context_from_path(
            &test_file,
            "Test Context",
            "Test Description",
            true,
            None::<fn(ProgressStatus)>,
        )
        .unwrap();

    // Verify the context was created
    let contexts = client.get_contexts();
    assert!(!contexts.is_empty());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_add_context_from_path_with_invalid_path() {
    if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
        println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
        return;
    }
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_invalid");
    let base_dir = temp_dir.join("memory_bank");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Try to add a context from an invalid path
    let invalid_path = Path::new("/path/that/does/not/exist");
    let result = client.add_context_from_path(
        invalid_path,
        "Test Context",
        "Test Description",
        false,
        None::<fn(ProgressStatus)>,
    );

    // Verify the operation failed
    assert!(result.is_err());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_backward_compatibility() {
    // Skip this test in CI environments
    if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
        println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
        return;
    }

    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_compat");
    let base_dir = temp_dir.join("memory_bank");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a test directory with a file
    let test_dir = temp_dir.join("test_dir");
    fs::create_dir_all(&test_dir).unwrap();
    let test_file = test_dir.join("test.txt");
    fs::write(&test_file, "This is a test file").unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Add a context using the original method
    let _context_id = client
        .add_context_from_directory(
            &test_dir,
            "Test Context",
            "Test Description",
            true,
            None::<fn(ProgressStatus)>,
        )
        .unwrap();

    // Verify the context was created
    let contexts = client.get_contexts();
    assert!(!contexts.is_empty());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}
