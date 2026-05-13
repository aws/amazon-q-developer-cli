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

/// Pluggable retriever backend. Production uses Bedrock; tests use a stub.
#[async_trait::async_trait]
pub trait Retriever: Send + Sync {
    async fn retrieve(&self, input: &SearchInput) -> anyhow::Result<Vec<RetrievedChunk>>;
}

/// Render retrieved chunks into the agent-facing string format.
pub fn format_chunks(chunks: &[RetrievedChunk]) -> String {
    if chunks.is_empty() {
        return "No relevant results found.".to_string();
    }
    let mut out = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        out.push_str(&format!(
            "[{n}] {path} (relevance: {rel:.2})\n    {content}\n\n",
            n = i + 1,
            path = chunk.source_path,
            rel = chunk.relevance,
            content = chunk.content,
        ));
    }
    out.trim_end().to_string()
}

/// In-memory retriever used by tests and the `--stub` CLI mode.
pub struct StubRetriever {
    chunks: Vec<RetrievedChunk>,
}

impl StubRetriever {
    pub fn new(chunks: Vec<RetrievedChunk>) -> Self {
        Self { chunks }
    }
}

#[async_trait::async_trait]
impl Retriever for StubRetriever {
    async fn retrieve(&self, input: &SearchInput) -> anyhow::Result<Vec<RetrievedChunk>> {
        Ok(self.chunks.iter().take(input.max_results as usize).cloned().collect())
    }
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

    #[test]
    fn format_chunks_renders_numbered_chunks() {
        let chunks = vec![
            RetrievedChunk {
                source_path: "docs/auth.md".into(),
                content: "Run kiro-cli login.".into(),
                relevance: 0.9123,
            },
            RetrievedChunk {
                source_path: "github_issue:kiro-team/kiro-cli#42".into(),
                content: "Login hangs on Linux.".into(),
                relevance: 0.7000,
            },
        ];
        let rendered = format_chunks(&chunks);
        assert!(rendered.contains("[1] docs/auth.md (relevance: 0.91)"));
        assert!(rendered.contains("Run kiro-cli login."));
        assert!(rendered.contains("[2] github_issue:kiro-team/kiro-cli#42 (relevance: 0.70)"));
        assert!(rendered.contains("Login hangs on Linux."));
    }

    #[test]
    fn format_chunks_handles_empty_input() {
        let rendered = format_chunks(&[]);
        assert_eq!(rendered, "No relevant results found.");
    }

    #[tokio::test]
    async fn stub_retriever_returns_canned_chunks() {
        let stub = StubRetriever::new(vec![RetrievedChunk {
            source_path: "docs/x.md".into(),
            content: "stub".into(),
            relevance: 0.5,
        }]);
        let input = SearchInput {
            query: "anything".into(),
            source_filter: SourceFilter::All,
            max_results: 5,
        };
        let chunks = stub.retrieve(&input).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source_path, "docs/x.md");
    }

    #[tokio::test]
    async fn stub_retriever_respects_max_results() {
        let stub = StubRetriever::new(vec![
            RetrievedChunk {
                source_path: "a".into(),
                content: "a".into(),
                relevance: 1.0,
            },
            RetrievedChunk {
                source_path: "b".into(),
                content: "b".into(),
                relevance: 0.9,
            },
            RetrievedChunk {
                source_path: "c".into(),
                content: "c".into(),
                relevance: 0.8,
            },
        ]);
        let input = SearchInput {
            query: "anything".into(),
            source_filter: SourceFilter::All,
            max_results: 2,
        };
        let chunks = stub.retrieve(&input).await.unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].source_path, "a");
        assert_eq!(chunks[1].source_path, "b");
    }
}
