//! Local Whisper transcription provider using whisper-rs (whisper.cpp bindings)

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{
    debug,
    info,
};
use whisper_rs::{
    FullParams,
    SamplingStrategy,
    WhisperContext,
    WhisperContextParameters,
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

const DEFAULT_MODEL_SIZE: &str = "base";
const CDN_BASE: &str = "https://prod.download.cli.kiro.dev/stable/models";

fn model_filename(size: &str) -> String {
    format!("ggml-{}.bin", size)
}

fn model_url(size: &str) -> String {
    format!("{}/{}.zip", CDN_BASE, model_filename(size))
}

fn flush_stderr() {
    use std::io::Write;
    let _ = std::io::stderr().flush();
}

/// Result of transcription including per-segment timing information.
pub struct PartialResult {
    pub text: String,
    /// Per-segment (text, t0_centisec, t1_centisec) — t values relative to
    /// the start of the PCM slice that was transcribed (NOT absolute).
    pub segments: Vec<(String, i64, i64)>,
}

pub struct LocalWhisperProvider {
    model_path: PathBuf,
    context: Arc<WhisperContext>,
}

impl LocalWhisperProvider {
    pub async fn new() -> VoiceResult<Self> {
        Self::with_model_size(DEFAULT_MODEL_SIZE).await
    }

    pub async fn with_model_size(size: &str) -> VoiceResult<Self> {
        let valid = ["base", "small"];
        let size = if valid.contains(&size) {
            size
        } else {
            DEFAULT_MODEL_SIZE
        };
        let model_path = Self::ensure_model(size).await?;
        info!("Using whisper model: {}", model_path.display());

        // Pre-load the WhisperContext once and share via Arc
        let mp = model_path.clone();
        let context = tokio::task::spawn_blocking(move || {
            whisper_rs::install_whisper_log_trampoline();
            WhisperContext::new_with_params(mp.to_str().unwrap_or(""), WhisperContextParameters::default())
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to load model: {}", e)))
        })
        .await
        .map_err(|e| VoiceError::ProviderInitFailed(format!("Task join error: {}", e)))??;

        Ok(Self {
            model_path,
            context: Arc::new(context),
        })
    }

    /// Get a shared reference to the preloaded WhisperContext.
    pub fn ctx(&self) -> Arc<WhisperContext> {
        Arc::clone(&self.context)
    }

    fn model_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kiro")
            .join("models")
    }

    /// Check if the model file exists and is plausible (>1MB).
    pub fn model_ready(size: &str) -> bool {
        let valid = ["base", "small"];
        let size = if valid.contains(&size) {
            size
        } else {
            DEFAULT_MODEL_SIZE
        };
        let path = Self::model_dir().join(model_filename(size));
        path.exists() && std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false)
    }

    pub async fn ensure_model(size: &str) -> VoiceResult<PathBuf> {
        let dir = Self::model_dir();
        let filename = model_filename(size);
        let path = dir.join(&filename);

        // Check if model exists and is plausible (>1MB — real models are 140MB+)
        if path.exists()
            && let Ok(m) = tokio::fs::metadata(&path).await
        {
            if m.len() > 1_000_000 {
                debug!("Whisper model found at {}", path.display());
                return Ok(path);
            }
            eprintln!(
                "Model file at {} appears incomplete ({} bytes), re-downloading...",
                path.display(),
                m.len()
            );
            let _ = tokio::fs::remove_file(&path).await;
        }

        let url = model_url(size);
        eprintln!("Downloading whisper model ({})...", filename);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to create model dir: {}", e)))?;

        let response = reqwest::get(&url)
            .await
            .map_err(|e| VoiceError::ProviderInitFailed(format!("Download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(VoiceError::ProviderInitFailed(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        // Download zip to a temp file, then extract the .bin model
        let zip_path = dir.join(format!("{}.zip", filename));
        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;

        let download_result = async {
            let mut file = tokio::fs::File::create(&zip_path)
                .await
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to create temp file: {}", e)))?;

            let mut stream = response.bytes_stream();
            use futures::StreamExt;
            use tokio::io::AsyncWriteExt;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| VoiceError::ProviderInitFailed(format!("Download failed: {}", e)))?;
                file.write_all(&chunk)
                    .await
                    .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to write model: {}", e)))?;
                downloaded += chunk.len() as u64;

                if total_size > 0 {
                    let pct = (downloaded as f64 / total_size as f64 * 100.0) as u32;
                    let mb_down = downloaded as f64 / 1_048_576.0;
                    let mb_total = total_size as f64 / 1_048_576.0;
                    let bar_width = 30;
                    let filled = (pct as usize * bar_width / 100).min(bar_width);
                    eprint!(
                        "\r  [{}{}] {}% ({:.0}MB / {:.0}MB)",
                        "█".repeat(filled),
                        "░".repeat(bar_width - filled),
                        pct,
                        mb_down,
                        mb_total
                    );
                    flush_stderr();
                }
            }

            file.flush()
                .await
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to flush model: {}", e)))?;
            Ok::<(), VoiceError>(())
        }
        .await;

        if let Err(e) = download_result {
            let _ = tokio::fs::remove_file(&zip_path).await;
            return Err(e);
        }

        // Clear the progress bar line and show completion status on separate lines
        eprint!("\r\x1B[K");
        flush_stderr();
        if total_size > 0 {
            let mb_total = total_size as f64 / 1_048_576.0;
            eprintln!("  Downloaded ({:.0}MB). Extracting...", mb_total);
        } else {
            eprintln!("  Downloaded. Extracting...");
        }

        // Extract the .bin file from the zip archive
        let extract_dir = dir.clone();
        let extract_filename = filename.clone();
        let tmp_path = dir.join(format!("{}.tmp", filename));
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&zip_path)
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to open zip: {}", e)))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Invalid zip archive: {}", e)))?;

            // Find the .bin file in the archive
            let bin_index = (0..archive.len())
                .find(|&i| {
                    archive
                        .by_index(i)
                        .map(|f| f.name().ends_with(&extract_filename))
                        .unwrap_or(false)
                })
                .ok_or_else(|| VoiceError::ProviderInitFailed(format!("{} not found in zip", extract_filename)))?;

            let mut bin_file = archive
                .by_index(bin_index)
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to read zip entry: {}", e)))?;

            let tmp = extract_dir.join(format!("{}.tmp", extract_filename));
            let mut out = std::fs::File::create(&tmp)
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to create temp file: {}", e)))?;
            std::io::copy(&mut bin_file, &mut out)
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to extract model: {}", e)))?;

            // Clean up zip
            let _ = std::fs::remove_file(&zip_path);
            Ok::<(), VoiceError>(())
        })
        .await
        .map_err(|e| VoiceError::ProviderInitFailed(format!("Extract task failed: {}", e)))?
        .map_err(|e: VoiceError| e)?;

        // Atomic rename: only a complete extraction becomes the final model file
        tokio::fs::rename(&tmp_path, &path)
            .await
            .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to finalize model file: {}", e)))?;

        eprintln!("  Model ready.");
        Ok(path)
    }

    fn transcribe_audio(
        model_path: &std::path::Path,
        pcm_f32: &[f32],
        context_hint: Option<&str>,
    ) -> VoiceResult<String> {
        Self::transcribe_audio_static(model_path, pcm_f32, context_hint)
    }

    /// Static version for use from spawned tasks
    pub fn transcribe_audio_static(
        model_path: &std::path::Path,
        pcm_f32: &[f32],
        context_hint: Option<&str>,
    ) -> VoiceResult<String> {
        // Suppress whisper.cpp's verbose C-level logging
        whisper_rs::install_whisper_log_trampoline();

        let ctx =
            WhisperContext::new_with_params(model_path.to_str().unwrap_or(""), WhisperContextParameters::default())
                .map_err(|e| VoiceError::TranscriptionFailed(format!("Failed to load model: {}", e)))?;

        let mut state = ctx
            .create_state()
            .map_err(|e| VoiceError::TranscriptionFailed(format!("Failed to create state: {}", e)))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(None);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_single_segment(false);
        params.set_suppress_non_speech_tokens(true);
        // [Improvement #6] Suppress blank output from silence
        params.set_suppress_blank(true);

        if let Some(hint) = context_hint {
            params.set_initial_prompt(hint);
        }
        params.set_no_context(false);

        state
            .full(params, pcm_f32)
            .map_err(|e| VoiceError::TranscriptionFailed(format!("Transcription failed: {}", e)))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| VoiceError::TranscriptionFailed(format!("Failed to get segments: {}", e)))?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                let s = segment.trim();
                if is_hallucination(s) {
                    continue;
                }
                text.push_str(s);
                text.push(' ');
            }
        }

        Ok(text.trim().to_string())
    }
}

/// Transcribe audio using a pre-loaded WhisperContext, returning text plus
/// per-segment timing information. This avoids reloading the model from disk.
pub fn transcribe_with_segments(
    ctx: &WhisperContext,
    pcm_f32: &[f32],
    prompt: Option<&str>,
    language: Option<&str>,
) -> VoiceResult<PartialResult> {
    whisper_rs::install_whisper_log_trampoline();

    let mut state = ctx
        .create_state()
        .map_err(|e| VoiceError::TranscriptionFailed(format!("Failed to create state: {}", e)))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(language);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_single_segment(false);
    params.set_suppress_non_speech_tokens(true);
    params.set_suppress_blank(true);
    params.set_no_context(false);

    if let Some(p) = prompt {
        params.set_initial_prompt(p);
    }

    state
        .full(params, pcm_f32)
        .map_err(|e| VoiceError::TranscriptionFailed(format!("Transcription failed: {}", e)))?;

    let num_segments = state
        .full_n_segments()
        .map_err(|e| VoiceError::TranscriptionFailed(format!("Failed to get segments: {}", e)))?;

    let mut text = String::new();
    let mut segments = Vec::new();
    for i in 0..num_segments {
        if let Ok(segment_text) = state.full_get_segment_text(i) {
            let s = segment_text.trim();
            if is_hallucination(s) {
                continue;
            }
            let t0 = state.full_get_segment_t0(i).unwrap_or(0);
            let t1 = state.full_get_segment_t1(i).unwrap_or(0);
            text.push_str(s);
            text.push(' ');
            segments.push((s.to_string(), t0, t1));
        }
    }

    Ok(PartialResult {
        text: text.trim().to_string(),
        segments,
    })
}

/// Detect common whisper hallucinations that occur on silence or noise
fn is_hallucination(text: &str) -> bool {
    let lower = text.to_lowercase();
    let patterns = [
        "[laughter]",
        "[laughing]",
        "[laughs]",
        "(laughter)",
        "(laughing)",
        "(laughs)",
        "[inaudible]",
        "(inaudible)",
        "[music]",
        "(music)",
        "[silence]",
        "(silence)",
        "[applause]",
        "(applause)",
        "displaystyle",
        "speaking in foreign language",
        "general talking gibberish",
        ">>",
        "♪",
        "www.",
        "http",
        "thank you for watching",
        "thanks for watching",
        "subscribe",
        "please like and subscribe",
        "see you next time",
    ];
    patterns.iter().any(|p| lower.contains(p)) || lower.chars().all(|c| !c.is_alphanumeric())
}

#[async_trait]
impl TranscriptionProvider for LocalWhisperProvider {
    fn whisper_ctx(&self) -> Option<Arc<WhisperContext>> {
        Some(self.ctx())
    }

    async fn stream_transcribe(
        &self,
        mut audio_stream: AudioStream,
        options: &TranscriptionOptions,
    ) -> VoiceResult<TranscriptionStream> {
        let (tx, rx) = mpsc::channel(1);
        let model_path = self.model_path.clone();
        let context_hint = options.context_hint.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();
            let mut pcm_i16_bytes = Vec::<u8>::new();

            while let Some(blob) = audio_stream.recv().await {
                pcm_i16_bytes.extend_from_slice(&blob.data);
            }

            if pcm_i16_bytes.is_empty() {
                return;
            }

            let pcm_i16: Vec<i16> = pcm_i16_bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();

            let pcm_f32: Vec<f32> = pcm_i16.iter().map(|&s| s as f32 / 32768.0).collect();

            let result = tokio::task::spawn_blocking(move || {
                Self::transcribe_audio(&model_path, &pcm_f32, context_hint.as_deref())
            })
            .await;

            match result {
                Ok(Ok(text)) if !text.is_empty() => {
                    let _ = tx
                        .send(StreamingTranscription::final_result(text, 0.95, start.elapsed()))
                        .await;
                },
                Ok(Err(e)) => {
                    debug!("Transcription error: {}", e);
                },
                _ => {},
            }
        });

        Ok(rx)
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn model_path(&self) -> Option<std::path::PathBuf> {
        Some(self.model_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hallucination_detects_common_patterns() {
        assert!(is_hallucination("[laughter]"));
        assert!(is_hallucination("(music)"));
        assert!(is_hallucination("[SILENCE]")); // case-insensitive
        assert!(is_hallucination("Thank you for watching"));
        assert!(is_hallucination("♪♪♪"));
        assert!(is_hallucination("www.example.com"));
    }

    #[test]
    fn hallucination_rejects_normal_speech() {
        assert!(!is_hallucination("hello world"));
        assert!(!is_hallucination("refactor this function"));
        assert!(!is_hallucination("日本語のテスト"));
        assert!(!is_hallucination("add error handling to the parser"));
    }

    #[test]
    fn hallucination_detects_non_alphanumeric_only() {
        assert!(is_hallucination("..."));
        assert!(is_hallucination("---"));
        assert!(is_hallucination("   "));
    }
}
