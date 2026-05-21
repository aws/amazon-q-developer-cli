use std::io::{
    self,
    Write,
};
use std::time::{
    Duration,
    Instant,
};

use eyre::Result;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::error;

use super::provider::TranscriptionProvider;
use super::providers::local_whisper::LocalWhisperProvider;
use super::providers::remote_server::RemoteServerProvider;
use super::silero_vad::SileroVad;
use super::transcription_provider::TranscriptionBackend;
use super::{
    AudioCapture,
    VoiceError,
};

/// RAII guard that disables raw mode on drop, ensuring the terminal is
/// restored even if the function panics between enable and disable.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        crossterm::terminal::disable_raw_mode().ok();
    }
}

// [Improvement #3] VAD constants — tuned for MacBook Pro mics
const VOICE_THRESHOLD_DB: f64 = -42.0; // Stricter than -48dB (VoiceMode uses -40 to -45)
const DEFAULT_SILENCE_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_MAX_SESSION_TIME: Duration = Duration::from_secs(300);
const MIN_RECORDING_DURATION: Duration = Duration::from_millis(500); // Don't process < 0.5s
const INITIAL_GRACE_PERIOD: Duration = Duration::from_secs(1); // Don't check silence for first 1s

/// [Improvement #4] Play terminal bell as audio chime
fn chime_start() {
    eprint!("\x07"); // Terminal bell on record start
    io::stderr().flush().ok();
}

fn chime_stop() {
    eprint!("\x07"); // Terminal bell on record stop
    io::stderr().flush().ok();
}

/// Compute dB level from PCM i16 samples
fn compute_db(data: &[u8]) -> f64 {
    if data.len() < 2 {
        return -100.0;
    }
    let samples: Vec<i16> = data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    if samples.is_empty() {
        return -100.0;
    }
    let rms = (samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    if rms > 0.0 {
        20.0 * (rms / 32767.0).log10()
    } else {
        -100.0
    }
}

/// Output a JSON event line on stdout for structured consumers (TUI).
fn emit_voice_event(event_type: &str, value: serde_json::Value) {
    use std::io::Write;
    let obj = serde_json::json!({"type": event_type, "value": value});
    let _ = writeln!(io::stdout(), "{}", obj);
    io::stdout().flush().ok();
}

/// Standalone voice mode: record → transcribe → print to stdout → exit.
/// Output format depends on whether stdout is a TTY:
/// - TTY (interactive): prints plain text transcription with volume bar
/// - Pipe (TUI): prints JSON lines with level updates and final text
pub async fn voice_only_mode(
    server_url: Option<String>,
    silence_timeout_secs: Option<u64>,
    language: Option<String>,
    model_size: Option<String>,
) -> eyre::Result<std::process::ExitCode> {
    use std::io::IsTerminal;
    let is_piped = !std::io::stdout().is_terminal();
    // "auto" means auto-detect (same as unset)
    let language = language.filter(|l| l != "auto");
    let silence_timeout = silence_timeout_secs.map(Duration::from_secs);

    // If model isn't downloaded yet, download it and exit — don't auto-start recording.
    // The caller (TUI) will show a message telling the user to try again.
    let effective_model_size = model_size.as_deref().unwrap_or("base");
    if is_piped && !LocalWhisperProvider::model_ready(effective_model_size) {
        emit_voice_event("status", serde_json::json!("downloading"));
        LocalWhisperProvider::ensure_model(effective_model_size)
            .await
            .map_err(|e| eyre::eyre!("Model download failed: {}", e))?;
        emit_voice_event("status", serde_json::json!("download_complete"));
        return Ok(std::process::ExitCode::SUCCESS);
    }

    // Try local capture with LocalWhisper; fall back to remote server if unavailable.
    // Download model first, then open mic — avoids showing macOS mic indicator during download.
    let local_result = async {
        let provider: Box<dyn super::provider::TranscriptionProvider + Send + Sync> = Box::new(
            LocalWhisperProvider::with_model_size(effective_model_size)
                .await
                .map_err(|e| eyre::eyre!("Failed to initialize whisper: {}", e))?,
        );
        let audio_capture = AudioCapture::new()?;
        Ok::<_, eyre::Error>((provider, audio_capture))
    }
    .await;

    if local_result.is_err() {
        if let Some(url) = server_url {
            let mut handler = VoiceHandler::with_language(
                TranscriptionBackend::RemoteServer { url },
                None,
                model_size.clone(),
                language.clone(),
                None,
                None,
            )
            .await?;
            return match handler.listen_remote().await? {
                Some(text) => {
                    let text = text.trim_start_matches(['!', '/']).trim().to_string();
                    if !text.is_empty() {
                        if is_piped {
                            emit_voice_event("text", serde_json::json!(text));
                        } else {
                            let _ = writeln!(io::stdout(), "{}", text);
                        }
                    }
                    Ok(std::process::ExitCode::SUCCESS)
                },
                None => Ok(std::process::ExitCode::SUCCESS),
            };
        }
        return Err(local_result.err().unwrap());
    }

    let (provider, audio_capture) = local_result.ok().unwrap();

    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1000);
    let _stream = audio_capture.start_capture(audio_tx)?;

    let mut audio_buffer = Vec::new();
    let recording_start = Instant::now();
    let mut last_voice_time = Instant::now();

    // Clear any residual stderr output (e.g. model download progress bar)
    if !is_piped {
        let _ = write!(io::stderr(), "\r\x1B[K");
        io::stderr().flush().ok();
        let _ = write!(io::stdout(), "\r\x1B[K");
        io::stdout().flush().ok();
    }

    chime_start();

    let mut vad = SileroVad::new().ok();
    let mut level: u8 = 0;
    #[allow(unused_assignments)]
    let mut committed_text = String::new();

    if is_piped {
        emit_voice_event("status", serde_json::json!("recording"));
    } else {
        VoiceHandler::update_status_line(0.0, 0, "");
    }

    if is_piped {
        // Piped stdin (PTT mode): read stop signal ('\n') from stdin asynchronously.
        // Don't touch crossterm — there's no terminal attached.
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 1];
            loop {
                match std::io::stdin().read(&mut buf) {
                    Ok(0) | Err(_) => break,           // EOF or error = stop
                    Ok(_) if buf[0] == b'\n' => break, // Enter = stop
                    _ => {},
                }
            }
            let _ = stop_tx.send(());
        });

        // Reuse the Arc<WhisperContext> already loaded by the provider (avoids double-load).
        use super::partial_pipeline::PartialPipeline;

        let whisper_ctx = provider.whisper_ctx().expect("local provider must have whisper_ctx");

        let mut pipeline = PartialPipeline::new(whisper_ctx, language.clone());

        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                chunk = audio_rx.recv() => {
                    match chunk {
                        Some(data) => {
                            let is_speech = vad.as_mut().map_or_else(
                                || compute_db(&data) > VOICE_THRESHOLD_DB,
                                |v| v.is_speech(&data),
                            );
                            if is_speech {
                                level = 8;
                                last_voice_time = Instant::now();
                            } else {
                                level = level.saturating_sub(1);
                            }
                            emit_voice_event("level", serde_json::json!(level));

                            pipeline.push_chunk(&data, is_speech);

                            // Try to fire partial and check for results
                            if let Some(display) = pipeline.try_fire_partial() {
                                committed_text = display;
                                emit_voice_event("partial", serde_json::json!(committed_text));
                            }

                            if let Some(timeout) = silence_timeout {
                                let past_grace = recording_start.elapsed() >= INITIAL_GRACE_PERIOD;
                                if past_grace && last_voice_time.elapsed() >= timeout && pipeline.had_speech() { break; }
                            }
                            if recording_start.elapsed() >= DEFAULT_MAX_SESSION_TIME { break; }
                        },
                        None => break,
                    }
                }
            }
        }

        // Finalize: get the final transcription from the pipeline
        chime_stop();
        let duration = recording_start.elapsed();
        let had_speech = pipeline.had_speech();
        let buf_len = pipeline.audio_len();
        tracing::debug!(
            ?duration,
            had_speech,
            buf_len,
            stable_prefix = ?pipeline.stable_prefix(),
            display = ?pipeline.display_text(),
            "PTT recording finished"
        );
        if duration < MIN_RECORDING_DURATION || !had_speech {
            tracing::debug!("EARLY EXIT: too short or no speech");
            emit_voice_event("text", serde_json::json!(null));
            return Ok(std::process::ExitCode::SUCCESS);
        }

        let final_text = pipeline.finalize().await;
        let final_text = final_text.trim().to_string();
        tracing::debug!(final_text = ?final_text, "PTT transcription result");
        if !final_text.is_empty() {
            emit_voice_event("text", serde_json::json!(final_text));
        } else {
            emit_voice_event("text", serde_json::json!(null));
        }
        return Ok(std::process::ExitCode::SUCCESS);
    } else {
        use crossterm::event::{
            Event,
            EventStream,
            KeyCode,
            KeyModifiers,
        };
        use crossterm::terminal::enable_raw_mode;
        use futures::StreamExt;

        enable_raw_mode().ok();
        let _raw_guard = RawModeGuard;
        let mut event_stream = EventStream::new();

        loop {
            tokio::select! {
                event = event_stream.next() => {
                    if let Some(Ok(Event::Key(k))) = event {
                        if k.code == KeyCode::Enter {
                            break;
                        }
                        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                            drop(event_stream);
                            drop(_raw_guard);
                            let _ = write!(io::stdout(), "\r\x1B[K");
                            io::stdout().flush().ok();
                            return Ok(std::process::ExitCode::FAILURE);
                        }
                    }
                }
                chunk = audio_rx.recv() => {
                    match chunk {
                        Some(data) => {
                            audio_buffer.extend_from_slice(&data);
                            let is_speech = vad.as_mut().map_or_else(
                                || compute_db(&data) > VOICE_THRESHOLD_DB,
                                |v| v.is_speech(&data),
                            );
                            if is_speech { last_voice_time = Instant::now(); level = 8; }
                            else { level = level.saturating_sub(1); }
                            VoiceHandler::update_status_line(recording_start.elapsed().as_secs_f32(), level, "");
                            let timeout = silence_timeout.unwrap_or(DEFAULT_SILENCE_TIMEOUT);
                            let past_grace = recording_start.elapsed() >= INITIAL_GRACE_PERIOD;
                            if past_grace && last_voice_time.elapsed() >= timeout && !audio_buffer.is_empty() {
                                break;
                            }
                            if recording_start.elapsed() >= DEFAULT_MAX_SESSION_TIME { break; }
                        },
                        None => break,
                    }
                }
            }
        }

        drop(event_stream);
        // _raw_guard dropped here automatically, restoring terminal
    }
    // TTY branch post-loop handling (piped branch returns early above)
    chime_stop();
    let _ = write!(io::stdout(), "\r\x1B[K\n");
    io::stdout().flush().ok();

    // [Improvement #3] Don't process very short recordings (noise bursts)
    let duration = recording_start.elapsed();
    if audio_buffer.is_empty() || duration < MIN_RECORDING_DURATION {
        return Ok(std::process::ExitCode::SUCCESS);
    }

    // TTY mode: no partials, transcribe full buffer
    let (blob_tx, blob_rx) = mpsc::channel(1);
    let _ = blob_tx.send(super::streaming::AudioBlob { data: audio_buffer }).await;
    drop(blob_tx);

    let options = super::provider::TranscriptionOptions { context_hint: None };
    let mut rx = provider
        .stream_transcribe(blob_rx, &options)
        .await
        .map_err(|e| eyre::eyre!("Transcription failed: {}", e))?;

    if let Ok(Some(evt)) = timeout(Duration::from_secs(60), rx.recv()).await {
        let text = evt.partial_text.trim().to_string();
        if !text.is_empty() {
            let _ = writeln!(io::stdout(), "{}", text);
        }
    }

    Ok(std::process::ExitCode::SUCCESS)
}

pub struct VoiceHandler {
    provider: Box<dyn TranscriptionProvider + Send + Sync>,
    audio_capture: Option<AudioCapture>,
    context_hint: Option<String>,
    silence_timeout: Duration,
    max_session_time: Duration,
    vad: Option<SileroVad>,
    /// Set when using RemoteServer backend — used for SSE streaming in listen_remote
    remote_base_url: Option<String>,
    /// Model size to forward to remote server
    model_size: Option<String>,
}

impl VoiceHandler {
    pub async fn new(
        backend: TranscriptionBackend,
        context_hint: Option<String>,
        model_size: Option<String>,
        silence_timeout: Option<u64>,
        max_session_time: Option<u64>,
    ) -> Result<Self> {
        Self::with_language(
            backend,
            context_hint,
            model_size,
            None,
            silence_timeout,
            max_session_time,
        )
        .await
    }

    pub async fn with_language(
        backend: TranscriptionBackend,
        context_hint: Option<String>,
        model_size: Option<String>,
        language: Option<String>,
        silence_timeout: Option<u64>,
        max_session_time: Option<u64>,
    ) -> Result<Self> {
        // Remote server mode: no local audio capture needed
        if let TranscriptionBackend::RemoteServer { ref url } = backend {
            let provider = RemoteServerProvider::with_options(url, model_size.clone(), language)
                .map_err(|e| eyre::eyre!("Failed to create remote voice provider: {}", e))?;
            provider.health_check().await.map_err(|e| eyre::eyre!("{}", e))?;
            let remote_base_url = Some(provider.base_url().to_string());
            return Ok(Self {
                provider: Box::new(provider),
                audio_capture: None,
                context_hint,
                silence_timeout: silence_timeout
                    .map(Duration::from_secs)
                    .unwrap_or(DEFAULT_SILENCE_TIMEOUT),
                max_session_time: max_session_time
                    .map(Duration::from_secs)
                    .unwrap_or(DEFAULT_MAX_SESSION_TIME),
                vad: None,
                remote_base_url,
                model_size,
            });
        }

        // Download model first, then open mic — avoids showing macOS mic indicator during download.
        let provider: Box<dyn TranscriptionProvider + Send + Sync> = match backend {
            TranscriptionBackend::LocalWhisper => {
                let size = model_size.as_deref().unwrap_or("base");
                Box::new(LocalWhisperProvider::with_model_size(size).await?)
            },
            TranscriptionBackend::RemoteServer { .. } => unreachable!(),
        };

        let audio_capture = AudioCapture::new()?;
        let vad = SileroVad::new().map_err(|e| eyre::eyre!("Failed to initialize Silero VAD: {}", e))?;
        Ok(Self {
            provider,
            audio_capture: Some(audio_capture),
            context_hint,
            silence_timeout: silence_timeout
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_SILENCE_TIMEOUT),
            max_session_time: max_session_time
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_MAX_SESSION_TIME),
            vad: Some(vad),
            remote_base_url: None,
            model_size: None,
        })
    }

    pub fn supports_streaming(&self) -> bool {
        self.provider.supports_streaming()
    }

    fn update_status_line(elapsed: f32, activity_level: u8, transcript: &str) {
        const VOICE_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        const HINTS: &[&str] = &[
            "ENTER to stop",
            "hold Space for quick PTT",
            "set voice.autoSubmit to skip the review step",
        ];
        let bar = VOICE_CHARS[activity_level.min(7) as usize];
        let hint_idx = (elapsed as usize / 3) % HINTS.len();
        let hint = HINTS[hint_idx];
        let display = if !transcript.is_empty() {
            let t = if transcript.chars().count() > 50 {
                let start = transcript.char_indices().rev().nth(46).map(|(i, _)| i).unwrap_or(0);
                format!("...{}", &transcript[start..])
            } else {
                transcript.to_string()
            };
            format!("{t}  \x1B[2m{hint}\x1B[0m")
        } else {
            format!("Recording...  \x1B[2m{hint}\x1B[0m")
        };
        // Green volume bar + elapsed time + display
        let _ = write!(io::stdout(), "\r\x1B[32m{bar}\x1B[0m  {elapsed:.1}s  {display}\x1B[K");
        io::stdout().flush().ok();
    }

    /// Check if this handler uses a remote server (no local audio capture)
    pub fn is_remote(&self) -> bool {
        self.audio_capture.is_none()
    }

    /// For remote server mode: delegate recording to the remote server via SSE streaming,
    /// showing real-time volume bar driven by actual audio activity from the Mac.
    pub async fn listen_remote(&mut self) -> Result<Option<String>> {
        use super::providers::remote_server::RemoteServerProvider;

        let base_url = self
            .remote_base_url
            .clone()
            .ok_or_else(|| eyre::eyre!("listen_remote called on non-remote handler"))?;

        let remote = RemoteServerProvider::with_model_size(&base_url, self.model_size.clone())
            .map_err(|e| eyre::eyre!("{e}"))?;

        use std::io::IsTerminal;
        let is_piped = !std::io::stdout().is_terminal();

        chime_start();
        if is_piped {
            emit_voice_event("status", serde_json::json!("recording"));
        } else {
            Self::update_status_line(0.0, 0, "");
        }

        let start = Instant::now();
        let (activity_tx, mut activity_rx) = mpsc::channel::<u8>(200);
        let context_hint = self.context_hint.clone();

        // Run SSE stream in a task — it sends activity levels and returns final text
        let handle = tokio::spawn(async move { remote.record_streaming(context_hint, activity_tx).await });

        let mut last_level = 0u8;
        let mut last_display = Instant::now();

        let result = loop {
            tokio::select! {
                level = activity_rx.recv() => {
                    match level {
                        Some(l) => {
                            last_level = l;
                            // Throttle display updates to ~10 fps
                            if last_display.elapsed().as_millis() >= 100 {
                                if is_piped {
                                    emit_voice_event("level", serde_json::json!(last_level));
                                } else {
                                    Self::update_status_line(start.elapsed().as_secs_f32(), last_level, "");
                                }
                                last_display = Instant::now();
                            }
                        },
                        None => {
                            // activity_tx dropped = recording task finished
                            break match handle.await {
                                Ok(Ok(text)) => Ok(text),
                                Ok(Err(e)) => Err(eyre::eyre!("{e}")),
                                Err(e) => Err(eyre::eyre!("Task error: {e}")),
                            };
                        },
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    last_level = last_level.saturating_sub(1);
                    if is_piped {
                        emit_voice_event("level", serde_json::json!(last_level));
                    } else {
                        Self::update_status_line(start.elapsed().as_secs_f32(), last_level, "");
                    }
                    last_display = Instant::now();
                }
            }
        };

        if !is_piped {
            let _ = write!(io::stdout(), "\r\x1B[K\n");
            io::stdout().flush().ok();
        }
        chime_stop();

        match result {
            Ok(text) => Ok(text),
            Err(e) => Err(e),
        }
    }

    pub async fn listen_for_speech_streaming(&mut self) -> Result<Option<String>> {
        if !self.supports_streaming() {
            return self.listen_for_speech(false).await;
        }

        let audio_capture = self
            .audio_capture
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No local audio capture available"))?;

        chime_start();
        Self::update_status_line(0.0, 0, "");

        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(1000);
        let _stream = audio_capture.start_capture(audio_tx)?;

        let (blob_tx, blob_rx) = mpsc::channel(1000);
        let (vad_tx, mut vad_rx) = mpsc::channel::<u8>(100);

        tokio::spawn(async move {
            let mut audio_rx = audio_rx;
            let mut current_level = 0u8;
            let mut vad = SileroVad::new().ok();

            while let Some(audio_chunk) = audio_rx.recv().await {
                if let Some(ref mut vad) = vad {
                    if vad.is_speech(&audio_chunk) {
                        current_level = 8;
                    } else {
                        current_level = current_level.saturating_sub(1);
                    }
                } else {
                    // Fallback to RMS if VAD init failed
                    let db = compute_db(&audio_chunk);
                    if db > VOICE_THRESHOLD_DB {
                        current_level = 8;
                    } else {
                        current_level = current_level.saturating_sub(1);
                    }
                }
                let _ = vad_tx.send(current_level).await;
                let audio_blob = super::streaming::AudioBlob { data: audio_chunk };
                if blob_tx.send(audio_blob).await.is_err() {
                    break;
                }
            }
        });

        let options = super::provider::TranscriptionOptions {
            context_hint: self.context_hint.clone(),
        };
        let mut transcript_receiver = self.provider.stream_transcribe(blob_rx, &options).await?;

        let mut current_transcript = String::new();
        let mut last_update = Instant::now();
        let recording_start = Instant::now();
        let mut last_speech_time = Instant::now();
        let mut last_activity_time = Instant::now();
        let mut display_timer = tokio::time::interval(Duration::from_millis(100));
        display_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut current_voice_level = 0u8;

        loop {
            tokio::select! {
                transcript_result = transcript_receiver.recv() => {
                    match transcript_result {
                        Some(transcript_event) => {
                            last_activity_time = Instant::now();
                            if !transcript_event.partial_text.trim().is_empty() {
                                last_speech_time = Instant::now();
                                if transcript_event.is_final {
                                    if !current_transcript.is_empty() {
                                        current_transcript.push(' ');
                                    }
                                    current_transcript.push_str(transcript_event.partial_text.trim());
                                }
                            }
                        },
                        None => break,
                    }
                }
                voice_level = vad_rx.recv() => {
                    if let Some(level) = voice_level {
                        current_voice_level = level;
                        if level > 0 { last_activity_time = Instant::now(); }
                    }
                }
                _ = display_timer.tick() => {
                    if last_update.elapsed() >= Duration::from_millis(100) {
                        Self::update_status_line(
                            recording_start.elapsed().as_secs_f32(),
                            current_voice_level,
                            &current_transcript,
                        );
                        last_update = Instant::now();
                    }
                    let past_grace = recording_start.elapsed() >= INITIAL_GRACE_PERIOD;
                    if past_grace && last_speech_time.elapsed() >= self.silence_timeout { break; }
                    if past_grace && last_activity_time.elapsed() >= self.silence_timeout * 2 { break; }
                    if recording_start.elapsed() >= self.max_session_time { break; }
                }
            }
        }

        chime_stop();
        let _ = writeln!(io::stdout());

        if current_transcript.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(current_transcript))
    }

    /// Like `listen_headless` but sends real-time activity levels (0–10) to `activity_tx`.
    /// Used by the SSE streaming endpoint so the remote client can show a volume bar.
    pub async fn listen_headless_with_activity(&mut self, activity_tx: mpsc::Sender<u8>) -> Result<Option<String>> {
        let audio_capture = self
            .audio_capture
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No local audio capture available"))?;

        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1000);
        let _stream = audio_capture.start_capture(audio_tx)?;

        let mut audio_buffer = Vec::new();
        let recording_start = Instant::now();
        let mut last_voice_time = Instant::now();
        let mut level: u8 = 0;

        while let Some(chunk) = audio_rx.recv().await {
            audio_buffer.extend_from_slice(&chunk);
            let is_speech = self
                .vad
                .as_mut()
                .map_or_else(|| compute_db(&chunk) > VOICE_THRESHOLD_DB, |v| v.is_speech(&chunk));
            if is_speech {
                last_voice_time = Instant::now();
                level = 8;
            } else {
                level = level.saturating_sub(1);
            }
            let _ = activity_tx.try_send(level);
            let past_grace = recording_start.elapsed() >= INITIAL_GRACE_PERIOD;
            if past_grace && last_voice_time.elapsed() >= self.silence_timeout && !audio_buffer.is_empty() {
                break;
            }
            if recording_start.elapsed() >= self.max_session_time {
                break;
            }
        }

        let duration = recording_start.elapsed();
        if audio_buffer.is_empty() || duration < MIN_RECORDING_DURATION {
            return Ok(None);
        }

        match self.process_batch_audio(&audio_buffer).await {
            Ok(transcript) if !transcript.trim().is_empty() => Ok(Some(transcript)),
            Ok(_) => Ok(None),
            Err(e) => {
                error!("Transcription failed: {}", e);
                Ok(None)
            },
        }
    }

    /// Record and transcribe without a TTY — auto-stops on silence only.
    /// Used by the HTTP voice server where there is no terminal attached.
    pub async fn listen_headless(&mut self) -> Result<Option<String>> {
        let audio_capture = self
            .audio_capture
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No local audio capture available"))?;

        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1000);
        let _stream = audio_capture.start_capture(audio_tx)?;

        let mut audio_buffer = Vec::new();
        let recording_start = Instant::now();
        let mut last_voice_time = Instant::now();

        while let Some(chunk) = audio_rx.recv().await {
            audio_buffer.extend_from_slice(&chunk);
            let is_speech = self
                .vad
                .as_mut()
                .map_or_else(|| compute_db(&chunk) > VOICE_THRESHOLD_DB, |v| v.is_speech(&chunk));
            if is_speech {
                last_voice_time = Instant::now();
            }
            let past_grace = recording_start.elapsed() >= INITIAL_GRACE_PERIOD;
            if past_grace && last_voice_time.elapsed() >= self.silence_timeout && !audio_buffer.is_empty() {
                break;
            }
            if recording_start.elapsed() >= self.max_session_time {
                break;
            }
        }

        let duration = recording_start.elapsed();
        if audio_buffer.is_empty() || duration < MIN_RECORDING_DURATION {
            return Ok(None);
        }

        match self.process_batch_audio(&audio_buffer).await {
            Ok(transcript) if !transcript.trim().is_empty() => Ok(Some(transcript)),
            Ok(_) => Ok(None),
            Err(e) => {
                error!("Transcription failed: {}", e);
                Ok(None)
            },
        }
    }

    pub async fn listen_for_speech(&mut self, ptt_mode: bool) -> Result<Option<String>> {
        let audio_capture = self
            .audio_capture
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No local audio capture available"))?;

        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(1000);
        let _stream = audio_capture.start_capture(audio_tx)?;

        let mut audio_buffer = Vec::new();
        let recording_start = Instant::now();
        let mut voice_activity_level = 0u8;
        let mut partial_transcript = String::new();
        let mut last_partial_time = Instant::now();
        let partial_interval = Duration::from_secs(5);

        // Clear current line on both stderr and stdout to remove any prior output
        // (e.g. model download progress bar on stderr). Then show the recording bar.
        let _ = write!(io::stderr(), "\r\x1B[K");
        io::stderr().flush().ok();
        let _ = write!(io::stdout(), "\r\x1B[K");
        io::stdout().flush().ok();

        chime_start();
        Self::update_status_line(0.0, 0, "");

        let (partial_tx, mut partial_rx) = mpsc::channel::<String>(1);
        let mut partial_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        use crossterm::event::{
            Event,
            EventStream,
            KeyCode,
            KeyModifiers,
        };
        use crossterm::terminal::enable_raw_mode;
        use futures::StreamExt;

        enable_raw_mode().ok();
        let _raw_guard = RawModeGuard;
        // Drain buffered events from the readline→recording transition so a
        // stale key (e.g. the 'V' from Shift+V) can't immediately stop recording.
        while crossterm::event::poll(Duration::from_millis(0)).unwrap_or(false) {
            let _ = crossterm::event::read();
        }
        let mut event_stream = EventStream::new();
        let mut last_voice_time = Instant::now();
        let mut cancelled = false;
        // PTT Space-release detection: only armed after the FIRST Space keypress arrives
        // in the recording loop (so Shift+V and delayed starts don't trigger immediately).
        let mut last_space_time: Option<Instant> = None;
        let mut space_seen = false;

        loop {
            tokio::select! {
                event = event_stream.next() => {
                    if let Some(Ok(Event::Key(k))) = event {
                        if k.code == KeyCode::Enter {
                            break;
                        }
                        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                            cancelled = true;
                            break;
                        }
                        // In PTT mode, Space key repeats mean the key is still held
                        if ptt_mode && k.code == KeyCode::Char(' ') {
                            last_space_time = Some(Instant::now());
                            space_seen = true;
                        }
                    }
                }
                partial_result = partial_rx.recv() => {
                    if let Some(text) = partial_result {
                        partial_transcript = text;
                    }
                }
                audio_chunk = audio_rx.recv() => {
                    match audio_chunk {
                        Some(chunk) => {
                            audio_buffer.extend_from_slice(&chunk);
                            if self.vad.as_mut().is_some_and(|v| v.is_speech(&chunk)) {
                                last_voice_time = Instant::now();
                                voice_activity_level = 8;
                            } else {
                                voice_activity_level = voice_activity_level.saturating_sub(1);
                            }
                            Self::update_status_line(
                                recording_start.elapsed().as_secs_f32(),
                                voice_activity_level,
                                &partial_transcript,
                            );

                            // Trigger partial transcription every 5s
                            if last_partial_time.elapsed() >= partial_interval && audio_buffer.len() > 32000 {
                                last_partial_time = Instant::now();
                                let pcm_snapshot: Vec<f32> = audio_buffer
                                    .chunks_exact(2)
                                    .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                                    .collect();
                                let ptx = partial_tx.clone();
                                let model_path = self.provider.model_path();
                                if let Some(model_path) = model_path {
                                    let handle = tokio::task::spawn_blocking(move || {
                                        if let Ok(text) = super::providers::local_whisper::LocalWhisperProvider::transcribe_audio_static(&model_path, &pcm_snapshot, None)
                                            && !text.is_empty() {
                                                let _ = ptx.blocking_send(text);
                                            }
                                    });
                                    partial_tasks.push(handle);
                                }
                            }

                            // PTT Space-release: only check once we've seen at least one
                            // Space repeat (so Shift+V and delayed starts aren't affected)
                            if space_seen
                                && let Some(t) = last_space_time
                                && t.elapsed() > Duration::from_millis(200)
                            {
                                break;
                            }
                            let past_grace = recording_start.elapsed() >= INITIAL_GRACE_PERIOD;
                            if past_grace && last_voice_time.elapsed() >= self.silence_timeout && !audio_buffer.is_empty() {
                                break;
                            }
                            if recording_start.elapsed() >= self.max_session_time { break; }
                        },
                        None => break,
                    }
                }
            }
        }

        // Clean shutdown: drop event stream first, then raw mode guard restores terminal
        drop(event_stream);
        drop(_raw_guard);
        chime_stop();
        // Clear recording bar — use write! instead of print! to avoid
        // panic on broken pipe (e.g. Ctrl+C during recording)
        if ptt_mode {
            let _ = write!(io::stdout(), "\r\x1B[K\x1B[1A");
        } else {
            let _ = write!(io::stdout(), "\r\x1B[K");
        }
        io::stdout().flush().ok();

        // Abort any in-flight partial transcription tasks so they don't compete
        // with the final transcription for CPU/memory (each loads the Whisper model)
        for handle in partial_tasks {
            handle.abort();
        }
        drop(partial_tx);

        if cancelled {
            return Ok(None);
        }

        // Don't process very short recordings
        let duration = recording_start.elapsed();
        if audio_buffer.is_empty() || duration < MIN_RECORDING_DURATION {
            return Ok(None);
        }

        match self.process_batch_audio(&audio_buffer).await {
            Ok(transcript) if !transcript.trim().is_empty() => Ok(Some(transcript)),
            Ok(_) => Ok(None),
            Err(e) => {
                error!("Transcription failed: {}", e);
                Ok(None)
            },
        }
    }

    async fn process_batch_audio(&self, audio_data: &[u8]) -> Result<String> {
        let (blob_tx, blob_rx) = mpsc::channel(1);
        let _ = blob_tx
            .send(super::streaming::AudioBlob {
                data: audio_data.to_vec(),
            })
            .await;
        drop(blob_tx);

        let options = super::provider::TranscriptionOptions {
            context_hint: self.context_hint.clone(),
        };
        let mut rx = self
            .provider
            .stream_transcribe(blob_rx, &options)
            .await
            .map_err(|e| eyre::eyre!("Stream transcribe failed: {}", e))?;

        let deadline = Instant::now() + Duration::from_secs(60);
        let mut last_partial = String::new();
        let mut final_text: Option<String> = None;

        while Instant::now() < deadline {
            match timeout(Duration::from_secs(30), rx.recv()).await {
                Ok(Some(evt)) => {
                    let p = evt.partial_text.trim();
                    if !p.is_empty() {
                        last_partial = p.to_string();
                    }
                    if evt.is_final {
                        final_text = if !p.is_empty() {
                            Some(p.to_string())
                        } else if !last_partial.is_empty() {
                            Some(last_partial.clone())
                        } else {
                            None
                        };
                        break;
                    }
                },
                Ok(None) | Err(_) => break,
            }
        }

        Ok(final_text.unwrap_or(last_partial))
    }

    pub async fn check_setup(&self) -> Result<()> {
        if self.is_remote() {
            // Remote server mode — no local mic needed
            return Ok(());
        }
        super::audio_capture::request_microphone_permission()
            .map_err(|e| VoiceError::AudioProcessingError(format!("Microphone check failed: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_db_silence_returns_negative() {
        let silence = vec![0u8; 100];
        assert_eq!(compute_db(&silence), -100.0);
    }

    #[test]
    fn compute_db_empty_returns_floor() {
        assert_eq!(compute_db(&[]), -100.0);
        assert_eq!(compute_db(&[0]), -100.0); // less than 2 bytes
    }

    #[test]
    fn compute_db_max_amplitude() {
        // i16::MAX = 32767, as little-endian bytes
        let max_sample: Vec<u8> = i16::MAX.to_le_bytes().repeat(10);
        let db = compute_db(&max_sample);
        // Should be close to 0 dB (full scale)
        assert!(db > -1.0 && db <= 0.0, "got {db}");
    }

    #[test]
    fn compute_db_moderate_signal() {
        // ~50% amplitude
        let sample: Vec<u8> = 16384i16.to_le_bytes().repeat(10);
        let db = compute_db(&sample);
        // Should be around -6 dB
        assert!(db > -7.0 && db < -5.0, "got {db}");
    }
}
