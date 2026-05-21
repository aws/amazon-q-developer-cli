use std::sync::Arc;

use async_trait::async_trait;
use whisper_rs::WhisperContext;

use super::error::VoiceResult;
use super::streaming::{
    AudioStream,
    TranscriptionStream,
};

#[derive(Debug, Clone, Default)]
pub struct TranscriptionOptions {
    /// Recent conversation context to improve transcription accuracy.
    /// Whisper uses this as an "initial prompt" to prime the model with
    /// domain-specific terms (code identifiers, function names, etc.)
    pub context_hint: Option<String>,
}

#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Streaming transcription with real-time updates
    async fn stream_transcribe(
        &self,
        audio_stream: AudioStream,
        options: &TranscriptionOptions,
    ) -> VoiceResult<TranscriptionStream>;

    /// Check if provider supports streaming
    fn supports_streaming(&self) -> bool;

    /// Get the model path if applicable (for partial transcription during recording)
    fn model_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Get a shared WhisperContext if the provider has one loaded (avoids double-loading).
    fn whisper_ctx(&self) -> Option<Arc<WhisperContext>> {
        None
    }
}
