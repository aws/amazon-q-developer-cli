# Voice Mode — Complete Reference

## Invocation

| Method | Description |
|--------|-------------|
| `/voice` | Single recording session |
| `/voice --continuous` | Keep listening between turns (toggle on/off) |
| `/voice --model aws-transcribe` | Use AWS Transcribe instead of local whisper |
| `Ctrl+O` | Push-to-talk shortcut from prompt |

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `voice.modelSize` | `base` | Whisper model: `tiny`, `base`, `small`, `medium`, `turbo`, `turbo-q5` |
| `voice.silenceTimeout` | `3` | Seconds of silence before auto-stop |
| `voice.maxSessionTime` | `60` | Maximum recording duration in seconds |
| `voice.language` | `en` | Voice input language |

Configure with: `cargo run --bin chat_cli -- settings voice.modelSize turbo-q5`

## Models

| Size | File | Download | Notes |
|------|------|----------|-------|
| `tiny` | `ggml-tiny.en.bin` | ~75MB | Fastest, lowest accuracy |
| `base` | `ggml-base.en.bin` | ~142MB | Default, good balance |
| `small` | `ggml-small.en.bin` | ~466MB | Better accuracy |
| `medium` | `ggml-medium.en.bin` | ~1.5GB | High accuracy |
| `turbo` | `ggml-large-v3-turbo.bin` | ~1.62GB | Best quality, 6x faster than large-v3, multilingual |
| `turbo-q5` | `ggml-large-v3-turbo-q5_0.bin` | ~574MB | Quantized turbo, best quality/size tradeoff |

Models auto-download to `~/Library/Application Support/kiro/models/` (macOS) or `~/.local/share/kiro/models/` (Linux) on first use with a progress bar.

## Architecture

```
Microphone → cpal (audio capture, 16kHz mono PCM)
    ↓
Silero VAD V5 (neural speech detection, 512-sample windows)
    ↓ speech detected? reset silence timer
    ↓ no speech for 3s? → auto-stop
    ↓
Whisper (local) or AWS Transcribe (cloud)
    ↓
Transcribed text → auto-submitted to chat
```

## Voice Activity Detection

- Silero VAD V5 neural model via `voice_activity_detector` crate
- Processes 512-sample windows at 16kHz, returns speech probability (0.0–1.0)
- Speech threshold: probability >= 0.5
- Rejects keyboard clicks, fan noise, background sounds
- Falls back to RMS-based detection if VAD init fails (streaming mode)

```
AUDIO INPUT (microphone)
│
│  "Hey Kiro, fix the bug"  [keyboard clicks]  [fan noise]  [silence]
│   ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓   ░▒░▒░░░░░░░░░░░   ░░░░░░░░░   ░░░░░░░░
│
│  OLD: RMS Energy (just measures loudness)
│  ────────────────────────────────────────
│  dB: ████████████████████   ██░██░░░░░░░░░░   ░░░░░░░░░   ░░░░░░░░
│      -20dB (LOUD)          -38dB (LOUD-ish)   -55dB       -80dB
│       → SPEECH ✓            → SPEECH ✗✗✗       → silence   → silence
│                             (false positive!)
│
│  NEW: Silero VAD Neural Model (recognizes human voice)
│  ─────────────────────────────────────────────────────
│  Audio → [512 samples] → 🧠 Silero V5 → probability (0.0–1.0)
│
│  prob: ████████████████████   ░░░░░░░░░░░░░░░   ░░░░░░░░░   ░░░░░░░░
│        0.95 (SPEECH)         0.08 (not voice)   0.02        0.01
│         → SPEECH ✓            → IGNORED ✓        → silence   → silence
│
│  SILENCE AUTO-STOP:
│  Speaking → Speaking → Stop talking → 3s silence → ✂️ AUTO-STOP
│  (reset)    (reset)    (start timer)  (expired)    → transcribe
```

## Streaming Partial Transcription

- Every 5 seconds during recording, whisper runs on accumulated audio
- Partial transcription shown in the status line while recording
- Final full transcription runs on complete audio after recording stops

## Recording UI

```
🎤 Recording... 7.1sec ██████░░░░ fix the bug in...
```
- Elapsed time, voice activity bar, partial transcript
- Terminal bell chime on start/stop
- Press ENTER to stop early, Ctrl+C to cancel

## First-Use Experience

- Help box shown once: model info, keybind reminder
- Model downloads with progress bar on first use
- Marked as seen via `voice.language` setting

## Transcription Backends

1. **Local Whisper** (default) — `whisper-rs` (whisper.cpp bindings), runs on CPU, no internet after model download
2. **AWS Transcribe** (`--model aws-transcribe`) — real-time streaming, requires AWS credentials with `transcribe:StartStreamTranscription`

## Dependencies

| Crate | Purpose |
|-------|---------|
| `cpal` 0.15.3 | Cross-platform audio capture |
| `whisper-rs` 0.13.0 | Whisper.cpp Rust bindings |
| `voice_activity_detector` 0.2.1 | Silero VAD V5 neural speech detection |
| `aws-sdk-transcribestreaming` | AWS Transcribe streaming |

## Build Requirements

- C/C++ compiler (GCC 10+ on Linux, Xcode on macOS)
- CMake 3.14+
- libclang (for bindgen)
- ALSA dev headers on Linux (`alsa-lib-devel`)

## Files

```
crates/chat-cli/src/cli/chat/
├── cli/voice.rs                    # /voice slash command handler
├── voice/
│   ├── mod.rs                      # Module exports
│   ├── audio_capture.rs            # cpal microphone capture
│   ├── silero_vad.rs               # Silero VAD wrapper
│   ├── voice_handler.rs            # Recording loop, VAD, auto-stop
│   ├── voice_display.rs            # Status line UI
│   ├── provider.rs                 # TranscriptionProvider trait
│   ├── transcription_provider.rs   # Backend enum
│   ├── streaming.rs                # Audio/transcription stream types
│   ├── error.rs                    # VoiceError types
│   ├── common.rs                   # Shared types
│   └── providers/
│       ├── local_whisper.rs        # Whisper.cpp provider
│       ├── aws.rs                  # AWS Transcribe provider
│       └── mod.rs
docs/voice-mode-design.md           # Full design document
```

## Improvements Roadmap

### Done
- [x] Distil-Whisper models (turbo + turbo-q5)
- [x] Silero VAD neural speech detection
- [x] Configurable timeouts
- [x] Model download progress bar
- [x] Streaming local transcription

### High Priority
- [ ] CoreML / GPU acceleration
- [ ] AI text cleanup (LLM pass to remove filler words)

### Medium Priority
- [ ] Editable transcription (voice.autoSubmit setting)
- [ ] Quantized models (8-bit)

### Lower Priority
- [ ] Multilingual support
- [ ] Wake word ("Hey Kiro")
- [ ] Word-level timestamps
