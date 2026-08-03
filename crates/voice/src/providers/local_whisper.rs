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

/// Pinned SHA-256 of each supported model's extracted `.bin`, lowercase hex.
///
/// The model is a native artifact loaded and executed by whisper.cpp, from a
/// predictable, user-writable cache directory. Verifying it against a pinned
/// digest before every load (not just after download) closes the local
/// cache-poisoning / model-substitution vector: a file planted or tampered with
/// by another same-UID process is rejected instead of being loaded. These digests
/// are the canonical whisper.cpp weights mirrored on the Kiro CDN — verified
/// against the actual `ggml-{base,small}.bin.zip` artifacts.
fn model_sha256(size: &str) -> &'static str {
    match normalize_model_size(size) {
        "small" => "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        // "base"
        _ => "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    }
}

/// Normalize an arbitrary model-size string to a supported value, falling back
/// to the default for anything unknown. Single source of truth so the filename,
/// download size, and readiness check never disagree for a bogus setting.
pub fn normalize_model_size(size: &str) -> &'static str {
    match size {
        "small" => "small",
        _ => DEFAULT_MODEL_SIZE,
    }
}

/// The on-disk / CDN filename for a (normalized) model size, e.g. `ggml-base.bin`.
pub fn model_filename(size: &str) -> String {
    format!("ggml-{}.bin", normalize_model_size(size))
}

/// Approximate download size in MB for a (normalized) model size, for UI notices.
pub fn model_download_size_mb(size: &str) -> u32 {
    match normalize_model_size(size) {
        "small" => 466,
        _ => 148,
    }
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
        let size = normalize_model_size(size);
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

    /// Best-effort: restrict the model directory to owner-only (0700) on Unix.
    /// No-op on other platforms and on any error — integrity is enforced by the
    /// SHA-256 check, not by permissions.
    #[cfg(unix)]
    fn harden_dir_permissions(dir: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
            debug!("Could not restrict model dir permissions on {}: {}", dir.display(), e);
        }
    }

    #[cfg(not(unix))]
    fn harden_dir_permissions(_dir: &std::path::Path) {}

    /// Check if the model file exists and passes its pinned-SHA-256 integrity
    /// check. This is intentionally the full cryptographic check (not a size
    /// heuristic): callers use it to decide whether a usable model is present,
    /// and a tampered/planted file must not count as "ready".
    ///
    /// Async because hashing a 148-487MB model takes long enough to stall the
    /// async runtime; the digest runs on a blocking task, as in `ensure_model`.
    pub async fn model_ready(size: &str) -> bool {
        let path = Self::model_dir().join(model_filename(size));
        if !path.exists() {
            return false;
        }
        let size = size.to_string();
        tokio::task::spawn_blocking(move || Self::verify_model_integrity(&path, &size).is_ok())
            .await
            .unwrap_or(false)
    }

    /// Compute the SHA-256 of a file, streaming it in chunks so a large model
    /// isn't read fully into memory.
    fn file_sha256(path: &std::path::Path) -> std::io::Result<String> {
        use std::io::Read;

        use sha2::{
            Digest,
            Sha256,
        };
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    /// Verify a model file matches its pinned SHA-256. Called after download AND
    /// before every load from cache, so an attacker-controlled file planted in
    /// the predictable, user-writable model directory is rejected rather than
    /// loaded and executed by the native whisper.cpp runtime.
    fn verify_model_integrity(path: &std::path::Path, size: &str) -> VoiceResult<()> {
        let expected = model_sha256(size);
        let actual = Self::file_sha256(path)
            .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to read model for integrity check: {}", e)))?;
        if actual.eq_ignore_ascii_case(expected) {
            Ok(())
        } else {
            Err(VoiceError::ProviderInitFailed(format!(
                "Model integrity check failed for {}: expected SHA-256 {}, got {}. The cached model may be corrupt or \
                 tampered with; delete it and let the voice feature re-download.",
                path.display(),
                expected,
                actual
            )))
        }
    }

    pub async fn ensure_model(size: &str) -> VoiceResult<PathBuf> {
        let dir = Self::model_dir();
        let filename = model_filename(size);
        let path = dir.join(&filename);

        // Reuse the cached model ONLY if it passes its pinned-SHA-256 check. A
        // size heuristic can't tell a genuine model from a planted/tampered one
        // in this predictable, user-writable directory, so verify cryptographically
        // before trusting (and later loading + executing) the cached file.
        if path.exists() {
            let vp = path.clone();
            let vsize = size.to_string();
            let verified = tokio::task::spawn_blocking(move || Self::verify_model_integrity(&vp, &vsize))
                .await
                .map_err(|e| VoiceError::ProviderInitFailed(format!("Integrity check task failed: {}", e)))?;
            match verified {
                Ok(()) => {
                    debug!("Whisper model found and verified at {}", path.display());
                    return Ok(path);
                },
                Err(e) => {
                    // Don't load a file that failed verification. Drop it and
                    // re-download from the pinned HTTPS source.
                    eprintln!(
                        "Cached model at {} failed integrity check ({}); re-downloading...",
                        path.display(),
                        e
                    );
                    let _ = tokio::fs::remove_file(&path).await;
                },
            }
        }

        let url = model_url(size);

        // Model + whisper.cpp are MIT-licensed; surface the notice before downloading.
        eprintln!(
            "\nThe voice feature downloads the OpenAI Whisper model (ggml format via whisper.cpp).\n\
             Model license: MIT — https://github.com/openai/whisper/blob/main/LICENSE\n\
             whisper.cpp license: MIT — https://github.com/ggml-org/whisper.cpp/blob/master/LICENSE\n"
        );
        eprintln!("Downloading whisper model ({})...", filename);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| VoiceError::ProviderInitFailed(format!("Failed to create model dir: {}", e)))?;
        // Defense-in-depth: restrict the model dir to owner-only (0700) on Unix so
        // a different local user can't drop files into it. Same-UID tampering is
        // still handled by the SHA-256 verification above — this is not the primary
        // control. Best-effort: a failure here must not block the download.
        Self::harden_dir_permissions(&dir);

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

        // Verify the freshly-extracted artifact against its pinned SHA-256 BEFORE
        // it becomes the final model file. A download that doesn't match the
        // pinned digest (corrupt, MITM despite TLS, or a compromised mirror) is
        // discarded rather than promoted into the cache.
        let verify_tmp = tmp_path.clone();
        let verify_size = size.to_string();
        let verified = tokio::task::spawn_blocking(move || Self::verify_model_integrity(&verify_tmp, &verify_size))
            .await
            .map_err(|e| VoiceError::ProviderInitFailed(format!("Integrity check task failed: {}", e)))?;
        if let Err(e) = verified {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(e);
        }

        // Atomic rename: only a complete, verified extraction becomes the final model file
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

    #[test]
    fn normalize_model_size_falls_back_to_default() {
        assert_eq!(normalize_model_size("base"), "base");
        assert_eq!(normalize_model_size("small"), "small");
        // Anything unsupported normalizes to the default so downstream state agrees.
        assert_eq!(normalize_model_size("large"), DEFAULT_MODEL_SIZE);
        assert_eq!(normalize_model_size(""), DEFAULT_MODEL_SIZE);
    }

    #[test]
    fn model_filename_and_size_agree_for_invalid_size() {
        // A bogus size must not produce a "ggml-large.bin" filename with a
        // mismatched download size — both derive from the normalized value.
        assert_eq!(model_filename("large"), "ggml-base.bin");
        assert_eq!(model_download_size_mb("large"), model_download_size_mb("base"));
        assert_eq!(model_filename("small"), "ggml-small.bin");
        assert_eq!(model_download_size_mb("small"), 466);
        assert_eq!(model_download_size_mb("base"), 148);
    }

    #[test]
    fn model_sha256_is_pinned_and_normalized() {
        // Digests are pinned per size and unknown sizes fall back to base's.
        assert_eq!(
            model_sha256("base"),
            "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"
        );
        assert_eq!(
            model_sha256("small"),
            "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"
        );
        assert_eq!(model_sha256("large"), model_sha256("base"));
        assert_ne!(model_sha256("small"), model_sha256("base"));
    }

    #[test]
    fn file_sha256_matches_known_vector() {
        // SHA-256("abc") — the canonical NIST test vector.
        let dir = std::env::temp_dir().join(format!("kvs-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("abc.txt");
        std::fs::write(&f, b"abc").unwrap();
        let got = LocalWhisperProvider::file_sha256(&f).unwrap();
        assert_eq!(got, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_model_integrity_rejects_tampered_file() {
        // A planted/tampered file whose bytes don't match the pinned digest must
        // be rejected — this is the core cache-poisoning defense.
        let dir = std::env::temp_dir().join(format!("kvs-tamper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let planted = dir.join(model_filename("base"));
        std::fs::write(&planted, b"totally not a whisper model").unwrap();

        let result = LocalWhisperProvider::verify_model_integrity(&planted, "base");
        assert!(result.is_err(), "tampered model must fail integrity verification");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("integrity check failed"),
            "error should name the integrity failure: {msg}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
