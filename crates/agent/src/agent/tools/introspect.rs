use std::collections::HashMap;
use std::io::Read;
use std::sync::LazyLock;

use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::Mutex;

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};

const INTROSPECT_DESCRIPTION: &str = r#"
Look up documentation about this chat application's own features, slash commands, settings, or capabilities.

WHEN TO USE:
- User asks about this assistant's features, commands, or settings
- User wants to know what slash commands are available
- User asks how to use a specific feature of this chat application

WHEN NOT TO USE:
- General coding questions, AWS help, or tasks the user wants you to perform
- Questions unrelated to this chat application itself

HOW TO USE:
- Provide a query to search the documentation
- Or provide a doc_path to retrieve a specific document
- When mentioning commands in your response, always prefix them with '/' (e.g., '/save', '/load', '/context')
- CRITICAL: Only provide information explicitly documented. If details are not documented, clearly state the information is not available rather than generating assumptions.
"#;

const INTROSPECT_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "query": {
            "type": "string",
            "description": "The user's question about this assistant's usage, features, or capabilities"
        },
        "doc_path": {
            "type": "string",
            "description": "Path to a specific doc to retrieve (e.g., \"features/tangent-mode.md\"). Use this to get full content of a doc from the index."
        }
    },
    "required": []
}
"#;

// Embed pre-built search index at compile time
const DOC_SEARCH_INDEX_GZ: &[u8] = include_bytes!("../../../../../autodocs/meta/doc-search-index.tar.gz");

// Embed doc index for progressive loading fallback
const DOC_INDEX_JSON: &str = include_str!("../../../../../autodocs/meta/doc-index.json");

// Lazy-initialized semantic search client
static DOC_SEARCH_CLIENT: LazyLock<Mutex<Option<std::sync::Arc<semantic_search_client::AsyncSemanticSearchClient>>>> =
    LazyLock::new(|| Mutex::new(None));

// Cache for parsed doc index (path -> content)
static DOC_CONTENT_CACHE: LazyLock<std::sync::Mutex<Option<HashMap<String, String>>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Introspect {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub doc_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct IntrospectResponse {
    documentation: Option<String>,
    query_context: Option<String>,
}

impl BuiltInToolTrait for Introspect {
    fn name() -> BuiltInToolName {
        BuiltInToolName::Introspect
    }

    fn description() -> std::borrow::Cow<'static, str> {
        INTROSPECT_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        INTROSPECT_SCHEMA.into()
    }
}

impl Introspect {
    pub async fn execute(&self) -> ToolExecutionResult {
        // If doc_path is provided, return that specific doc
        if let Some(path) = &self.doc_path {
            let documentation = Self::get_doc_by_path(path).map_err(|e| ToolExecutionError::Custom(e.to_string()))?;
            let response = IntrospectResponse {
                documentation: Some(documentation),
                query_context: None,
            };
            return Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
                serde_json::to_value(&response).map_err(|e| ToolExecutionError::Custom(e.to_string()))?,
            )]));
        }

        let documentation = if let Some(query) = &self.query {
            match self.get_relevant_docs(query).await {
                Ok(docs) => docs,
                Err(e) => {
                    tracing::warn!("Semantic search failed: {e}, using fallback");
                    Self::get_all_docs()
                },
            }
        } else {
            Self::get_all_docs()
        };

        let response = IntrospectResponse {
            documentation: Some(documentation),
            query_context: self.query.clone(),
        };

        Ok(ToolExecutionOutput::new(vec![ToolExecutionOutputItem::Json(
            serde_json::to_value(&response).map_err(|e| ToolExecutionError::Custom(e.to_string()))?,
        )]))
    }

    /// Get or initialize doc search client
    async fn get_doc_search_client()
    -> Result<std::sync::Arc<semantic_search_client::AsyncSemanticSearchClient>, eyre::Report> {
        let mut client_guard = DOC_SEARCH_CLIENT.lock().await;

        if let Some(client) = client_guard.as_ref() {
            return Ok(client.clone());
        }

        // Parse embedded index in memory
        let (contexts, semantic_data, bm25_data) = Self::parse_embedded_index()?;

        let config = semantic_search_client::config::SemanticSearchConfig {
            chunk_size: 100000,
            chunk_overlap: 0,
            ..Default::default()
        };

        // Create client from in-memory data (no disk I/O)
        let client = semantic_search_client::AsyncSemanticSearchClient::from_embedded_data(
            contexts,
            semantic_data,
            bm25_data,
            config,
        )
        .await?;
        let client_arc = std::sync::Arc::new(client);
        *client_guard = Some(client_arc.clone());

        Ok(client_arc)
    }

    /// Parse embedded search index in memory
    #[allow(clippy::type_complexity)]
    fn parse_embedded_index() -> Result<
        (
            HashMap<String, semantic_search_client::types::KnowledgeContext>,
            HashMap<String, Vec<semantic_search_client::types::DataPoint>>,
            HashMap<String, Vec<semantic_search_client::types::BM25DataPoint>>,
        ),
        eyre::Report,
    > {
        // Decompress tar in memory
        let mut decoder = flate2::read::GzDecoder::new(DOC_SEARCH_INDEX_GZ);
        let mut tar_data = Vec::new();
        decoder.read_to_end(&mut tar_data)?;

        // Parse tar and extract files in memory
        let mut archive = tar::Archive::new(&tar_data[..]);
        let mut files = HashMap::new();

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().to_string();

            if entry.header().entry_type().is_dir() || path.contains("/.") {
                continue;
            }

            // Normalize path (remove ./ prefix)
            let normalized_path = path.trim_start_matches("./").to_string();
            let mut content_bytes = Vec::new();
            entry.read_to_end(&mut content_bytes)?;

            // Only include valid UTF-8 files (skip binary/corrupted files)
            if let Ok(content) = String::from_utf8(content_bytes) {
                files.insert(normalized_path, content);
            }
        }

        // Parse contexts.json
        let contexts_json = files
            .get("contexts.json")
            .ok_or_else(|| eyre::eyre!("contexts.json not found in embedded index"))?;
        let contexts: HashMap<String, semantic_search_client::types::KnowledgeContext> =
            serde_json::from_str(contexts_json)?;

        // Parse data.json for each context
        let mut semantic_data = HashMap::new();
        let mut bm25_data = HashMap::new();

        for (context_id, context_meta) in contexts.iter() {
            // Check if it's BM25 or semantic
            if context_meta.embedding_type.is_bm25() {
                let bm25_path = format!("{}/data.bm25.json", context_id);
                if let Some(bm25_json) = files.get(&bm25_path) {
                    // Parse BM25 data directly (no vector conversion needed)
                    let data: Vec<semantic_search_client::types::BM25DataPoint> =
                        serde_json::from_str(bm25_json).map_err(|e| eyre::eyre!("Failed to parse BM25 data: {}", e))?;
                    bm25_data.insert(context_id.clone(), data);
                }
            } else {
                // Semantic data
                let data_path = format!("{}/data.json", context_id);
                if let Some(data_json) = files.get(&data_path) {
                    let data: Vec<semantic_search_client::types::DataPoint> = serde_json::from_str(data_json)?;
                    semantic_data.insert(context_id.clone(), data);
                }
            }
        }

        Ok((contexts, semantic_data, bm25_data))
    }

    /// Extract document path from search result
    fn get_doc_path(result: &semantic_search_client::types::SearchResult) -> &str {
        result
            .point
            .payload
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    }

    /// Combine search results using Reciprocal Rank Fusion (RRF)
    fn rrf_combine(
        semantic_results: Vec<semantic_search_client::types::SearchResult>,
        bm25_results: Vec<semantic_search_client::types::SearchResult>,
        top_k: usize,
    ) -> Vec<semantic_search_client::types::SearchResult> {
        const RRF_K: f32 = 60.0; // RRF constant

        let mut scores: HashMap<usize, (f32, semantic_search_client::types::SearchResult)> = HashMap::new();

        // Score semantic results
        for (rank, result) in semantic_results.into_iter().enumerate() {
            let doc_id = result.point.id;
            let rrf_score = 1.0 / (RRF_K + (rank as f32) + 1.0);
            scores.insert(doc_id, (rrf_score, result));
        }

        // Add BM25 results
        for (rank, result) in bm25_results.into_iter().enumerate() {
            let doc_id = result.point.id;
            let rrf_score = 1.0 / (RRF_K + (rank as f32) + 1.0);
            scores
                .entry(doc_id)
                .and_modify(|(score, _)| *score += rrf_score)
                .or_insert((rrf_score, result));
        }

        let mut combined: Vec<_> = scores.into_values().collect();
        combined.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Update distance field with RRF score for display
        combined
            .into_iter()
            .take(top_k)
            .map(|(rrf_score, mut result)| {
                result.distance = rrf_score;
                result
            })
            .collect()
    }

    /// Get relevant docs using semantic search
    async fn get_relevant_docs(&self, query: &str) -> Result<String, eyre::Report> {
        let client = Self::get_doc_search_client().await?;

        let contexts = client.get_contexts().await;
        let semantic_ctx = contexts.iter().find(|c| c.name == "kiro-autodocs-semantic");
        let bm25_ctx = contexts.iter().find(|c| c.name == "kiro-autodocs-bm25");

        // Fallback to old single index if hybrid not available
        let legacy_ctx = contexts.iter().find(|c| c.name == "kiro-autodocs");

        let results = if let (Some(sem), Some(bm25)) = (semantic_ctx, bm25_ctx) {
            const FETCH_SIZE: usize = 10;
            const RESULT_SIZE: usize = 5;

            // Hybrid search: fetch FETCH_SIZE from each, RRF combine, return RESULT_SIZE
            let sem_results = client
                .search_context(&sem.id, query, Some(FETCH_SIZE))
                .await
                .map_err(|e| eyre::eyre!("Semantic search failed: {}", e))?;
            let bm25_results = client
                .search_context(&bm25.id, query, Some(FETCH_SIZE))
                .await
                .map_err(|e| eyre::eyre!("BM25 search failed: {}", e))?;

            tracing::debug!("=== SEMANTIC RESULTS (top {}) ===", FETCH_SIZE);
            for (i, result) in sem_results.iter().enumerate() {
                tracing::debug!(
                    "  {}. {} (distance: {:.4})",
                    i + 1,
                    Self::get_doc_path(result),
                    result.distance
                );
            }

            tracing::debug!("=== BM25 RESULTS (top {}) ===", FETCH_SIZE);
            for (i, result) in bm25_results.iter().enumerate() {
                tracing::debug!(
                    "  {}. {} (score: {:.4})",
                    i + 1,
                    Self::get_doc_path(result),
                    result.distance
                );
            }

            Self::rrf_combine(sem_results, bm25_results, RESULT_SIZE)
        } else if let Some(ctx) = legacy_ctx.or(semantic_ctx) {
            tracing::debug!("Fallback to single index: {}", ctx.name);
            client.search_context(&ctx.id, query, Some(5)).await?
        } else {
            return Err(eyre::eyre!("No autodocs context found in embedded index"));
        };

        let mut documentation = String::new();
        documentation.push_str(&format!("\n\n--- Documentation for: {} ---\n", query));
        documentation.push_str(
            "The following documentation is provided inline. DO NOT attempt to read files - all content is below.\n\n",
        );

        tracing::debug!("=== INTROSPECT: Sending {} docs to LLM ===", results.len());

        // Load matching docs
        for (i, result) in results.iter().enumerate() {
            if let Some(text) = result.text() {
                tracing::debug!(
                    "  {}. {} (RRF score: {:.4})",
                    i + 1,
                    Self::get_doc_path(result),
                    result.distance
                );

                documentation.push_str(&format!("\n\n--- Document {} ---\n", i + 1));
                documentation.push_str(text);
            }
        }
        tracing::debug!("=== END INTROSPECT ===");

        documentation.push_str(&Self::get_critical_instruction());
        Ok(documentation)
    }

    /// Get a specific doc by path from the embedded index (cached)
    fn extract_docs_from_payload(payload: &HashMap<String, serde_json::Value>) -> Option<(String, String)> {
        let doc_path = payload.get("path").and_then(|v| v.as_str())?;
        let text = payload.get("text").and_then(|v| v.as_str())?;
        Some((doc_path.to_string(), text.to_string()))
    }

    // Process semantic data
    fn get_doc_by_path(path: &str) -> Result<String, eyre::Report> {
        let mut cache = DOC_CONTENT_CACHE.lock().unwrap();

        if cache.is_none() {
            let (_contexts, semantic_data, bm25_data) = Self::parse_embedded_index()?;
            let mut doc_map = HashMap::new();

            // Process BM25 data
            for data_points in semantic_data.values() {
                for point in data_points {
                    if let Some((doc_path, text)) = Self::extract_docs_from_payload(&point.payload) {
                        doc_map.insert(doc_path, text);
                    }
                }
            }
            for data_points in bm25_data.values() {
                for point in data_points {
                    if let Some((doc_path, text)) = Self::extract_docs_from_payload(&point.payload) {
                        doc_map.insert(doc_path, text);
                    }
                }
            }
            *cache = Some(doc_map);
        }

        let doc_map = cache.as_ref().unwrap();
        doc_map
            .get(path)
            .or_else(|| doc_map.iter().find(|(k, _)| k.ends_with(path)).map(|(_, v)| v))
            .map(|text| format!("--- Documentation: {} ---\n\n{}", path, text))
            .ok_or_else(|| eyre::eyre!("Document not found: {}", path))
    }

    fn get_all_docs() -> String {
        let mut content = String::new();
        content.push_str("--- Available Documentation Index ---\n");
        content.push_str("Below is metadata for all available documentation.\n");
        content.push_str("To get full content of a specific doc, call introspect with doc_path parameter.\n");
        content.push_str("Example: {\"doc_path\": \"features/tangent-mode.md\"}\n\n");
        content.push_str(DOC_INDEX_JSON);
        content.push_str(&Self::get_critical_instruction());
        content
    }

    fn get_critical_instruction() -> String {
        "\n\n--- CRITICAL INSTRUCTION ---\nYOU MUST ONLY provide information that is explicitly documented in the sections above. If specific details about any tool, feature, or command are not documented, you MUST clearly state that the information is not available in the documentation. DO NOT generate plausible-sounding information or make assumptions about undocumented features.\n\n".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use semantic_search_client::types::{
        DataPoint,
        SearchResult,
    };

    use super::*;

    fn create_test_result(id: usize, path: &str, distance: f32) -> SearchResult {
        let mut payload = HashMap::new();
        payload.insert("path".to_string(), serde_json::json!(path));
        payload.insert("text".to_string(), serde_json::json!("test content"));

        SearchResult {
            point: DataPoint {
                id,
                payload,
                vector: vec![],
            },
            distance,
        }
    }

    #[test]
    fn test_rrf_combine_consensus_boosting() {
        let semantic = vec![
            create_test_result(1, "doc1.md", 0.5),
            create_test_result(2, "doc2.md", 0.6),
            create_test_result(3, "doc3.md", 0.7),
        ];

        let bm25 = vec![
            create_test_result(2, "doc2.md", 5.0),
            create_test_result(4, "doc4.md", 4.0),
            create_test_result(5, "doc5.md", 3.0),
        ];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        assert_eq!(Introspect::get_doc_path(&results[0]), "doc2.md");
        assert!((results[0].distance - 0.0325).abs() < 0.001);
    }

    #[test]
    fn test_rrf_combine_single_source() {
        let semantic = vec![create_test_result(1, "doc1.md", 0.5)];
        let bm25 = vec![create_test_result(2, "doc2.md", 5.0)];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        assert_eq!(results.len(), 2);
        assert!((results[0].distance - 0.0164).abs() < 0.001);
        assert!((results[1].distance - 0.0164).abs() < 0.001);
    }

    #[test]
    fn test_rrf_combine_respects_top_k() {
        let semantic = vec![
            create_test_result(1, "doc1.md", 0.1),
            create_test_result(2, "doc2.md", 0.2),
            create_test_result(3, "doc3.md", 0.3),
        ];

        let bm25 = vec![
            create_test_result(4, "doc4.md", 1.0),
            create_test_result(5, "doc5.md", 2.0),
        ];

        let results = Introspect::rrf_combine(semantic, bm25, 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_rrf_combine_empty_results() {
        let semantic = vec![];
        let bm25 = vec![create_test_result(1, "doc1.md", 1.0)];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        assert_eq!(results.len(), 1);
        assert_eq!(Introspect::get_doc_path(&results[0]), "doc1.md");
    }

    #[test]
    fn test_rrf_combine_rank_matters() {
        let semantic = vec![
            create_test_result(1, "doc1.md", 0.1),
            create_test_result(2, "doc2.md", 0.2),
        ];

        let bm25 = vec![];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        assert!(results[0].distance > results[1].distance);
        assert_eq!(Introspect::get_doc_path(&results[0]), "doc1.md");
    }

    #[test]
    fn test_get_doc_path() {
        let result = create_test_result(1, "features/agent-config.md", 0.5);
        assert_eq!(Introspect::get_doc_path(&result), "features/agent-config.md");
    }

    #[test]
    fn test_get_doc_path_missing() {
        let mut payload = HashMap::new();
        payload.insert("text".to_string(), serde_json::json!("content"));

        let result = SearchResult {
            point: DataPoint {
                id: 1,
                payload,
                vector: vec![],
            },
            distance: 0.5,
        };

        assert_eq!(Introspect::get_doc_path(&result), "unknown");
    }
}
