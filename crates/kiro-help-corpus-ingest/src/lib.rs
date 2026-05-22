//! Core types for the kiro-help corpus ingest Lambda.
//!
//! Production sources (git checkout, GitHub API, S3 writer) live in their own
//! modules. The trait surface here is testable without any AWS or GitHub
//! integration — fakes plug in via [`Source`] and [`runner::CorpusWriter`].

pub mod aws_writer;
pub mod git_source;
pub mod github_http;
pub mod github_source;
pub mod manifest;
pub mod normalize;
pub mod runner;

use serde::{
    Deserialize,
    Serialize,
};

/// Which corpus partition a chunk belongs to. Drives S3 key prefixes and the
/// `source` field on the JSONL record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Partition {
    Docs,
    Issues,
    Releases,
}

impl Partition {
    pub const ALL: [Partition; 3] = [Self::Docs, Self::Issues, Self::Releases];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Docs => "docs",
            Self::Issues => "issues",
            Self::Releases => "releases",
        }
    }
}

/// A single raw document chunk, pre-normalization. Producers (git/github/release
/// sources) emit these; the normalizer batches them into JSONL and writes them
/// to S3 under `<partition>/<run_iso>/batch-N.jsonl`.
#[derive(Debug, Clone, PartialEq)]
pub struct RawChunk {
    pub source_path: String,
    pub content: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub partition: Partition,
}

/// A pluggable provider of `RawChunk`s from a single corpus partition.
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn partition(&self) -> Partition;
    async fn fetch(&self) -> anyhow::Result<Vec<RawChunk>>;
}

/// In-memory `Source` for use in tests and the `--stub` runtime mode.
pub struct StubSource {
    partition: Partition,
    chunks: Vec<RawChunk>,
}

impl StubSource {
    pub fn new(partition: Partition, chunks: Vec<RawChunk>) -> Self {
        Self { partition, chunks }
    }
}

#[async_trait::async_trait]
impl Source for StubSource {
    fn partition(&self) -> Partition {
        self.partition
    }

    async fn fetch(&self) -> anyhow::Result<Vec<RawChunk>> {
        Ok(self.chunks.clone())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn partition_round_trips_serde() {
        let json = serde_json::to_string(&Partition::Docs).unwrap();
        assert_eq!(json, "\"docs\"");
        let back: Partition = serde_json::from_str("\"issues\"").unwrap();
        assert_eq!(back, Partition::Issues);
    }

    #[test]
    fn partition_all_covers_every_variant() {
        let count = Partition::ALL.iter().count();
        assert_eq!(count, 3, "Partition::ALL must list every variant");
    }

    #[tokio::test]
    async fn stub_source_returns_chunks() {
        let chunk = RawChunk {
            source_path: "docs/foo.md".to_string(),
            content: "hello".to_string(),
            last_modified: chrono::Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0).unwrap(),
            partition: Partition::Docs,
        };
        let s = StubSource::new(Partition::Docs, vec![chunk.clone()]);
        assert_eq!(s.partition(), Partition::Docs);
        assert_eq!(s.fetch().await.unwrap(), vec![chunk]);
    }
}
