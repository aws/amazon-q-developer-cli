use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTranscription {
    pub partial_text: String,
    pub confidence: f32,
    pub is_final: bool,
    pub timestamp: Duration,
    pub word_count: usize,
}

impl StreamingTranscription {
    #[allow(dead_code)]
    pub fn new(text: String, is_final: bool) -> Self {
        Self {
            word_count: text.split_whitespace().count(),
            partial_text: text,
            confidence: 1.0,
            is_final,
            timestamp: Duration::from_secs(0),
        }
    }

    #[allow(dead_code)]
    pub fn partial(text: String, confidence: f32, timestamp: Duration) -> Self {
        Self {
            word_count: text.split_whitespace().count(),
            partial_text: text,
            confidence,
            is_final: false,
            timestamp,
        }
    }

    pub fn final_result(text: String, confidence: f32, timestamp: Duration) -> Self {
        Self {
            word_count: text.split_whitespace().count(),
            partial_text: text,
            confidence,
            is_final: true,
            timestamp,
        }
    }
}

pub type TranscriptionStream = mpsc::Receiver<StreamingTranscription>;

#[derive(Debug, Clone)]
pub struct AudioBlob {
    pub data: Vec<u8>,
}

pub type AudioStream = mpsc::Receiver<AudioBlob>;
