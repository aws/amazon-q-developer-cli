# Voice Mode for Kiro CLI — Design Document

**Author:** Sai Srinivas  
**Status:** In Development  
**Branch:** `voice-mode`  
**Date:** 2026-02-20

## Overview

Voice Mode enables hands-free, speech-to-text input for Kiro CLI. Users speak their prompts instead of typing them, and the transcribed text is submitted directly to the AI assistant. This is particularly valuable for:

- Rapid prototyping and brainstorming
- Accessibility for users who prefer voice input
- Multitasking while coding (walking, coffee, eyes resting)
- Reducing context-switching friction

## Prior Art & Competitive Analysis

| Tool | Recording | Transcription | Remote Support | UX |
|------|-----------|---------------|----------------|-----|
| **Aider** | sounddevice (local) | OpenAI Whisper API (cloud) | None | Single-line status, auto-submit |
| **VoiceMode (Claude Code)** | sounddevice via MCP | Local Whisper or OpenAI API | WebSocket relay (voicemode.dev) | Full conversation with TTS |
| **Cursor** | Browser/Electron | Deepgram | N/A (IDE is local) | Toggle in Labs settings |
| **WisprFlow** | OS-level | Local Whisper | Works over SSH (OS-level) | Global push-to-talk |
| **Kiro CLI (ours)** | cpal (Rust-native) | whisper-rs (local, Rust-native) | Planned (MCP) | Single-line status, auto-submit |

### Key Differentiators

1. **Zero external dependencies** — No Python, no cloud API keys required. whisper-rs compiles whisper.cpp into the binary. Model auto-downloads on first use.
2. **Context-aware transcription** — Conversation history is passed to Whisper's `initial_prompt` parameter, improving accuracy for code terms and identifiers discussed in the session.
3. **Rust-native** — Both audio capture (cpal) and transcription (whisper-rs) are pure Rust/C++, no subprocess overhead.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Kiro CLI Chat Loop                │
│                                                     │
│  /voice or Ctrl+O                                   │
│       │                                             │
│       ▼                                             │
│  ┌─────────────┐    ┌──────────────┐                │
│  │ VoiceHandler │───▶│ AudioCapture │ (cpal)         │
│  │             │    │  16kHz mono  │                │
│  │             │    └──────┬───────┘                │
│  │             │           │ PCM i16 bytes           │
│  │             │    ┌──────▼───────┐                │
│  │             │───▶│  Provider    │                │
│  │             │    │             │                │
│  │             │    │ ┌─────────┐ │                │
│  │             │    │ │whisper- │ │ (default)       │
│  │             │    │ │rs local │ │                │
│  │             │    │ └─────────┘ │                │
│  │             │    │ ┌─────────┐ │                │
│  │             │    │ │  AWS    │ │ (--model       │
│  │             │    │ │Transcr. │ │  aws-transcribe)│
│  │             │    │ └─────────┘ │                │
│  │             │    └──────┬───────┘                │
│  │             │           │ transcribed text        │
│  └──────┬──────┘           │                        │
│         │◀─────────────────┘                        │
│         │                                           │
│         ▼                                           │
│  ChatState::HandleInput { input: text }             │
│  (auto-submitted as user prompt)                    │
└─────────────────────────────────────────────────────┘
```

### Module Structure

```
crates/chat-cli/src/cli/chat/
├── voice/
│   ├── mod.rs                    # Module root, re-exports
│   ├── audio_capture.rs          # cpal microphone capture, resampling to 16kHz
│   ├── voice_handler.rs          # Main orchestrator: record → transcribe → return
│   ├── voice_display.rs          # Terminal UI components (reserved for future)
│   ├── error.rs                  # VoiceError enum
│   ├── streaming.rs              # StreamingTranscription, AudioBlob types
│   ├── provider.rs               # TranscriptionProvider trait, TranscriptionOptions
│   ├── transcription_provider.rs # TranscriptionBackend enum
│   ├── common.rs                 # Shared utilities
│   └── providers/
│       ├── mod.rs
│       ├── local_whisper.rs      # whisper-rs (whisper.cpp) — default provider
│       └── aws.rs                # AWS Transcribe streaming provider
└── cli/
    └── voice.rs                  # /voice slash command, VoiceArgs
```

## Features

### Implemented (v1)

#### 1. Local Whisper Transcription (Default)
- Uses `whisper-rs` crate (Rust bindings for whisper.cpp)
- Auto-downloads whisper model on first use to `~/.local/share/kiro/models/`
- Available models: `tiny` (~75MB), `base` (~142MB, default), `small` (~466MB), `medium` (~1.5GB), `turbo` (~1.62GB), `turbo-q5` (~574MB)
- Configure with `kiro-cli settings set voice.modelSize <size>`
- Runs transcription on a blocking thread (CPU-bound)
- No internet required after model download

#### 2. AWS Transcribe Streaming (Optional)
- Real-time streaming transcription via `aws-sdk-transcribestreaming`
- Requires AWS credentials with Transcribe permissions
- Activated with `--model aws-transcribe`

#### 3. Aider-Style Recording UI
- Single updating status line: `Recording, press ENTER when done... 3.5sec ░░░█████████`
- Voice activity visualization with proportional bar
- No verbose setup text or multi-line output

#### 4. Auto-Submit
- Transcribed text is immediately submitted as chat input
- No edit/cancel menu — frictionless like aider
- Transcription shown in grey quotes before submission

#### 5. Context-Aware Transcription
- Last 5 conversation transcript entries passed to Whisper's `initial_prompt`
- Improves accuracy for code identifiers, function names, and domain terms
- `ChatSession::voice_context_hint()` extracts and truncates context to ~500 chars

#### 6. Continuous Voice Mode
- `/voice --continuous` toggles auto-recording after each assistant response
- Chat loop auto-dispatches `/voice` when returning to prompt state
- Disables on: errors, no speech detected, or toggling off with `/voice --continuous`

#### 7. Push-to-Talk Keybind
- `Ctrl+O` instantly starts voice recording from the chat prompt
- Inserts and auto-submits `/voice` command
- No conflict with existing keybinds

### Implemented (v2)

#### 8. Remote/Cloud Desktop Support
Three-tier approach:
1. **Implemented:** Helpful error message suggesting OS dictation + remote voice server hint
2. **Implemented:** Voice HTTP server (`kiro-cli voice-serve`) that runs on the user's local machine, captures mic + transcribes, and exposes an HTTP API. Remote kiro-cli calls it when no local mic is detected.
3. **Planned:** Full MCP-based integration and two-way voice with TTS responses

#### 9. Voice Server Architecture (Cloud Desktop)

When running on a cloud desktop without a microphone, voice mode uses a client-server architecture:

```
┌─────────────────────┐       SSH reverse tunnel        ┌──────────────────────┐
│   User's Local Mac  │◀──────────────────────────────▶│   Cloud Desktop      │
│                     │                                 │                      │
│  kiro-cli           │   HTTP (port 19876)             │  kiro-cli            │
│  voice-serve        │◀────────────────────────────────│  /voice command      │
│                     │                                 │                      │
│  Endpoints:         │   POST /voice/record            │  Detects no mic      │
│  GET /voice/status  │   → records audio               │  Falls back to       │
│  POST /voice/record │   → transcribes via whisper     │  voice.serverUrl     │
│                     │   → returns { text: "..." }     │                      │
│  mic (cpal)         │                                 │  Receives text       │
│  whisper-rs         │                                 │  → ChatState::       │
│  Silero VAD         │                                 │    HandleInput       │
└─────────────────────┘                                 └──────────────────────┘
```

**Setup:**
```bash
# 1. On your local machine (with microphone):
kiro-cli voice-serve                    # starts HTTP server on localhost:19876

# 2. SSH to cloud desktop with reverse port forwarding:
ssh -R 19876:localhost:19876 cloud-desktop

# 3. On the cloud desktop, configure the voice server URL:
kiro-cli settings set voice.serverUrl http://localhost:19876

# 4. Use voice mode as usual:
/voice          # auto-detects no mic, falls back to remote server
/voice --model remote   # explicitly use remote server
Ctrl+O          # push-to-talk also works with fallback
```

**How the fallback works:**
1. `/voice` is triggered (via command or Ctrl+O)
2. `VoiceHandler::new()` tries to initialize local audio capture (cpal)
3. `check_setup()` detects no microphone available
4. If `voice.serverUrl` is configured, automatically falls back to `RemoteServer` backend
5. Makes HTTP POST to the remote voice server's `/voice/record` endpoint
6. Server records audio locally, transcribes with whisper-rs, returns text
7. Text is submitted as `ChatState::HandleInput` — same as local voice mode

**Settings:**
| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `voice.serverUrl` | string | none | URL of remote voice server (e.g., `http://localhost:19876`) |

**API Endpoints (voice-serve):**
| Method | Path | Description |
|--------|------|-------------|
| GET | `/voice/status` | Returns server status, mic availability, version |
| POST | `/voice/record` | Records audio, transcribes, returns `{ text: "..." }` |

**POST /voice/record request body (optional JSON):**
```json
{
  "context_hint": "recent conversation context for whisper",
  "language": "en",
  "model_size": "base"
}
```

#### 10. Configurable Whisper Model Size
- `kiro settings voice.model tiny|base|small|medium`
- Trade-off: speed vs accuracy
- Default: `base` (good balance)

#### 11. Configurable Recording Timeouts
- `voice.silenceTimeout` — seconds of silence before auto-stop (default: 3)
- `voice.maxSessionTime` — max recording duration in seconds (default: 300)

### Improvements Roadmap

#### High Priority
- [x] **Distil-Whisper models** — Add large-v3-turbo as `turbo` and `turbo-q5` model options. 6x faster, within 1% WER of full models.
- [x] **Silero VAD preprocessing** — Replace RMS-based energy detection with neural VAD (Silero). Eliminates whisper hallucinations on silence and reduces processing time by only transcribing speech segments.
- [ ] **CoreML / GPU acceleration** — Enable whisper-rs CoreML feature flag on macOS and CUDA/Metal on Linux. On Apple Silicon, CoreML makes base/small models near-instant.
- [ ] **AI text cleanup** — Post-process transcriptions with a quick LLM pass to remove filler words ("um", "uh"), add punctuation, and format appropriately before submitting.

#### Medium Priority
- [x] **Streaming local transcription** — Periodically run whisper on accumulated audio every 5s during recording, showing partial transcription in the status line.
- [x] **Model download progress bar** — Show download progress for first-use model downloads (75MB–1.5GB).
- [ ] **Editable transcription** — Let users review/edit transcribed text before auto-submitting.
- [ ] **Quantized models** — Support 8-bit quantized whisper models for lower memory usage and faster CPU inference.

#### Lower Priority
- [ ] **Multilingual support** — Switch from `.en` models to multilingual when `voice.language` is non-English.
- [ ] **Ambient noise calibration** — Auto-detect background noise level in first 0.5s instead of hardcoded -42dB threshold.
- [ ] **Wake word** — "Hey Kiro" hands-free activation.
- [ ] **Word-level timestamps** — WhisperX-style forced alignment for precise transcription segments.

## Audio Pipeline

### Recording
1. `cpal` opens default input device with native config
2. Audio callback converts to mono, resamples to 16kHz using linear interpolation
3. Output: 16-bit PCM little-endian bytes at 16kHz
4. Supports F32, I16, U16 sample formats

### Voice Activity Detection
- Silero VAD V5 neural model via `voice_activity_detector` crate
- Processes 512-sample windows at 16kHz, returns speech probability (0.0–1.0)
- Speech threshold: probability >= 0.5
- Falls back to RMS-based detection if VAD init fails (streaming mode)
- Auto-stop after configurable silence timeout (default: 3s) or max session time (default: 60s)

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

### Transcription (Local Whisper)
1. Collect all PCM bytes during recording
2. Convert i16 PCM to f32 samples (÷32768)
3. Load whisper model (e.g. ggml-base.en.bin, configurable via `voice.modelSize`)
4. Set `initial_prompt` from conversation context
5. Run `state.full(params, &samples)` on blocking thread
6. Extract text from all segments

## Dependencies

| Crate | Purpose | Size Impact |
|-------|---------|-------------|
| `cpal` 0.15.3 | Cross-platform audio capture | ~200KB (links to ALSA/CoreAudio) |
| `whisper-rs` 0.13.0 | Whisper.cpp Rust bindings | ~5MB (compiles whisper.cpp from source) |
| `voice_activity_detector` 0.2.1 | Silero VAD V5 neural speech detection | ~5MB (ONNX runtime) |
| `aws-sdk-transcribestreaming` | AWS Transcribe streaming | Already in workspace |

### Build Requirements
- C/C++ compiler (GCC 10+ on Linux, Xcode on macOS)
- CMake 3.14+
- libclang (for bindgen)
- ALSA dev headers on Linux (`alsa-lib-devel`)

## Usage

```bash
# Basic voice input (local whisper, default)
/voice

# Push-to-talk keybind
Ctrl+O

# AWS Transcribe streaming
/voice --model aws-transcribe

# Continuous conversation mode
/voice --continuous

# Stop continuous mode
/voice --continuous  (toggles off)
# or Ctrl+C
```

## Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `voice.language` | string | `en` | Voice input language |
| `voice.modelSize` | string | `base` | Whisper model: tiny, base, small, medium, turbo, turbo-q5 |
| `voice.silenceTimeout` | integer | `3` | Seconds of silence before auto-stop |
| `voice.maxSessionTime` | integer | `300` | Maximum recording duration in seconds |

## Testing

Voice mode requires a physical microphone, so automated testing is limited to:
- Unit tests for audio resampling logic
- Unit tests for WAV header generation
- Integration tests with pre-recorded audio files (future)

Manual testing checklist:
- [ ] `/voice` records and transcribes on macOS
- [ ] `/voice` records and transcribes on Linux with ALSA
- [ ] `/voice --model aws-transcribe` works with valid AWS credentials
- [ ] `/voice --continuous` loops correctly
- [ ] `Ctrl+O` triggers voice from prompt
- [ ] Graceful fallback when no microphone available
- [ ] Model auto-downloads on first use
- [ ] Context-aware transcription improves accuracy for code terms

## Acknowledgments

This implementation was inspired by and builds upon ideas from:

- **[Aider](https://aider.chat/docs/usage/voice.html)** — Pioneered the simple `/voice` → record → auto-submit pattern for AI coding assistants. Our single-line recording UI and auto-submit flow directly follow aider's design philosophy.
- **[VoiceMode for Claude Code](https://github.com/mbailey/voicemode)** — Demonstrated the MCP-based architecture for voice in terminal AI tools, including the remote agent pattern via WebSocket relay. Our planned MCP server architecture is inspired by this approach.
- **[whisper.cpp](https://github.com/ggerganov/whisper.cpp)** — The C/C++ port of OpenAI's Whisper model that makes local, fast transcription possible without Python. Used via the `whisper-rs` Rust bindings.
- **[cpal](https://github.com/RustAudio/cpal)** — Cross-platform audio I/O library for Rust that provides the foundation for our microphone capture across macOS, Linux, and Windows.
- **[OpenAI Whisper](https://github.com/openai/whisper)** — The original speech recognition model whose architecture and pre-trained weights power our local transcription.
- **[AWS Transcribe Streaming](https://docs.aws.amazon.com/transcribe/latest/dg/streaming.html)** — Real-time streaming transcription service used as our cloud-based provider option.

## References

- [Aider Voice Docs](https://aider.chat/docs/usage/voice.html)
- [VoiceMode GitHub](https://github.com/mbailey/voicemode)
- [whisper-rs Crate](https://codeberg.org/tazz4843/whisper-rs)
- [cpal Crate](https://github.com/RustAudio/cpal)
- [Distil-Whisper](https://github.com/huggingface/distil-whisper) — 6x faster, 50% smaller whisper variant
- [Silero VAD](https://github.com/snakers4/silero-vad) — Neural voice activity detection
- [LiveKit Agents](https://docs.livekit.io/agents/) — Reference for real-time voice agent architecture
- [Pipecat](https://github.com/pipecat-ai/pipecat) — Open-source voice pipeline framework
- [Kokoro TTS](https://github.com/hexgrad/kokoro) — Ultra-fast local TTS (potential future integration)
