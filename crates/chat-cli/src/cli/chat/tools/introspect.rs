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
        let (contexts, context_data) = Self::parse_embedded_index()?;

        let config = SemanticSearchConfig {
            chunk_size: 100000,
            chunk_overlap: 0,
            ..Default::default()
        };

        // Create client from in-memory data (no disk I/O)
        let client = AsyncSemanticSearchClient::from_embedded_data(contexts, context_data, config).await?;
        let client_arc = Arc::new(client);
        *client_guard = Some(client_arc.clone());

        Ok(client_arc)
    }

    /// Parse embedded search index in memory
    #[allow(clippy::type_complexity)]
    fn parse_embedded_index() -> Result<(
        HashMap<String, semantic_search_client::types::KnowledgeContext>,
        HashMap<String, Vec<semantic_search_client::types::DataPoint>>,
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
        let mut context_data = HashMap::new();
        for context_id in contexts.keys() {
            let data_path = format!("{}/data.json", context_id);
            if let Some(data_json) = files.get(&data_path) {
                let data: Vec<semantic_search_client::types::DataPoint> = serde_json::from_str(data_json)?;
                context_data.insert(context_id.clone(), data);
            }
        }

        Ok((contexts, context_data))
    }

    /// Get relevant docs using semantic search
    async fn get_relevant_docs(&self, query: &str) -> Result<String> {
        let client = Self::get_doc_search_client().await?;

        let contexts = client.get_contexts().await;
        let autodocs_context = contexts
            .iter()
            .find(|c| c.name == "kiro-autodocs")
            .ok_or_else(|| eyre::eyre!("kiro-autodocs context not found in embedded index"))?;

        let results = client.search_context(&autodocs_context.id, query, Some(5)).await?;

        let mut documentation = String::new();
        documentation.push_str(&format!("\n\n--- Documentation for: {} ---\n", query));
        documentation.push_str(
            "The following documentation is provided inline. DO NOT attempt to read files - all content is below.\n\n",
        );

        // Load matching docs
        for (i, result) in results.iter().enumerate() {
            if let Some(text) = result.text() {
                documentation.push_str(&format!("\n\n--- Document {} ---\n", i + 1));
                documentation.push_str(text);
            }
        }

        // Add settings
        documentation.push_str(&Self::get_settings_info());

        Ok(documentation)
    }

    /// Get a specific doc by path from the embedded index (cached)
    fn get_doc_by_path(path: &str) -> Result<String> {
        let mut cache = DOC_CONTENT_CACHE.lock().unwrap();

        if cache.is_none() {
            let (_contexts, context_data) = Self::parse_embedded_index()?;
            let mut doc_map = HashMap::new();

            for data_points in context_data.values() {
                for point in data_points {
                    if let Some(doc_path) = point.payload.get("path").and_then(|v| v.as_str())
                        && let Some(text) = point.payload.get("text").and_then(|v| v.as_str())
                    {
                        doc_map.insert(doc_path.to_string(), text.to_string());
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
