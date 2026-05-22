//! Normalize `RawChunk`s into JSONL batches keyed by partition + run timestamp.

use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};
use serde::Serialize;

use crate::{
    Partition,
    RawChunk,
};

/// Soft cap on the size of a single JSONL batch, in bytes. Bedrock KB has its
/// own per-file limits and very large objects also blow out Lambda memory; 1 MB
/// is comfortable on both axes.
const MAX_BATCH_BYTES: usize = 1_000_000;

/// One JSONL line, ready for upload.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CorpusRecord<'a> {
    pub path: &'a str,
    pub content: &'a str,
    pub last_modified: String,
    pub source: &'static str,
}

/// One JSONL batch, ready for upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// Full S3 object key, e.g. `docs/2026-05-19T00:00:00Z/batch-0.jsonl`.
    pub key: String,
    pub partition: Partition,
    pub body: Vec<u8>,
    pub record_count: usize,
}

/// Convert raw chunks for a single partition into one or more JSONL batches.
pub fn build_batches(partition: Partition, chunks: &[RawChunk], run_at: DateTime<Utc>) -> Vec<Batch> {
    let prefix = format!(
        "{}/{}",
        partition.as_str(),
        run_at.to_rfc3339_opts(SecondsFormat::Secs, true),
    );

    let mut batches = Vec::new();
    let mut current_body: Vec<u8> = Vec::new();
    let mut current_count = 0usize;
    let mut batch_index = 0usize;

    let push_batch = |batches: &mut Vec<Batch>, prefix: &str, body: Vec<u8>, count: usize, idx: usize| {
        batches.push(Batch {
            key: format!("{prefix}/batch-{idx}.jsonl"),
            partition,
            body,
            record_count: count,
        });
    };

    for chunk in chunks {
        if chunk.partition != partition {
            continue;
        }
        let record = CorpusRecord {
            path: &chunk.source_path,
            content: &chunk.content,
            last_modified: chunk.last_modified.to_rfc3339_opts(SecondsFormat::Secs, true),
            source: partition.as_str(),
        };
        let mut line = serde_json::to_vec(&record).expect("serialize CorpusRecord");
        line.push(b'\n');

        if !current_body.is_empty() && current_body.len() + line.len() > MAX_BATCH_BYTES {
            push_batch(
                &mut batches,
                &prefix,
                std::mem::take(&mut current_body),
                std::mem::take(&mut current_count),
                batch_index,
            );
            batch_index += 1;
        }

        current_body.extend_from_slice(&line);
        current_count += 1;
    }

    if !current_body.is_empty() {
        push_batch(&mut batches, &prefix, current_body, current_count, batch_index);
    }

    batches
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 19, 17, 30, 0).unwrap()
    }

    fn doc(path: &str, content: &str) -> RawChunk {
        RawChunk {
            source_path: path.to_string(),
            content: content.to_string(),
            last_modified: ts(),
            partition: Partition::Docs,
        }
    }

    #[test]
    fn empty_input_produces_no_batches() {
        let batches = build_batches(Partition::Docs, &[], ts());
        assert!(batches.is_empty(), "expected no batches for empty input");
    }

    #[test]
    fn single_chunk_produces_one_batch_with_iso_keyed_prefix() {
        let chunks = vec![doc("docs/auth.md", "Run kiro-cli login.")];
        let batches = build_batches(Partition::Docs, &chunks, ts());

        assert_eq!(batches.len(), 1);
        let b = &batches[0];
        assert_eq!(b.key, "docs/2026-05-19T17:30:00Z/batch-0.jsonl");
        assert_eq!(b.partition, Partition::Docs);
        assert_eq!(b.record_count, 1);

        let s = std::str::from_utf8(&b.body).unwrap();
        assert!(s.ends_with('\n'), "JSONL records must end with newline");
        let record: serde_json::Value = serde_json::from_str(s.trim_end()).unwrap();
        assert_eq!(record["path"], "docs/auth.md");
        assert_eq!(record["content"], "Run kiro-cli login.");
        assert_eq!(record["source"], "docs");
        assert_eq!(record["last_modified"], "2026-05-19T17:30:00Z");
    }

    #[test]
    fn ignores_chunks_from_other_partitions() {
        let chunks = vec![doc("docs/a.md", "a"), RawChunk {
            source_path: "github_issue:owner/repo#1".to_string(),
            content: "i".to_string(),
            last_modified: ts(),
            partition: Partition::Issues,
        }];
        let batches = build_batches(Partition::Docs, &chunks, ts());
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].record_count, 1);
    }

    #[test]
    fn rolls_over_to_new_batch_at_size_threshold() {
        // Each chunk's serialized JSONL line is ~250KB, so 4 fit per batch and
        // 6 chunks split into 2 batches.
        let chunk_payload = "x".repeat(250_000);
        let chunks: Vec<RawChunk> = (0..6)
            .map(|i| doc(&format!("docs/big-{i}.md"), &chunk_payload))
            .collect();
        let batches = build_batches(Partition::Docs, &chunks, ts());

        assert!(
            batches.len() >= 2,
            "expected to roll over to multiple batches, got {}",
            batches.len()
        );
        let total_records: usize = batches.iter().map(|b| b.record_count).sum();
        assert_eq!(total_records, 6, "every record must end up in some batch");
        for b in &batches {
            assert!(
                b.body.len() <= MAX_BATCH_BYTES + chunk_payload.len(),
                "batch body {} exceeds {} (with one in-flight chunk slack)",
                b.body.len(),
                MAX_BATCH_BYTES
            );
            assert!(b.record_count >= 1, "no batch should be empty");
        }
        // First batch is index 0, then 1, etc.
        for (i, b) in batches.iter().enumerate() {
            assert!(
                b.key.ends_with(&format!("batch-{i}.jsonl")),
                "batch {i} key was {}",
                b.key
            );
        }
    }
}
