//! Orchestrator that drives one ingest cycle: gather → normalize → write →
//! kick the Bedrock ingestion job → publish a manifest.
//!
//! Backend IO (S3 PutObject, Bedrock StartIngestionJob) lives behind the
//! [`CorpusWriter`] trait so the orchestration can be unit tested with an
//! in-memory fake.

use std::collections::BTreeMap;

use anyhow::Context;
use chrono::{
    DateTime,
    Utc,
};
use tracing::{
    info,
    warn,
};

use crate::manifest::Manifest;
use crate::normalize::{
    Batch,
    build_batches,
};
use crate::{
    Partition,
    Source,
};

/// Backend that knows how to push a batch to S3 and trigger the KB ingestion.
#[async_trait::async_trait]
pub trait CorpusWriter: Send + Sync {
    /// Persist a single batch to the configured corpus location.
    async fn put_batch(&self, batch: &Batch) -> anyhow::Result<()>;
    /// Persist the run manifest at the corpus root.
    async fn put_manifest(&self, manifest: &Manifest) -> anyhow::Result<()>;
    /// Kick a Bedrock KB ingestion job. Returns the job id.
    async fn start_ingestion(&self) -> anyhow::Result<String>;
}

/// Outcome of one ingest run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub run_at: DateTime<Utc>,
    pub counts: BTreeMap<Partition, usize>,
    pub batches_written: usize,
    pub ingestion_job_id: String,
}

impl RunSummary {
    pub fn total_chunks(&self) -> usize {
        self.counts.values().sum()
    }
}

/// Run every source once, write the JSONL batches, kick a KB ingestion job,
/// and return a summary suitable for logging or metric publication.
pub async fn run_once(
    sources: &[Box<dyn Source>],
    writer: &dyn CorpusWriter,
    run_at: DateTime<Utc>,
) -> anyhow::Result<RunSummary> {
    let mut counts: BTreeMap<Partition, usize> = BTreeMap::new();
    let mut total_batches = 0usize;

    for source in sources {
        let partition = source.partition();
        let chunks = source
            .fetch()
            .await
            .with_context(|| format!("source {} fetch failed", partition.as_str()))?;
        let batches = build_batches(partition, &chunks, run_at);
        let batch_count = batches.len();
        let chunk_count = chunks.len();
        info!(
            partition = partition.as_str(),
            chunk_count, batch_count, "fetched + normalized"
        );
        for batch in &batches {
            writer
                .put_batch(batch)
                .await
                .with_context(|| format!("put_batch {}", batch.key))?;
        }
        total_batches += batch_count;
        counts.insert(partition, chunk_count);
    }

    if total_batches == 0 {
        warn!("ingest produced zero batches; skipping StartIngestionJob");
        // Still publish a manifest so downstream tooling can see the run
        // happened. Bedrock skips re-indexing when the data source is empty.
        let manifest = Manifest::new(run_at, "skipped".to_string(), &counts);
        writer.put_manifest(&manifest).await.context("put_manifest")?;
        return Ok(RunSummary {
            run_at,
            counts,
            batches_written: 0,
            ingestion_job_id: "skipped".to_string(),
        });
    }

    let ingestion_job_id = writer.start_ingestion().await.context("start_ingestion")?;

    let manifest = Manifest::new(run_at, ingestion_job_id.clone(), &counts);
    writer.put_manifest(&manifest).await.context("put_manifest")?;

    Ok(RunSummary {
        run_at,
        counts,
        batches_written: total_batches,
        ingestion_job_id,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use chrono::TimeZone;

    use super::*;
    use crate::{
        RawChunk,
        StubSource,
    };

    #[derive(Default)]
    struct FakeWriter {
        batches: Mutex<Vec<Batch>>,
        manifest: Mutex<Option<Manifest>>,
        ingest_calls: Mutex<usize>,
        fail_ingest: bool,
    }

    impl FakeWriter {
        fn new() -> Self {
            Self::default()
        }

        fn fail_ingest() -> Self {
            Self {
                fail_ingest: true,
                ..Default::default()
            }
        }
    }

    #[async_trait::async_trait]
    impl CorpusWriter for FakeWriter {
        async fn put_batch(&self, batch: &Batch) -> anyhow::Result<()> {
            self.batches.lock().unwrap().push(batch.clone());
            Ok(())
        }

        async fn put_manifest(&self, manifest: &Manifest) -> anyhow::Result<()> {
            *self.manifest.lock().unwrap() = Some(manifest.clone());
            Ok(())
        }

        async fn start_ingestion(&self) -> anyhow::Result<String> {
            *self.ingest_calls.lock().unwrap() += 1;
            if self.fail_ingest {
                anyhow::bail!("simulated ConflictException");
            }
            Ok("JOB-1".to_string())
        }
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 19, 17, 0, 0).unwrap()
    }

    fn doc(path: &str) -> RawChunk {
        RawChunk {
            source_path: path.to_string(),
            content: "x".to_string(),
            last_modified: ts(),
            partition: Partition::Docs,
        }
    }

    #[tokio::test]
    async fn run_once_writes_batches_and_kicks_ingestion() {
        let writer = FakeWriter::new();
        let sources: Vec<Box<dyn Source>> = vec![
            Box::new(StubSource::new(Partition::Docs, vec![
                doc("docs/a.md"),
                doc("docs/b.md"),
            ])),
            Box::new(StubSource::new(Partition::Issues, vec![])),
        ];

        let summary = run_once(&sources, &writer, ts()).await.unwrap();
        assert_eq!(summary.batches_written, 1);
        assert_eq!(summary.total_chunks(), 2);
        assert_eq!(summary.ingestion_job_id, "JOB-1");

        let batches = writer.batches.lock().unwrap();
        assert_eq!(batches.len(), 1);
        assert!(batches[0].key.starts_with("docs/"));

        let manifest = writer.manifest.lock().unwrap().clone().unwrap();
        assert_eq!(manifest.last_ingestion_job_id, "JOB-1");
        assert_eq!(*manifest.counts.get("docs").unwrap(), 2);
        assert_eq!(*manifest.counts.get("issues").unwrap(), 0);

        assert_eq!(*writer.ingest_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn run_once_with_zero_batches_skips_ingestion() {
        let writer = FakeWriter::new();
        let sources: Vec<Box<dyn Source>> = vec![
            Box::new(StubSource::new(Partition::Docs, vec![])),
            Box::new(StubSource::new(Partition::Issues, vec![])),
            Box::new(StubSource::new(Partition::Releases, vec![])),
        ];

        let summary = run_once(&sources, &writer, ts()).await.unwrap();
        assert_eq!(summary.batches_written, 0);
        assert_eq!(summary.ingestion_job_id, "skipped");
        // Still publishes a manifest so observers can see "we ran, nothing changed".
        assert!(writer.manifest.lock().unwrap().is_some());
        assert_eq!(*writer.ingest_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn run_once_propagates_ingest_failure() {
        let writer = FakeWriter::fail_ingest();
        let sources: Vec<Box<dyn Source>> = vec![Box::new(StubSource::new(Partition::Docs, vec![doc("docs/a.md")]))];

        let err = run_once(&sources, &writer, ts()).await.unwrap_err();
        assert!(err.to_string().contains("start_ingestion"));
        // Batches should still have landed before the ingest call.
        assert_eq!(writer.batches.lock().unwrap().len(), 1);
        // Manifest should NOT publish on failure (the manifest claims an
        // ingestion job we don't have).
        assert!(writer.manifest.lock().unwrap().is_none());
    }
}
