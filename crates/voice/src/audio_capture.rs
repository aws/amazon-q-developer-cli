use cpal::traits::{
    DeviceTrait,
    HostTrait,
    StreamTrait,
};
use cpal::{
    Device,
    Stream,
    StreamConfig,
};
use eyre::Result;
use tokio::sync::mpsc;
use tracing::{
    debug,
    error,
    warn,
};

use super::VoiceError;

pub struct AudioCapture {
    device: Device,
    config: StreamConfig,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(VoiceError::MicrophoneUnavailable)?;

        debug!("Using audio device: {}", device.name().unwrap_or_default());

        let supported_config = device
            .default_input_config()
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;

        debug!("Device default config: {:?}", supported_config);

        let config = StreamConfig {
            channels: supported_config.channels(),
            sample_rate: supported_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        debug!("Using exact device config for maximum compatibility: {:?}", config);

        Ok(Self { device, config })
    }

    /// Resample mono f32 audio to exactly 16 kHz using linear interpolation
    fn resample_mono_to_16k(mono: &[f32], in_rate: u32) -> Vec<f32> {
        if mono.is_empty() || in_rate == 16000 {
            return mono.to_vec();
        }

        let in_len = mono.len();
        let out_len = ((in_len as u64 * 16000) / in_rate as u64).max(1) as usize;
        let scale = in_rate as f64 / 16000.0;

        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 * scale;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;

            let s0 = mono[idx.min(in_len - 1)];
            let s1 = mono[(idx + 1).min(in_len - 1)];
            out.push(s0 + (s1 - s0) * frac);
        }
        out
    }

    pub fn start_capture(&self, audio_sender: mpsc::Sender<Vec<u8>>) -> Result<Stream> {
        let config = self.config.clone();

        let supported_config = self
            .device
            .default_input_config()
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;

        let stream = match supported_config.sample_format() {
            cpal::SampleFormat::F32 => self.build_input_stream_f32(&config, audio_sender)?,
            cpal::SampleFormat::I16 => self.build_input_stream_i16(&config, audio_sender)?,
            cpal::SampleFormat::U16 => self.build_input_stream_u16(&config, audio_sender)?,
            sample_format => {
                error!("Unsupported sample format: {:?}", sample_format);
                return Err(VoiceError::UnsupportedAudioFormat.into());
            },
        };

        stream
            .play()
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;

        debug!("Audio capture started successfully with native device format");
        Ok(stream)
    }

    fn build_input_stream_f32(&self, config: &StreamConfig, sender: mpsc::Sender<Vec<u8>>) -> Result<Stream> {
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;

        debug!("Building F32 stream: {} channels, {} Hz", channels, sample_rate);

        let stream = self
            .device
            .build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mono_data: Vec<f32> = if channels == 1 {
                        data.to_vec()
                    } else {
                        data.chunks(channels)
                            .map(|f| f.iter().sum::<f32>() / channels as f32)
                            .collect()
                    };

                    let resampled = Self::resample_mono_to_16k(&mono_data, sample_rate);

                    let pcm_data: Vec<i16> = resampled
                        .iter()
                        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                        .collect();

                    let bytes: Vec<u8> = pcm_data.iter().flat_map(|&sample| sample.to_le_bytes()).collect();

                    if let Err(e) = sender.try_send(bytes) {
                        match e {
                            mpsc::error::TrySendError::Full(_) => {
                                warn!("Audio buffer full, dropping audio data");
                            },
                            mpsc::error::TrySendError::Closed(_) => {
                                debug!("Audio channel closed");
                            },
                        }
                    }
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;

        Ok(stream)
    }

    fn build_input_stream_i16(&self, config: &StreamConfig, sender: mpsc::Sender<Vec<u8>>) -> Result<Stream> {
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;

        debug!("Building I16 stream: {} channels, {} Hz", channels, sample_rate);

        let stream = self
            .device
            .build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mono_i16: Vec<i16> = if channels == 1 {
                        data.to_vec()
                    } else {
                        data.chunks(channels)
                            .map(|f| (f.iter().map(|&x| x as i32).sum::<i32>() / channels as i32) as i16)
                            .collect()
                    };

                    let mono_f32: Vec<f32> = mono_i16.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let resampled = Self::resample_mono_to_16k(&mono_f32, sample_rate);
                    let pcm_data: Vec<i16> = resampled
                        .iter()
                        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                        .collect();

                    let bytes: Vec<u8> = pcm_data.iter().flat_map(|&sample| sample.to_le_bytes()).collect();

                    if let Err(e) = sender.try_send(bytes) {
                        match e {
                            mpsc::error::TrySendError::Full(_) => {
                                warn!("Audio buffer full, dropping audio data");
                            },
                            mpsc::error::TrySendError::Closed(_) => {
                                debug!("Audio channel closed");
                            },
                        }
                    }
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;

        Ok(stream)
    }

    fn build_input_stream_u16(&self, config: &StreamConfig, sender: mpsc::Sender<Vec<u8>>) -> Result<Stream> {
        let channels = config.channels as usize;
        let sample_rate = config.sample_rate.0;

        debug!("Building U16 stream: {} channels, {} Hz", channels, sample_rate);

        let stream = self
            .device
            .build_input_stream(
                config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mono_i16: Vec<i16> = if channels == 1 {
                        data.iter().map(|&u| (u as i32 - 32768) as i16).collect()
                    } else {
                        data.chunks(channels)
                            .map(|f| {
                                let avg = f.iter().map(|&x| x as i32).sum::<i32>() / channels as i32;
                                (avg - 32768) as i16
                            })
                            .collect()
                    };

                    let mono_f32: Vec<f32> = mono_i16.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let resampled = Self::resample_mono_to_16k(&mono_f32, sample_rate);
                    let pcm_data: Vec<i16> = resampled
                        .iter()
                        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                        .collect();

                    let bytes: Vec<u8> = pcm_data.iter().flat_map(|&sample| sample.to_le_bytes()).collect();

                    if let Err(e) = sender.try_send(bytes) {
                        match e {
                            mpsc::error::TrySendError::Full(_) => {
                                warn!("Audio buffer full, dropping audio data");
                            },
                            mpsc::error::TrySendError::Closed(_) => {
                                debug!("Audio channel closed");
                            },
                        }
                    }
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;

        Ok(stream)
    }

    pub fn check_permissions() -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(VoiceError::MicrophoneUnavailable)?;
        let _config = device
            .default_input_config()
            .map_err(|e| VoiceError::AudioProcessingError(e.to_string()))?;
        Ok(())
    }
}

pub fn request_microphone_permission() -> Result<bool> {
    AudioCapture::check_permissions().map(|_| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_passthrough_at_16k() {
        let input = vec![0.1, 0.2, 0.3, 0.4];
        let output = AudioCapture::resample_mono_to_16k(&input, 16000);
        assert_eq!(output, input);
    }

    #[test]
    fn resample_empty_input() {
        let output = AudioCapture::resample_mono_to_16k(&[], 48000);
        assert!(output.is_empty());
    }

    #[test]
    fn resample_downsample_48k_to_16k() {
        // 48kHz → 16kHz = 3:1 ratio, so 90 samples → 30 samples
        let input: Vec<f32> = (0..90).map(|i| (i as f32) / 90.0).collect();
        let output = AudioCapture::resample_mono_to_16k(&input, 48000);
        assert_eq!(output.len(), 30);
        // First sample should be close to input[0]
        assert!((output[0] - input[0]).abs() < 0.01);
    }

    #[test]
    fn resample_upsample_8k_to_16k() {
        // 8kHz → 16kHz = 1:2 ratio, so 10 samples → 20 samples
        let input: Vec<f32> = (0..10).map(|i| i as f32 * 0.1).collect();
        let output = AudioCapture::resample_mono_to_16k(&input, 8000);
        assert_eq!(output.len(), 20);
    }
}
