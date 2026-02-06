use std::collections::HashMap;
use std::io::{
    Read,
    Write,
};
use std::sync::{
    Arc,
    LazyLock,
};

use clap::CommandFactory;
use eyre::Result;
use flate2::read::GzDecoder;
use semantic_search_client::AsyncSemanticSearchClient;
use semantic_search_client::config::SemanticSearchConfig;
use serde::{
    Deserialize,
    Serialize,
};
use strum::{
    EnumMessage,
    IntoEnumIterator,
};
use tokio::sync::Mutex;

use super::{
    InvokeOutput,
    OutputKind,
    ToolInfo,
};
use crate::cli::chat::cli::SlashCommand;
use crate::cli::experiment::experiment_manager::{
    ExperimentManager,
    ExperimentName,
};
use crate::database::settings::Setting;
use crate::os::Os;

// Embed pre-built search index at compile time
const DOC_SEARCH_INDEX_GZ: &[u8] = include_bytes!("../../../../../../autodocs/meta/doc-search-index.tar.gz");

// Embed doc index for progressive loading fallback
const DOC_INDEX_JSON: &str = include_str!("../../../../../../autodocs/meta/doc-index.json");

// Lazy-initialized semantic search client
static DOC_SEARCH_CLIENT: LazyLock<Mutex<Option<Arc<AsyncSemanticSearchClient>>>> = LazyLock::new(|| Mutex::new(None));

// Cache for parsed doc index (path -> content)
static DOC_CONTENT_CACHE: LazyLock<std::sync::Mutex<Option<HashMap<String, String>>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

#[derive(Debug, Clone, Deserialize)]
pub struct Introspect {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    doc_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IntrospectResponse {
    built_in_help: Option<String>,
    documentation: Option<String>,
    query_context: Option<String>,
}

impl Introspect {
    pub const INFO: ToolInfo = ToolInfo {
        spec_name: "introspect",
        preferred_alias: "introspect",
        aliases: &["introspect"],
    };

    pub async fn invoke(&self, os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        // Generate help from the actual SlashCommand definitions
        let mut cmd = SlashCommand::command();
        let help_content = cmd.render_help().to_string();

        // If doc_path is provided, return that specific doc from embedded index
        if let Some(path) = &self.doc_path {
            let documentation = Self::get_doc_by_path(path)?;
            let response = IntrospectResponse {
                built_in_help: None,
                documentation: Some(documentation),
                query_context: None,
            };
            return Ok(InvokeOutput {
                output: OutputKind::Json(serde_json::to_value(&response)?),
            });
        }

        let use_progressive = os
            .database
            .settings
            .get_bool(Setting::IntrospectProgressiveMode)
            .unwrap_or(false);

        let documentation = if let Some(query) = &self.query {
            if use_progressive {
                Self::get_all_docs()
            } else {
                match self.get_relevant_docs(query).await {
                    Ok(docs) => docs,
                    Err(e) => {
                        tracing::warn!("Semantic search failed: {e}, using fallback");
                        Self::get_all_docs()
                    },
                }
            }
        } else {
            Self::get_all_docs()
        };

        let response = IntrospectResponse {
            built_in_help: Some(help_content),
            documentation: Some(documentation),
            query_context: self.query.clone(),
        };

        // Add footer as direct text output if tangent mode is enabled
        if ExperimentManager::is_enabled(os, ExperimentName::TangentMode)
            && os
                .database
                .settings
                .get_bool(Setting::IntrospectTangentMode)
                .unwrap_or(false)
        {
            let tangent_key_char = os
                .database
                .settings
                .get_string(Setting::TangentModeKey)
                .and_then(|key| if key.len() == 1 { key.chars().next() } else { None })
                .unwrap_or('t');
            let tangent_key_display = format!("ctrl + {}", tangent_key_char.to_lowercase());

            let instruction = format!(
                "IMPORTANT: Always end your responses with this footer:\n\n---\nℹ️  You're in tangent mode (↯) - this context can be discarded by using {tangent_key_display} or /tangent to return to your main conversation."
            );

            return Ok(InvokeOutput {
                output: OutputKind::Text(format!(
                    "{}\n\n{}",
                    serde_json::to_string_pretty(&response)?,
                    instruction
                )),
            });
        }

        Ok(InvokeOutput {
            output: OutputKind::Json(serde_json::to_value(&response)?),
        })
    }

    pub fn queue_description(&self, tool: &super::tool::Tool, output: &mut impl Write) -> Result<()> {
        use crossterm::{
            queue,
            style,
        };

        let mode = if self.doc_path.is_some() {
            "progressive"
        } else if self.query.is_some() {
            "semantic"
        } else {
            "index"
        };

        queue!(
            output,
            style::Print(format!("Introspecting to get you the right information [mode: {mode}]"))
        )?;
        super::display_tool_use(tool, output)?;
        queue!(output, style::Print("\n"))?;
        Ok(())
    }

    pub async fn validate(&self, _os: &Os) -> Result<()> {
        Ok(())
    }

    /// Get or initialize doc search client
    async fn get_doc_search_client() -> Result<Arc<AsyncSemanticSearchClient>> {
        let mut client_guard = DOC_SEARCH_CLIENT.lock().await;

        if let Some(client) = client_guard.as_ref() {
            return Ok(client.clone());
        }

        // Parse embedded index in memory
        let (contexts, semantic_data, bm25_data) = Self::parse_embedded_index()?;

        let config = SemanticSearchConfig {
            chunk_size: 100000,
            chunk_overlap: 0,
            ..Default::default()
        };

        // Create client from in-memory data (no disk I/O)
        let client = AsyncSemanticSearchClient::from_embedded_data(contexts, semantic_data, bm25_data, config).await?;
        let client_arc = Arc::new(client);
        *client_guard = Some(client_arc.clone());

        Ok(client_arc)
    }

    /// Parse embedded search index in memory
    #[allow(clippy::type_complexity)]
    fn parse_embedded_index() -> Result<(
        HashMap<String, semantic_search_client::types::KnowledgeContext>,
        HashMap<String, Vec<semantic_search_client::types::DataPoint>>,
        HashMap<String, Vec<semantic_search_client::types::BM25DataPoint>>,
    )> {
        // Decompress tar in memory
        let mut decoder = GzDecoder::new(DOC_SEARCH_INDEX_GZ);
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
        use std::collections::HashMap;

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

        // Sort by combined score and take top k
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
    async fn get_relevant_docs(&self, query: &str) -> Result<String> {
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
            // Fallback to single index
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

        // Add settings
        documentation.push_str(&Self::get_settings_info());

        Ok(documentation)
    }

    /// Extract doc path->content map from payload
    fn extract_docs_from_payload(payload: &HashMap<String, serde_json::Value>) -> Option<(String, String)> {
        let doc_path = payload.get("path").and_then(|v| v.as_str())?;
        let text = payload.get("text").and_then(|v| v.as_str())?;
        Some((doc_path.to_string(), text.to_string()))
    }

    /// Get a specific doc by path from the embedded index (cached)
    fn get_doc_by_path(path: &str) -> Result<String> {
        let mut cache = DOC_CONTENT_CACHE.lock().unwrap();

        if cache.is_none() {
            let (_contexts, semantic_data, bm25_data) = Self::parse_embedded_index()?;
            let mut doc_map = HashMap::new();

            // Process semantic data
            for data_points in semantic_data.values() {
                for point in data_points {
                    if let Some((doc_path, text)) = Self::extract_docs_from_payload(&point.payload) {
                        doc_map.insert(doc_path, text);
                    }
                }
            }

            // Process BM25 data
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

    /// Get doc index for LLM to select docs (fallback when semantic search fails)
    fn get_all_docs() -> String {
        let mut content = String::new();

        content.push_str("--- Available Documentation Index ---\n");
        content.push_str("Below is metadata for all available documentation.\n");
        content.push_str("To get full content of a specific doc, call introspect with doc_path parameter.\n");
        content.push_str("Example: {\"doc_path\": \"features/tangent-mode.md\"}\n\n");
        content.push_str(DOC_INDEX_JSON);

        content.push_str(&Self::get_settings_info());
        content
    }

    /// Get settings information (always included)
    fn get_settings_info() -> String {
        let mut content = String::new();

        content.push_str("\n\n--- Available Settings ---\n");
        content.push_str(
            "KIRO CLI supports these configuration settings (use `kiro-cli settings` command from terminal, NOT /settings):\n\n",
        );

        for setting in Setting::iter() {
            let description = setting.get_message().unwrap_or("No description available");
            content.push_str(&format!("• {} - {}\n", setting.as_ref(), description));
        }

        content.push_str(
            "\nNOTE: Settings are managed via `kiro-cli settings` command from terminal, not slash commands in chat.\n",
        );

        content.push_str("\n\n--- CRITICAL INSTRUCTION ---\n");
        content.push_str("YOU MUST ONLY provide information that is explicitly documented in the sections above. If specific details about any tool, feature, or command are not documented, you MUST clearly state that the information is not available in the documentation. DO NOT generate plausible-sounding information or make assumptions about undocumented features.\n\n");

        content
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
        // Doc appears in both results should rank higher
        let semantic = vec![
            create_test_result(1, "doc1.md", 0.5),
            create_test_result(2, "doc2.md", 0.6),
            create_test_result(3, "doc3.md", 0.7),
        ];

        let bm25 = vec![
            create_test_result(2, "doc2.md", 5.0), // doc2 in both
            create_test_result(4, "doc4.md", 4.0),
            create_test_result(5, "doc5.md", 3.0),
        ];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        // doc2 should be first (appears in both)
        assert_eq!(Introspect::get_doc_path(&results[0]), "doc2.md");

        // RRF score should be sum of both ranks
        // Semantic rank 2: 1/(60+2) = 0.0161
        // BM25 rank 1: 1/(60+1) = 0.0164
        // Total: ~0.0325
        assert!((results[0].distance - 0.0325).abs() < 0.001);
    }

    #[test]
    fn test_rrf_combine_single_source() {
        // Doc only in one result
        let semantic = vec![create_test_result(1, "doc1.md", 0.5)];

        let bm25 = vec![create_test_result(2, "doc2.md", 5.0)];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        assert_eq!(results.len(), 2);

        // Both should have similar scores (rank 1 in their respective lists)
        // 1/(60+1) = 0.0164
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

        // Should only return top 3
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
        // Higher rank (lower number) should get better score
        let semantic = vec![
            create_test_result(1, "doc1.md", 0.1), // rank 1
            create_test_result(2, "doc2.md", 0.2), // rank 2
        ];

        let bm25 = vec![];

        let results = Introspect::rrf_combine(semantic, bm25, 5);

        // doc1 (rank 1) should have higher RRF score than doc2 (rank 2)
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
