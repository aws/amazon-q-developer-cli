//! Entry point for the kiro-help corpus ingest Lambda.
//!
//! This binary is invoked by EventBridge once an hour. It runs all configured
//! `Source` impls in parallel, writes JSONL batches to the corpus S3 bucket,
//! then triggers a Bedrock KB ingestion job.
//!
//! Production wiring (live AWS clients, GitHub API, git clone) is intentionally
//! kept thin and is exercised only against real AWS — unit-tested logic lives
//! in [`kiro_help_corpus_ingest`] (the lib).

use anyhow::Context;
use kiro_help_corpus_ingest::{
    Partition,
    Source,
    StubSource,
    normalize::build_batches,
};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::try_init().ok();

    // For Phase 3 v0 we register only stubs; live `Source` implementations
    // (git/github/release) land in follow-up commits and are wired here.
    let sources: Vec<Box<dyn Source>> = vec![
        Box::new(StubSource::new(Partition::Docs, vec![])),
        Box::new(StubSource::new(Partition::Issues, vec![])),
        Box::new(StubSource::new(Partition::Releases, vec![])),
    ];

    let run_at = chrono::Utc::now();
    let mut total_chunks = 0usize;
    for s in &sources {
        let chunks = s.fetch().await.context("source fetch failed")?;
        total_chunks += chunks.len();
        let _batches = build_batches(s.partition(), &chunks, run_at);
        // TODO(phase-3): write batches to S3, then call StartIngestionJob.
    }

    info!(total_chunks, "ingest run complete (no live sources yet)");
    Ok(())
}
