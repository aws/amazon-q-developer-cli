use std::collections::HashMap;

use semantic_search_client::embedding::EmbeddingType;
use semantic_search_client::types::KnowledgeContext;
use tempfile::TempDir;

/// Test that client initializes quickly without loading contexts
#[tokio::test]
async fn test_client_init_does_not_load_contexts() {
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path();

    // Create a fake context metadata file (simulating existing KB)
    let context_id = "test_context_123";
    let context = KnowledgeContext {
        id: context_id.to_string(),
        name: "Test KB".to_string(),
        description: "Test".to_string(),
        source_path: Some(base_dir.to_string_lossy().to_string()),
        persistent: true,
        embedding_type: EmbeddingType::Fast,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        item_count: 10,
        include_patterns: vec![],
        exclude_patterns: vec![],
        auto_sync: false,
    };

    let contexts_file = base_dir.join("contexts.json");
    let mut contexts = HashMap::new();
    contexts.insert(context_id.to_string(), context);
    let json = serde_json::to_string(&contexts).unwrap();
    std::fs::write(&contexts_file, json).unwrap();

    // Create context data directory with BM25 data
    let context_dir = base_dir.join(context_id);
    std::fs::create_dir_all(&context_dir).unwrap();
    std::fs::write(context_dir.join("data.bm25.json"), "{}").unwrap();

    // Init client
    let client = semantic_search_client::AsyncSemanticSearchClient::with_config(
        base_dir,
        semantic_search_client::config::SemanticSearchConfig {
            embedding_type: EmbeddingType::Fast,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Verify NO contexts were loaded (lazy loading)
    let (bm25_count, semantic_count) = client.loaded_contexts_count().await;
    assert_eq!(bm25_count, 0, "BM25 contexts should NOT be loaded at init");
    assert_eq!(semantic_count, 0, "Semantic contexts should NOT be loaded at init");

    // But metadata should be available
    let all_contexts = client.get_contexts().await;
    assert_eq!(all_contexts.len(), 1);
    assert_eq!(all_contexts[0].name, "Test KB");
}

/// Test that get_contexts returns metadata without loading data
#[tokio::test]
async fn test_get_contexts_returns_metadata_only() {
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path();

    // Create multiple fake contexts
    let mut contexts = HashMap::new();
    for i in 0..3 {
        let context_id = format!("context_{}", i);
        let context = KnowledgeContext {
            id: context_id.clone(),
            name: format!("KB {}", i),
            description: "Test".to_string(),
            source_path: Some(base_dir.to_string_lossy().to_string()),
            persistent: true,
            embedding_type: EmbeddingType::Fast,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            item_count: 10,
            include_patterns: vec![],
            exclude_patterns: vec![],
            auto_sync: false,
        };
        contexts.insert(context_id, context);
    }

    let contexts_file = base_dir.join("contexts.json");
    let json = serde_json::to_string(&contexts).unwrap();
    std::fs::write(&contexts_file, json).unwrap();

    let client = semantic_search_client::AsyncSemanticSearchClient::with_config(
        base_dir,
        semantic_search_client::config::SemanticSearchConfig {
            embedding_type: EmbeddingType::Fast,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Should return all 3 contexts metadata
    let all_contexts = client.get_contexts().await;
    assert_eq!(all_contexts.len(), 3);
}
