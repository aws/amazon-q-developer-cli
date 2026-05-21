use async_trait::async_trait;
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;
use tracing::{
    debug,
    error,
};

use crate::error::{
    VoiceError,
    VoiceResult,
};
use crate::provider::{
    TranscriptionOptions,
    TranscriptionProvider,
};
use crate::streaming::{
    AudioStream,
    StreamingTranscription,
    TranscriptionStream,
};

#[derive(Debug, Serialize)]
struct RecordRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    context_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordResponse {
    text: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

/// Provider that delegates voice recording and transcription to a remote server.
/// Used when kiro-cli runs on a cloud desktop without a microphone — the remote
/// server runs on the user's local machine where a mic is available.
pub struct RemoteServerProvider {
    base_url: String,
    client: Client,
    model_size: Option<String>,
    language: Option<String>,
}

impl RemoteServerProvider {
    pub fn new(base_url: &str) -> VoiceResult<Self> {
        Self::with_options(base_url, None, None)
    }

    pub fn with_model_size(base_url: &str, model_size: Option<String>) -> VoiceResult<Self> {
        Self::with_options(base_url, model_size, None)
    }

    pub fn with_options(base_url: &str, model_size: Option<String>, language: Option<String>) -> VoiceResult<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| VoiceError::AudioProcessingError(format!("Failed to create HTTP client: {e}")))?;

        // Normalize URL — strip trailing slash
        let base_url = base_url.trim_end_matches('/').to_string();

        Ok(Self {
            base_url,
            client,
            model_size,
            language,
        })
    }

    /// Check if the remote voice server is reachable and healthy
    pub async fn health_check(&self) -> VoiceResult<()> {
        let url = format!("{}/voice/status", self.base_url);
        debug!("Checking remote voice server at {}", url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            VoiceError::AudioProcessingError(format!(
                "Cannot reach voice server at {}: {e}. \
                     Make sure 'kiro-cli voice-serve' is running on your local machine \
                     and the port is forwarded via SSH (-R 19876:localhost:19876)",
                self.base_url
            ))
        })?;
        if !response.status().is_success() {
            return Err(VoiceError::AudioProcessingError(format!(
                "Voice server at {} returned HTTP {}",
                self.base_url,
                response.status()
            )));
        }
        Ok(())
    }

    /// Request the remote server to record audio and return transcribed text
    pub async fn record_and_transcribe(&self, context_hint: Option<String>) -> VoiceResult<Option<String>> {
        let url = format!("{}/voice/record", self.base_url);
        debug!("Requesting voice recording from {}", url);

        let request = RecordRequest {
            context_hint,
            language: self.language.clone(),
            model_size: self.model_size.clone(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| VoiceError::AudioProcessingError(format!("Voice server request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(VoiceError::AudioProcessingError(format!(
                "Voice server returned {status}: {body}"
            )));
        }

        let record_response: RecordResponse = response
            .json()
            .await
            .map_err(|e| VoiceError::AudioProcessingError(format!("Failed to parse voice server response: {e}")))?;

        if let Some(err) = record_response.error {
            error!("Remote voice server error: {}", err);
            return Err(VoiceError::AudioProcessingError(err));
        }

        Ok(record_response.text.filter(|t| !t.trim().is_empty()))
    }
}

#[async_trait]
impl TranscriptionProvider for RemoteServerProvider {
    async fn stream_transcribe(
        &self,
        _audio_stream: AudioStream,
        options: &TranscriptionOptions,
    ) -> VoiceResult<TranscriptionStream> {
        // For remote server, we don't stream audio — the server handles recording.
        // We just make a single HTTP request and return the result as a single event.
        let text = self.record_and_transcribe(options.context_hint.clone()).await?;

        let (tx, rx) = mpsc::channel(1);
        if let Some(text) = text {
            let _ = tx
                .send(StreamingTranscription::final_result(
                    text,
                    1.0,
                    std::time::Duration::from_secs(0),
                ))
                .await;
        }
        Ok(rx)
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

/// SSE event received from /voice/record/stream
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEvent {
    Activity { level: u8 },
    Done { text: Option<String> },
    Error { message: String },
}

impl RemoteServerProvider {
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Call /voice/record/stream (SSE), sending activity levels to `activity_tx`.
    /// Returns the final transcribed text when recording is complete.
    pub async fn record_streaming(
        &self,
        context_hint: Option<String>,
        activity_tx: mpsc::Sender<u8>,
    ) -> VoiceResult<Option<String>> {
        use futures::StreamExt;

        let url = format!("{}/voice/record/stream", self.base_url);
        debug!("Requesting streaming voice recording from {}", url);

        let request = RecordRequest {
            context_hint,
            language: self.language.clone(),
            model_size: self.model_size.clone(),
        };
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| VoiceError::AudioProcessingError(format!("Voice server request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(VoiceError::AudioProcessingError(format!(
                "Voice server returned {status}: {body}"
            )));
        }

        let mut stream = response.bytes_stream();
        let mut buf = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| VoiceError::AudioProcessingError(format!("SSE stream error: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            // Parse complete SSE events (delimited by \n\n)
            while let Some(pos) = buf.find("\n\n") {
                let raw = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();

                for line in raw.lines() {
                    if let Some(data) = line.strip_prefix("data: ")
                        && let Ok(event) = serde_json::from_str::<SseEvent>(data)
                    {
                        match event {
                            SseEvent::Activity { level } => {
                                let _ = activity_tx.try_send(level);
                            },
                            SseEvent::Done { text } => {
                                return Ok(text.filter(|t| !t.trim().is_empty()));
                            },
                            SseEvent::Error { message } => {
                                return Err(VoiceError::AudioProcessingError(message));
                            },
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}
