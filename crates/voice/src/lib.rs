// Core modules
pub mod audio_capture;
pub mod partial_pipeline;
pub mod silero_vad;

// Architecture modules
pub mod error;
pub mod provider;
pub mod providers;
pub mod streaming;

// Main handler
pub mod transcription_provider;
pub mod voice_cloud_setup;
pub mod voice_handler;
pub mod voice_serve;

// Re-exports
pub use audio_capture::AudioCapture;
pub use error::VoiceError;
pub use transcription_provider::TranscriptionBackend;
pub use voice_handler::VoiceHandler;
