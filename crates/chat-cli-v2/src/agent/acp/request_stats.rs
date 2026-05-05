//! In-memory ring buffer of per-request metadata for `/stats` debugging.
//!
//! Captures request IDs, timings, token counts, and error info from each
//! model response stream so they can be inspected interactively without
//! relying on log verbosity.

use std::collections::VecDeque;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};

const MAX_RECORDS: usize = 100;

/// A single request's metadata, captured from `StreamMetadata` at stream end.
#[derive(Debug, Clone)]
pub struct RequestRecord {
    pub request_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub duration: Option<Duration>,
    pub time_to_first_chunk: Option<Duration>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub status_code: Option<u16>,
    pub had_tool_use: bool,
    pub error: Option<String>,
}

/// Shared handle to the ring buffer. Clone-cheap (Arc).
#[derive(Debug, Clone, Default)]
pub struct RequestStats {
    records: Arc<Mutex<VecDeque<RequestRecord>>>,
}

impl RequestStats {
    pub fn push(&self, record: RequestRecord) {
        let mut buf = self.records.lock().unwrap();
        if buf.len() >= MAX_RECORDS {
            buf.pop_front();
        }
        buf.push_back(record);
    }

    pub fn snapshot(&self) -> Vec<RequestRecord> {
        self.records.lock().unwrap().iter().cloned().collect()
    }
}
