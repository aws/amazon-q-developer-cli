//! Manifest written to `s3://<bucket>/manifest.json` after every successful
//! run. Pure data shapes here; the actual S3 upload happens in `main.rs`.

use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::Partition;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Last successful run timestamp.
    pub last_run_at: DateTime<Utc>,
    /// Bedrock ingestion job id this run started.
    pub last_ingestion_job_id: String,
    /// Per-partition record counts from the last successful run.
    pub counts: BTreeMap<String, usize>,
}

impl Manifest {
    pub fn new(
        run_at: DateTime<Utc>,
        ingestion_job_id: impl Into<String>,
        counts_per_partition: &BTreeMap<Partition, usize>,
    ) -> Self {
        let counts = counts_per_partition
            .iter()
            .map(|(p, n)| (p.as_str().to_string(), *n))
            .collect();
        Self {
            last_run_at: run_at,
            last_ingestion_job_id: ingestion_job_id.into(),
            counts,
        }
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).expect("serialize manifest")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn manifest_round_trips_through_json() {
        let mut counts = BTreeMap::new();
        counts.insert(Partition::Docs, 12);
        counts.insert(Partition::Issues, 7);
        counts.insert(Partition::Releases, 0);
        let m = Manifest::new(
            Utc.with_ymd_and_hms(2026, 5, 19, 17, 0, 0).unwrap(),
            "JOB-1",
            &counts,
        );
        let bytes = m.to_json();
        let back: Manifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(m, back);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("\"docs\""));
        assert!(s.contains("\"issues\""));
        assert!(s.contains("\"releases\""));
        assert!(s.contains("\"last_ingestion_job_id\""));
    }
}
