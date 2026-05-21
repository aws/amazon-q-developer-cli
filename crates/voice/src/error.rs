use eyre::Report as ErrReport;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("Microphone not available or permission denied")]
    MicrophoneUnavailable,

    #[error("Audio format not supported")]
    UnsupportedAudioFormat,

    #[error("Audio processing error: {0}")]
    AudioProcessingError(String),

    #[error("Provider initialization failed: {0}")]
    ProviderInitFailed(String),

    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),
}

impl From<ErrReport> for VoiceError {
    fn from(err: ErrReport) -> Self {
        VoiceError::AudioProcessingError(err.to_string())
    }
}

pub type VoiceResult<T> = Result<T, VoiceError>;
