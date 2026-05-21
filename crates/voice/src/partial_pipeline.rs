//! Partial transcription pipeline for PTT (push-to-talk) mode.
//!
//! Manages incremental transcription with committed stable text and trimmed
//! trailing silence to prevent hallucinations.

use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::{
    Duration,
    Instant,
};

use tokio::sync::mpsc;
use whisper_rs::WhisperContext;

use crate::error::VoiceResult;
use crate::providers::local_whisper::{
    PartialResult,
    transcribe_with_segments,
};

// === Constants ===

const SAMPLE_RATE: u32 = 16_000;
const BYTES_PER_SAMPLE: usize = 2; // i16 LE

// VAD debounce (time-based)
const SILENCE_DEBOUNCE_MS: u64 = 256; // ~8 VAD frames

// Partial firing
const PARTIAL_PAUSE_MS: u64 = 500; // silence before partial fires
const MIN_NEW_AUDIO_MS: u64 = 800; // skip partials on tiny chunks
const MIN_PARTIAL_INTERVAL_MS: u64 = 1500; // hard rate limit

// Safety bounds
const TAIL_PADDING_MS: u64 = 200; // keep this much after last_speech
const MIN_RECORDING_MS: u64 = 500; // below this, don't transcribe at all

// Whisper prompt
const PROMPT_MAX_WORDS: usize = 150;

// === VAD State ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VadState {
    Idle,
    Speaking,
    Paused,
}

// === Pipeline ===

pub struct PartialPipeline {
    // Model (shared, loaded once)
    ctx: Arc<WhisperContext>,
    inference_lock: Arc<Mutex<()>>,

    // Audio
    audio_buffer: Vec<u8>, // 16 kHz, mono, i16 LE
    recording_start: Instant,
    language: Option<String>,

    // VAD state
    state: VadState,
    last_speech_instant: Instant,
    silence_start: Option<Instant>,

    // Partial tracking
    latest_gen: Arc<AtomicU64>,
    in_flight_gen: Option<u64>,
    in_flight_audio_end: Option<usize>,
    partial_tx: mpsc::Sender<(u64, VoiceResult<PartialResult>)>,
    partial_rx: mpsc::Receiver<(u64, VoiceResult<PartialResult>)>,

    // Committed state (never re-transcribed)
    stable_prefix: String,
    stable_audio_end: usize,

    // Display / stability
    last_partial_result: Option<String>,
    display_text: String,

    // Rate limiting
    last_partial_fired: Instant,
}

impl PartialPipeline {
    pub fn new(ctx: Arc<WhisperContext>, language: Option<String>) -> Self {
        let (partial_tx, partial_rx) = mpsc::channel(4);
        let now = Instant::now();
        Self {
            ctx,
            inference_lock: Arc::new(Mutex::new(())),
            audio_buffer: Vec::new(),
            recording_start: now,
            language,
            state: VadState::Idle,
            last_speech_instant: now,
            silence_start: None,
            latest_gen: Arc::new(AtomicU64::new(0)),
            in_flight_gen: None,
            in_flight_audio_end: None,
            partial_tx,
            partial_rx,
            stable_prefix: String::new(),
            stable_audio_end: 0,
            last_partial_result: None,
            display_text: String::new(),
            last_partial_fired: now,
        }
    }

    /// Push a new audio chunk, update VAD state. Returns true if speech was detected.
    pub fn push_chunk(&mut self, chunk: &[u8], is_speech: bool) -> bool {
        self.audio_buffer.extend_from_slice(chunk);
        let now = Instant::now();

        match self.state {
            VadState::Idle => {
                if is_speech {
                    self.state = VadState::Speaking;
                    self.last_speech_instant = now;
                }
            },
            VadState::Speaking => {
                if is_speech {
                    self.last_speech_instant = now;
                    self.silence_start = None;
                } else if self.silence_start.is_none() {
                    self.silence_start = Some(now);
                } else if let Some(s) = self.silence_start
                    && s.elapsed() >= Duration::from_millis(SILENCE_DEBOUNCE_MS)
                {
                    self.state = VadState::Paused;
                }
            },
            VadState::Paused => {
                if is_speech {
                    self.state = VadState::Speaking;
                    self.last_speech_instant = now;
                    self.silence_start = None;
                }
            },
        }

        is_speech
    }

    /// Check if a partial should fire and dispatch it if so.
    /// Returns new display text if a partial result was received.
    pub fn try_fire_partial(&mut self) -> Option<String> {
        // First, check for completed partials
        let display_change = self.poll_partial_results();

        // Fire new partial if conditions are met
        if self.should_fire_partial() {
            self.fire_partial();
        }

        display_change
    }

    /// Poll for completed partial results without blocking.
    pub fn poll_partial_results(&mut self) -> Option<String> {
        match self.partial_rx.try_recv() {
            Ok((generation, result)) => {
                self.handle_partial_result(generation, result);
                if !self.display_text.is_empty() {
                    Some(self.display_text.clone())
                } else {
                    None
                }
            },
            Err(_) => None,
        }
    }

    /// Finalize transcription on stop. Returns the final text.
    pub async fn finalize(mut self) -> String {
        let bytes_per_ms = (SAMPLE_RATE as usize * BYTES_PER_SAMPLE) / 1000;

        // Try to reuse an in-flight partial if it covers essentially all uncommitted audio
        if let Some(inflight_gen) = self.in_flight_gen {
            let partial_covered = self.in_flight_audio_end.unwrap_or(self.stable_audio_end);
            let current_end = trim_end_to_last_speech(
                &self.audio_buffer,
                self.stable_audio_end,
                self.last_speech_instant,
                self.recording_start,
                TAIL_PADDING_MS,
            );
            let new_after_fire_ms = current_end.saturating_sub(partial_covered) / bytes_per_ms;

            if new_after_fire_ms < 150 {
                // In-flight partial covers ~everything. Wait for it.
                if let Ok(Some((g, Ok(partial)))) =
                    tokio::time::timeout(Duration::from_millis(400), self.partial_rx.recv()).await
                    && g == inflight_gen
                {
                    let text = filter_hallucinations(&partial.text, true);
                    return combine_final(&self.stable_prefix, &text);
                }
            }
            // Too much new audio, OR timeout. Invalidate the partial.
            self.latest_gen.fetch_add(1_000_000, Ordering::SeqCst);
        }

        // Fresh transcription of the uncommitted region (trimmed).
        let audio_end = trim_end_to_last_speech(
            &self.audio_buffer,
            self.stable_audio_end,
            self.last_speech_instant,
            self.recording_start,
            TAIL_PADDING_MS,
        );
        let uncommitted = &self.audio_buffer[self.stable_audio_end..audio_end];

        if uncommitted.len() < MIN_RECORDING_MS as usize * bytes_per_ms {
            return self.stable_prefix.trim().to_string();
        }

        let pcm_f32: Vec<f32> = uncommitted
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect();
        let prompt = self.build_prompt();
        let ctx = Arc::clone(&self.ctx);
        let lock = Arc::clone(&self.inference_lock);
        let stable_prefix = self.stable_prefix.clone();
        let lang = self.language.clone();

        let result = tokio::task::spawn_blocking(move || {
            let _guard = lock.lock().unwrap();
            transcribe_with_segments(&ctx, &pcm_f32, Some(&prompt), lang.as_deref())
        })
        .await;

        match result {
            Ok(Ok(partial)) => {
                let text = filter_hallucinations(&partial.text, true);
                combine_final(&stable_prefix, &text)
            },
            _ => stable_prefix.trim().to_string(),
        }
    }

    /// Get the current display text (stable_prefix + latest partial).
    pub fn display_text(&self) -> &str {
        &self.display_text
    }

    /// Whether we've detected speech at all.
    pub fn had_speech(&self) -> bool {
        self.state != VadState::Idle
    }

    pub fn audio_len(&self) -> usize {
        self.audio_buffer.len()
    }

    pub fn stable_prefix(&self) -> &str {
        &self.stable_prefix
    }

    // === Private methods ===

    fn should_fire_partial(&self) -> bool {
        if self.state != VadState::Paused {
            return false;
        }
        let Some(silence_start) = self.silence_start else {
            return false;
        };
        if silence_start.elapsed() < Duration::from_millis(PARTIAL_PAUSE_MS) {
            return false;
        }
        if self.last_partial_fired.elapsed() < Duration::from_millis(MIN_PARTIAL_INTERVAL_MS) {
            return false;
        }
        if self.in_flight_gen.is_some() {
            return false;
        }

        let new_bytes = self.audio_buffer.len().saturating_sub(self.stable_audio_end);
        let new_ms = new_bytes * 1000 / (BYTES_PER_SAMPLE * SAMPLE_RATE as usize);
        new_ms >= MIN_NEW_AUDIO_MS as usize
    }

    fn fire_partial(&mut self) {
        let generation = self.latest_gen.fetch_add(1, Ordering::SeqCst);
        self.in_flight_gen = Some(generation);
        self.last_partial_fired = Instant::now();

        // Trim trailing silence before sending to Whisper
        let audio_end = trim_end_to_last_speech(
            &self.audio_buffer,
            self.stable_audio_end,
            self.last_speech_instant,
            self.recording_start,
            TAIL_PADDING_MS,
        );
        self.in_flight_audio_end = Some(audio_end);

        let audio_slice = self.audio_buffer[self.stable_audio_end..audio_end].to_vec();
        let prompt = self.build_prompt();
        let ctx = Arc::clone(&self.ctx);
        let lock = Arc::clone(&self.inference_lock);
        let tx = self.partial_tx.clone();
        let latest_gen = Arc::clone(&self.latest_gen);
        let lang = self.language.clone();

        tokio::task::spawn_blocking(move || {
            // Bail early if we've already been superseded
            if generation + 1 < latest_gen.load(Ordering::SeqCst) {
                return;
            }

            let _guard = lock.lock().unwrap();

            // Re-check after lock
            if generation + 1 < latest_gen.load(Ordering::SeqCst) {
                return;
            }

            let pcm_f32: Vec<f32> = audio_slice
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                .collect();

            let result = transcribe_with_segments(&ctx, &pcm_f32, Some(&prompt), lang.as_deref());
            let _ = tx.blocking_send((generation, result));
        });
    }

    fn handle_partial_result(&mut self, generation: u64, result: VoiceResult<PartialResult>) {
        // Discard stale
        if Some(generation) != self.in_flight_gen {
            return;
        }
        self.in_flight_gen = None;

        let Ok(partial) = result else {
            // Even on error, advance the offset so we don't retry the same audio
            if let Some(end) = self.in_flight_audio_end {
                self.stable_audio_end = end;
            }
            self.in_flight_audio_end = None;
            return;
        };
        self.in_flight_audio_end = None;

        let text = filter_hallucinations(&partial.text, false);
        if text.is_empty() {
            // Advance offset past this chunk (it was silence/hallucination)
            if let Some(&(_, _, last_t1)) = partial.segments.last() {
                let commit_bytes = (last_t1 as usize * SAMPLE_RATE as usize * BYTES_PER_SAMPLE) / 100;
                self.stable_audio_end = (self.stable_audio_end + commit_bytes).min(self.audio_buffer.len());
            } else {
                // No segments at all — advance past the in-flight region
                self.stable_audio_end = self.in_flight_audio_end.unwrap_or(self.stable_audio_end);
            }
            return;
        }

        // Commit immediately — append new text directly.
        if self.stable_prefix.is_empty() {
            self.stable_prefix = text.trim().to_string();
        } else {
            self.stable_prefix = format!("{} {}", self.stable_prefix.trim(), text.trim());
        }

        // Advance audio offset using the last segment's end timestamp
        if let Some(&(_, _, last_t1)) = partial.segments.last() {
            let commit_bytes = (last_t1 as usize * SAMPLE_RATE as usize * BYTES_PER_SAMPLE) / 100;
            self.stable_audio_end = (self.stable_audio_end + commit_bytes).min(self.audio_buffer.len());
        }

        self.last_partial_result = Some(self.stable_prefix.clone());
        self.display_text = self.stable_prefix.clone();
    }

    fn build_prompt(&self) -> String {
        if self.stable_prefix.is_empty() {
            return String::new();
        }
        // Use the last N words of committed text as context.
        // No CODING_VOCAB — it causes English hallucinations during non-English speech.
        let tail_words: Vec<&str> = self
            .stable_prefix
            .split_whitespace()
            .rev()
            .take(PROMPT_MAX_WORDS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        tail_words.join(" ")
    }
}

// === Helper functions ===

fn trim_end_to_last_speech(
    audio_buffer: &[u8],
    stable_audio_end: usize,
    last_speech_instant: Instant,
    recording_start: Instant,
    tail_padding_ms: u64,
) -> usize {
    let bytes_per_ms = (SAMPLE_RATE as usize * BYTES_PER_SAMPLE) / 1000;

    let last_speech_ms = last_speech_instant
        .saturating_duration_since(recording_start)
        .as_millis() as usize;

    let trim_end_ms = last_speech_ms + tail_padding_ms as usize;
    let trim_end_bytes = trim_end_ms * bytes_per_ms;

    trim_end_bytes.max(stable_audio_end).min(audio_buffer.len())
}

fn combine_final(stable: &str, tail: &str) -> String {
    let s = stable.trim();
    let t = tail.trim();
    if s.is_empty() {
        t.to_string()
    } else if t.is_empty() {
        s.to_string()
    } else {
        format!("{} {}", s, t)
    }
}

fn filter_hallucinations(text: &str, _is_final: bool) -> String {
    text.trim().to_string()
}
