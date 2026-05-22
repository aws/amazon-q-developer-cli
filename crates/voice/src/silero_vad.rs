use tracing::debug;
use voice_activity_detector::VoiceActivityDetector;

const SAMPLE_RATE: i64 = 16000;
const CHUNK_SIZE: usize = 512; // Required by Silero VAD V5 at 16kHz
const SPEECH_THRESHOLD: f32 = 0.5;

/// Wrapper around Silero VAD for speech detection on PCM i16 audio at 16kHz.
pub struct SileroVad {
    vad: VoiceActivityDetector,
    buffer: Vec<i16>,
}

impl SileroVad {
    pub fn new() -> Result<Self, voice_activity_detector::Error> {
        let vad = VoiceActivityDetector::builder()
            .sample_rate(SAMPLE_RATE)
            .chunk_size(CHUNK_SIZE)
            .build()?;
        Ok(Self {
            vad,
            buffer: Vec::with_capacity(CHUNK_SIZE),
        })
    }

    /// Feed raw PCM i16 LE bytes and return whether speech was detected in any chunk.
    pub fn is_speech(&mut self, pcm_bytes: &[u8]) -> bool {
        let samples: Vec<i16> = pcm_bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();

        self.buffer.extend_from_slice(&samples);

        let mut speech = false;
        while self.buffer.len() >= CHUNK_SIZE {
            let chunk: Vec<i16> = self.buffer.drain(..CHUNK_SIZE).collect();
            let prob = self.vad.predict(chunk);
            if prob >= SPEECH_THRESHOLD {
                speech = true;
                debug!("VAD speech detected: {:.2}", prob);
            }
        }
        speech
    }
}
