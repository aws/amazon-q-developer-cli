//! MCP stdio server backed by an Amazon Bedrock Knowledge Base.

use serde::{
    Deserialize,
    Serialize,
};

/// Source filter for narrowing a knowledge search to a single corpus partition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFilter {
    Docs,
    Issues,
    Releases,
    #[default]
    All,
}

/// Input arguments to the `search_kiro_knowledge` tool.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchInput {
    pub query: String,
    #[serde(default)]
    pub source_filter: SourceFilter,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_max_results() -> u32 {
    5
}

/// A chunk returned by the underlying retriever.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedChunk {
    pub source_path: String,
    pub content: String,
    pub relevance: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_input_defaults_apply() {
        let input: SearchInput = serde_json::from_value(serde_json::json!({
            "query": "how do i log in"
        }))
        .unwrap();
        assert_eq!(input.query, "how do i log in");
        assert_eq!(input.source_filter, SourceFilter::All);
        assert_eq!(input.max_results, 5);
    }

    #[test]
    fn search_input_accepts_explicit_source_filter() {
        let input: SearchInput = serde_json::from_value(serde_json::json!({
            "query": "release notes",
            "source_filter": "releases",
            "max_results": 10
        }))
        .unwrap();
        assert_eq!(input.source_filter, SourceFilter::Releases);
        assert_eq!(input.max_results, 10);
    }

    #[test]
    fn search_input_rejects_invalid_source_filter() {
        let result: Result<SearchInput, _> = serde_json::from_value(serde_json::json!({
            "query": "x",
            "source_filter": "bogus"
        }));
        assert!(result.is_err());
    }
}
