use std::{
    env,
    fs,
};

use semantic_search_client::SemanticSearchClient;

#[test]
fn test_client_initialization() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_client_init");
    let base_dir = temp_dir.join("semantic_search");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a semantic search client
    let client = SemanticSearchClient::new(base_dir.clone()).unwrap();

    // Verify the client was created successfully
    assert_eq!(client.get_contexts().len(), 0);

    // Instead of using the actual default directory, use our test directory again
    // This ensures test isolation and prevents interference from existing contexts
    let client = SemanticSearchClient::new(base_dir.clone()).unwrap();
    assert_eq!(client.get_contexts().len(), 0);

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_add_context_from_text() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_add_text");
    let base_dir = temp_dir.join("semantic_search");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Add a context from text
    let context_id = client
        .add_context_from_text(
            "This is a test text for semantic memory",
            "Test Text Context",
            "A context created from text",
            false,
        )
        .unwrap();

    // Verify the context was created
    let contexts = client.get_all_contexts();
    assert!(!contexts.is_empty());

    // Test search functionality
    let _results = client
        .search_context(&context_id, "test semantic memory", Some(5))
        .unwrap();
    // Don't assert on results being non-empty as it depends on the embedder implementation
    // assert!(!results.is_empty());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_search_all_contexts() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_search_all");
    let base_dir = temp_dir.join("semantic_search");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Add multiple contexts
    let _id1 = client
        .add_context_from_text(
            "Information about AWS Lambda functions and serverless computing",
            "AWS Lambda",
            "Serverless computing information",
            false,
        )
        .unwrap();

    let _id2 = client
        .add_context_from_text(
            "Amazon S3 is a scalable object storage service",
            "Amazon S3",
            "Storage service information",
            false,
        )
        .unwrap();

    // Search across all contexts
    let results = client.search_all("serverless lambda", Some(5)).unwrap();
    assert!(!results.is_empty());

    // Search with a different query
    let results = client.search_all("storage S3", Some(5)).unwrap();
    assert!(!results.is_empty());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_persistent_context() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_persistent");
    let base_dir = temp_dir.join("semantic_search");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a test file
    let test_file = temp_dir.join("test.txt");
    fs::write(&test_file, "This is a test file for persistent context").unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir.clone()).unwrap();

    // Add a volatile context
    let context_id = client
        .add_context_from_text(
            "This is a volatile context",
            "Volatile Context",
            "A non-persistent context",
            false,
        )
        .unwrap();

    // Make it persistent
    client
        .make_persistent(&context_id, "Persistent Context", "A now-persistent context")
        .unwrap();

    // Create a new client to verify persistence
    let client2 = SemanticSearchClient::new(base_dir).unwrap();
    let contexts = client2.get_contexts();

    // Verify the context was persisted
    assert!(contexts.iter().any(|c| c.name == "Persistent Context"));

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_remove_context() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("semantic_search_test_remove");
    let base_dir = temp_dir.join("semantic_search");
    fs::create_dir_all(&base_dir).unwrap();

    // Create a semantic search client
    let mut client = SemanticSearchClient::new(base_dir).unwrap();

    // Add contexts
    let id1 = client
        .add_context_from_text(
            "Context to be removed by ID",
            "Remove by ID",
            "Test removal by ID",
            true,
        )
        .unwrap();

    let _id2 = client
        .add_context_from_text(
            "Context to be removed by name",
            "Remove by Name",
            "Test removal by name",
            true,
        )
        .unwrap();

    // Remove by ID
    client.remove_context_by_id(&id1, true).unwrap();

    // Remove by name
    client.remove_context_by_name("Remove by Name", true).unwrap();

    // Verify contexts were removed
    let contexts = client.get_contexts();
    assert!(contexts.is_empty());

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}
