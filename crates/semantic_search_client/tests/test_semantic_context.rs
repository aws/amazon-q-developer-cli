use std::collections::HashMap;
use std::{
    env,
    fs,
};

use semantic_search_client::client::SemanticContext;
use semantic_search_client::types::DataPoint;
use serde_json::Value;

#[test]
fn test_semantic_context_creation() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_semantic_context");
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("data.json");

    // Create a new semantic context
    let semantic_context = SemanticContext::new(data_path).unwrap();

    // Verify the context was created successfully
    assert_eq!(semantic_context.get_data_points().len(), 0);

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}

#[test]
fn test_add_data_points() {
    // Create a temporary directory for the test
    let temp_dir = env::temp_dir().join("memory_bank_test_add_data");
    fs::create_dir_all(&temp_dir).unwrap();

    let data_path = temp_dir.join("data.json");

    // Create a new semantic context
    let mut semantic_context = SemanticContext::new(data_path.clone()).unwrap();

    // Create data points
    let mut data_points = Vec::new();

    // First data point
    let mut payload1 = HashMap::new();
    payload1.insert(
        "text".to_string(),
        Value::String("This is the first test data point".to_string()),
    );
    payload1.insert("source".to_string(), Value::String("test1.txt".to_string()));

    // Create a mock embedding vector
    let vector1 = vec![0.1; 384]; // 384-dimensional vector with all values set to 0.1

    data_points.push(DataPoint {
        id: 0,
        payload: payload1,
        vector: vector1,
    });

    // Second data point
    let mut payload2 = HashMap::new();
    payload2.insert(
        "text".to_string(),
        Value::String("This is the second test data point".to_string()),
    );
    payload2.insert("source".to_string(), Value::String("test2.txt".to_string()));

    // Create a different mock embedding vector
    let vector2 = vec![0.2; 384]; // 384-dimensional vector with all values set to 0.2

    data_points.push(DataPoint {
        id: 1,
        payload: payload2,
        vector: vector2,
    });

    // Add the data points to the context
    let count = semantic_context.add_data_points(data_points).unwrap();

    // Verify the data points were added
    assert_eq!(count, 2);
    assert_eq!(semantic_context.get_data_points().len(), 2);

    // Test search functionality
    let query_vector = vec![0.15; 384]; // Query vector between the two data points
    let results = semantic_context.search(&query_vector, 2).unwrap();

    // Verify search results
    assert_eq!(results.len(), 2);

    // Save the context
    semantic_context.save().unwrap();

    // Load the context again to verify persistence
    let loaded_context = SemanticContext::new(data_path).unwrap();
    assert_eq!(loaded_context.get_data_points().len(), 2);

    // Clean up
    fs::remove_dir_all(temp_dir).unwrap_or(());
}
