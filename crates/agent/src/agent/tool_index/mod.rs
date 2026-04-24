use std::collections::{
    BTreeMap,
    HashSet,
};

use bm25::{
    Document,
    Language,
    SearchEngine,
    SearchEngineBuilder,
};
use convert_case::{
    Case,
    Casing,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::agent::agent_config::parse::CanonicalToolName;
use crate::agent::agent_loop::types::ToolSpec;
use crate::agent::util::truncate_safe_in_place;

const MAX_DESCRIPTION_BYTES: usize = 1024;

/// Composite key for tool lookup
fn composite_key(server: &str, tool: &str) -> String {
    format!("{server}::{tool}")
}

/// BM25-based tool index for fast keyword search (no ML model required)
#[derive(Debug)]
pub struct ToolIndex {
    /// The underlying BM25 search engine
    engine: SearchEngine<String>,
    /// Tool metadata indexed by composite key 'server_name::tool_name'
    tools: BTreeMap<String, ToolEntry>,
}

/// Entry in the tool index
#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub tool_name: String,
    pub server_name: String,
    pub description: String,
}

impl Default for ToolIndex {
    fn default() -> Self {
        Self {
            engine: SearchEngineBuilder::<String>::with_avgdl(50.0)
                .language_mode(Language::English)
                .k1(0.9)
                .b(0.4)
                .build(),
            tools: BTreeMap::new(),
        }
    }
}

impl ToolIndex {
    /// Create a new BM25 tool index from MCP tool specs
    pub fn from_tool_specs(mcp_server_tool_specs: &std::collections::HashMap<String, Vec<ToolSpec>>) -> Self {
        let mut index = Self::default();

        for (server_name, specs) in mcp_server_tool_specs {
            for spec in specs {
                let key = composite_key(server_name, &spec.name);
                let text = index_text(server_name, &spec.name, &spec.description, &spec.input_schema);
                index.engine.upsert(Document {
                    id: key.clone(),
                    contents: text,
                });

                let mut desc = spec.description.clone();
                truncate_safe_in_place(&mut desc, MAX_DESCRIPTION_BYTES, Some("..."));
                index.tools.insert(key, ToolEntry {
                    tool_name: spec.name.clone(),
                    server_name: server_name.clone(),
                    description: desc,
                });
            }
        }

        index
    }

    /// Search for tools matching the query
    pub fn search(&self, query: &str, limit: usize) -> Vec<ToolSearchResult> {
        self.engine
            .search(query, limit)
            .into_iter()
            .filter(|r| r.score > 0.0)
            .filter_map(|r| {
                self.tools.get(&r.document.id).map(|entry| ToolSearchResult {
                    tool_name: entry.tool_name.clone(),
                    server_name: entry.server_name.clone(),
                    description: entry.description.clone(),
                    score: r.score,
                })
            })
            .collect()
    }

    /// Get a tool entry by server_name and tool_name
    pub fn get_entry(&self, server_name: &str, tool_name: &str) -> Option<&ToolEntry> {
        self.tools.get(&composite_key(server_name, tool_name))
    }

    /// Get number of indexed tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Format all indexed tools as an XML-tagged list for system prompt injection.
    /// Returns empty string if no tools indexed.
    pub fn format_tool_list(&self) -> String {
        if self.tools.is_empty() {
            return String::new();
        }
        let mut out = String::from("<available-deferred-tools>\n");
        // BTreeMap iterates in sorted key order for deterministic output (enables prompt caching)
        for (key, entry) in &self.tools {
            if !entry.tool_name.is_empty() {
                out.push_str(&format!("- {}: {}\n", key, entry.description));
            }
        }
        out.push_str("</available-deferred-tools>");
        out
    }
}

/// Generate BM25 index text including args from input_schema
pub fn index_text(
    server_name: &str,
    tool_name: &str,
    description: &str,
    input_schema: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let split_name = tool_name.to_case(Case::Lower);
    let mut text = format!("{split_name} {server_name}::{tool_name} {description}");

    // Extract argument names and descriptions from JSON Schema properties
    if let Some(serde_json::Value::Object(props)) = input_schema.get("properties") {
        for (arg_name, arg_def) in props {
            text.push(' ');
            text.push_str(arg_name);
            if let Some(serde_json::Value::String(arg_desc)) = arg_def.get("description") {
                text.push(' ');
                text.push_str(arg_desc);
            }
        }
    }

    text
}

/// Search result returned by search (internal)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSearchResult {
    pub tool_name: String,
    pub server_name: String,
    pub description: String,
    pub score: f32,
}

/// Result of loading a tool
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolLoadResult {
    pub tool_name: String,
    pub server_name: String,
    pub description: String,
    pub score: f32,
}

/// Response from tool_search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSearchResponse {
    pub tools: Vec<ToolLoadResult>,
}

/// Configuration for tool loading thresholds
#[derive(Debug, Clone)]
pub struct ToolLoadConfig {
    /// Minimum BM25 score for a tool to be considered a match
    pub matching_threshold: f32,
}

impl Default for ToolLoadConfig {
    fn default() -> Self {
        Self {
            matching_threshold: 1.5,
        }
    }
}

impl ToolLoadConfig {
    /// Create ToolLoadConfig reading threshold from env var
    pub fn from_env() -> Self {
        let threshold = std::env::var("KIRO_CLI_TOOL_SEARCH_MATCHING_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.5);
        Self {
            matching_threshold: threshold,
        }
    }
}

/// Filter MCP tool specs to only include tools present in the allowed set.
/// This ensures the tool search index and deferred tools list only contain
/// tools the agent is actually permitted to use per its `tools` config.
pub fn filter_specs_by_allowed_tools(
    mcp_server_tool_specs: &std::collections::HashMap<String, Vec<ToolSpec>>,
    agent_tool_names: &HashSet<CanonicalToolName>,
) -> std::collections::HashMap<String, Vec<ToolSpec>> {
    mcp_server_tool_specs
        .iter()
        .filter_map(|(server, specs)| {
            let filtered: Vec<ToolSpec> = specs
                .iter()
                .filter(|s| {
                    agent_tool_names.contains(&CanonicalToolName::from_mcp_parts(server.clone(), s.name.clone()))
                })
                .cloned()
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some((server.clone(), filtered))
            }
        })
        .collect()
}

/// Determine whether tool search should be active for this turn.
///
/// Returns `false` if `tool_search_enabled` is off.
/// When enabled with no thresholds, always returns `true`.
/// When thresholds are set, returns `true` if the total MCP tool spec size
/// exceeds **either** threshold (OR logic).
pub fn should_activate_tool_search(
    settings: &crate::agent::types::AgentSettings,
    mcp_tool_spec_tokens: usize,
    context_window_size: Option<usize>,
) -> bool {
    if !settings.tool_search_enabled {
        return false;
    }
    let has_pct = settings.tool_search_min_pct.is_some();
    let has_tokens = settings.tool_search_min_tokens.is_some();
    if !has_pct && !has_tokens {
        return true;
    }
    if let Some(min_pct) = settings.tool_search_min_pct
        && let Some(ctx) = context_window_size
    {
        let pct = (mcp_tool_spec_tokens as f64 / ctx as f64) * 100.0;
        if pct > min_pct {
            return true;
        }
    }
    if let Some(min_tokens) = settings.tool_search_min_tokens
        && mcp_tool_spec_tokens as u64 > min_tokens
    {
        return true;
    }
    false
}

/// Filter tool names based on tool_search_enabled flag and activated tools set.
/// When disabled, returns all tools unchanged (backward compat).
/// When enabled, keeps all builtins and agents, but filters MCP tools to only activated ones.
pub fn filter_tool_names(
    tool_search_enabled: bool,
    all_tool_names: Vec<CanonicalToolName>,
    activated_tools: &HashSet<CanonicalToolName>,
) -> Vec<CanonicalToolName> {
    if !tool_search_enabled {
        return all_tool_names;
    }
    all_tool_names
        .into_iter()
        .filter(|name| match name {
            CanonicalToolName::BuiltIn(_) => true,
            CanonicalToolName::Agent { .. } => true,
            CanonicalToolName::Mcp { .. } => activated_tools.contains(name),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::agent::agent_loop::types::ToolSpec;

    fn make_tool_spec(name: &str, desc: &str) -> ToolSpec {
        ToolSpec {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: serde_json::Map::new(),
        }
    }

    #[test]
    fn bm25_from_tool_specs_builds_index() {
        let mut specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        specs.insert("server1".to_string(), vec![
            make_tool_spec("read_file", "Read contents of a file from disk"),
            make_tool_spec("write_file", "Write contents to a file on disk"),
        ]);
        specs.insert("server2".to_string(), vec![make_tool_spec(
            "fetch_url",
            "Fetch content from a URL",
        )]);

        let index = ToolIndex::from_tool_specs(&specs);

        assert_eq!(index.len(), 3);
        assert!(!index.is_empty());
    }

    #[test]
    fn bm25_search_returns_matching_tools() {
        let mut specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        specs.insert("filesystem".to_string(), vec![make_tool_spec(
            "read_file",
            "Read contents of a file from disk",
        )]);
        specs.insert("http".to_string(), vec![make_tool_spec(
            "fetch_url",
            "Fetch content from a URL",
        )]);

        let index = ToolIndex::from_tool_specs(&specs);
        let results = index.search("file", 5);

        assert!(!results.is_empty());
        assert_eq!(results[0].tool_name, "read_file");
        assert_eq!(results[0].server_name, "filesystem");
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn bm25_search_no_match_returns_empty() {
        let mut specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        specs.insert("server".to_string(), vec![make_tool_spec(
            "read_file",
            "Read contents of a file",
        )]);

        let index = ToolIndex::from_tool_specs(&specs);
        let results = index.search("xyzzy gibberish", 5);

        assert!(results.is_empty());
    }

    #[test]
    fn bm25_get_entry_finds_tool() {
        let mut specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        specs.insert("server".to_string(), vec![make_tool_spec(
            "my_tool",
            "A tool description",
        )]);

        let index = ToolIndex::from_tool_specs(&specs);
        let entry = index.get_entry("server", "my_tool");

        assert!(entry.is_some());
        let e = entry.unwrap();
        assert_eq!(e.tool_name, "my_tool");
        assert_eq!(e.server_name, "server");
    }

    #[test]
    fn bm25_empty_specs_creates_empty_index() {
        let specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        let index = ToolIndex::from_tool_specs(&specs);

        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn truncate_description_short_unchanged() {
        let mut s = "short".to_string();
        truncate_safe_in_place(&mut s, 100, Some("..."));
        assert_eq!(s, "short");
    }

    #[test]
    fn truncate_description_long_truncated() {
        let mut s = "a".repeat(3000);
        truncate_safe_in_place(&mut s, 2048, Some("..."));
        assert!(s.len() <= 2048);
        assert!(s.ends_with("..."));
    }

    #[test]
    fn filter_tool_names_disabled_returns_all() {
        use crate::agent::tools::BuiltInToolName;
        let all = vec![
            CanonicalToolName::BuiltIn(BuiltInToolName::FsRead),
            CanonicalToolName::from_mcp_parts("server".into(), "mcp_tool".into()),
        ];
        let activated = HashSet::new();
        let result = filter_tool_names(false, all.clone(), &activated);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn filter_tool_names_enabled_keeps_builtins_defers_mcp() {
        use crate::agent::tools::BuiltInToolName;
        let all = vec![
            CanonicalToolName::BuiltIn(BuiltInToolName::FsRead),
            CanonicalToolName::BuiltIn(BuiltInToolName::ToolSearch),
            CanonicalToolName::from_mcp_parts("server".into(), "mcp_tool".into()),
        ];
        let activated = HashSet::new();
        let result = filter_tool_names(true, all, &activated);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|n| n.tool_name() == "read"));
        assert!(result.iter().any(|n| n.tool_name() == "tool_search"));
    }

    #[test]
    fn filter_tool_names_enabled_keeps_activated_mcp() {
        use crate::agent::tools::BuiltInToolName;
        let all = vec![
            CanonicalToolName::BuiltIn(BuiltInToolName::FsRead),
            CanonicalToolName::from_mcp_parts("server".into(), "active_tool".into()),
            CanonicalToolName::from_mcp_parts("server".into(), "inactive_tool".into()),
        ];
        let mut activated = HashSet::new();
        activated.insert(CanonicalToolName::from_mcp_parts("server".into(), "active_tool".into()));
        let result = filter_tool_names(true, all, &activated);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|n| n.tool_name() == "active_tool"));
        assert!(!result.iter().any(|n| n.tool_name() == "inactive_tool"));
    }

    fn make_settings(
        enabled: bool,
        min_pct: Option<f64>,
        min_tokens: Option<u64>,
    ) -> crate::agent::types::AgentSettings {
        crate::agent::types::AgentSettings {
            tool_search_enabled: enabled,
            tool_search_min_pct: min_pct,
            tool_search_min_tokens: min_tokens,
            ..Default::default()
        }
    }

    #[test]
    fn should_activate_disabled_returns_false() {
        let s = make_settings(false, None, None);
        assert!(!should_activate_tool_search(&s, 100_000, Some(200_000)));
    }

    #[test]
    fn should_activate_enabled_no_thresholds_returns_true() {
        let s = make_settings(true, None, None);
        assert!(should_activate_tool_search(&s, 0, Some(200_000)));
    }

    #[test]
    fn should_activate_pct_below_threshold() {
        // 3% of 200K = 6000 tokens, threshold is 5%
        let s = make_settings(true, Some(5.0), None);
        assert!(!should_activate_tool_search(&s, 6_000, Some(200_000)));
    }

    #[test]
    fn should_activate_pct_above_threshold() {
        // 10% of 200K = 20000 tokens, threshold is 5%
        let s = make_settings(true, Some(5.0), None);
        assert!(should_activate_tool_search(&s, 20_000, Some(200_000)));
    }

    #[test]
    fn should_activate_tokens_below_threshold() {
        let s = make_settings(true, None, Some(50_000));
        assert!(!should_activate_tool_search(&s, 30_000, Some(200_000)));
    }

    #[test]
    fn should_activate_tokens_above_threshold() {
        let s = make_settings(true, None, Some(50_000));
        assert!(should_activate_tool_search(&s, 60_000, Some(200_000)));
    }

    #[test]
    fn should_activate_or_logic_one_met() {
        // pct not met (3%), but tokens met (60K > 50K)
        let s = make_settings(true, Some(5.0), Some(50_000));
        assert!(should_activate_tool_search(&s, 60_000, Some(2_000_000)));
    }

    #[test]
    fn should_activate_or_logic_neither_met() {
        let s = make_settings(true, Some(5.0), Some(50_000));
        assert!(!should_activate_tool_search(&s, 6_000, Some(200_000)));
    }

    #[test]
    fn should_activate_pct_no_context_window() {
        // pct threshold set but no context window — pct check skipped, tokens not set
        let s = make_settings(true, Some(5.0), None);
        assert!(!should_activate_tool_search(&s, 60_000, None));
    }

    #[test]
    fn filter_specs_excludes_tools_not_in_allowed_set() {
        // Server has 3 tools, but agent config only allows 1
        let mut specs: HashMap<String, Vec<ToolSpec>> = HashMap::new();
        specs.insert("myserver".to_string(), vec![
            make_tool_spec("allowed_tool", "An allowed tool"),
            make_tool_spec("blocked_tool", "A blocked tool"),
            make_tool_spec("another_blocked", "Another blocked tool"),
        ]);

        let mut allowed = HashSet::new();
        allowed.insert(CanonicalToolName::from_mcp_parts(
            "myserver".into(),
            "allowed_tool".into(),
        ));

        let filtered = filter_specs_by_allowed_tools(&specs, &allowed);
        let index = ToolIndex::from_tool_specs(&filtered);

        assert_eq!(index.len(), 1);
        assert!(index.get_entry("myserver", "allowed_tool").is_some());
        assert!(index.get_entry("myserver", "blocked_tool").is_none());
        assert!(index.get_entry("myserver", "another_blocked").is_none());
    }
}
