---
doc_meta:
  title: Voice Mode
  description: Hands-free speech-to-text input for chat using local Whisper transcription
  category: feature
  keywords: [voice, speech, transcription, whisper, microphone, hands-free, dictation, audio, recording]
  related: [voice-command, voice-settings]
  validated: 2026-06-22
  commit: 4dbb4e51f
  status: validated
  testable_headless: false
---

# Voice Mode

Hands-free speech-to-text input for Kiro CLI using local Whisper transcription.

## Overview

Voice Mode enables you to speak your prompts instead of typing them. Your speech is transcribed locally using Whisper and automatically submitted to the AI assistant. This is useful for:

- Rapid prototyping and brainstorming
- Accessibility for users who prefer voice input
- Multitasking while coding
- Reducing context-switching friction

Voice Mode runs entirely locally by default—no cloud API keys required. The Whisper model downloads automatically on first use.

## How It Works

1. Trigger voice input with `/voice` or `Ctrl+O`
2. Speak your prompt while watching the recording indicator
3. Stop recording by pressing Enter or waiting for silence auto-stop
4. Transcription appears and is automatically submitted

```
Recording, press ENTER when done... 3.5sec ░░░█████████
```

The voice activity bar shows your audio level in real-time.

## Activation Methods

### Slash Command

```
/voice
```

Starts a single voice recording session.

### Push-to-Talk Keybind

Press `Ctrl+O` from the chat prompt to instantly start recording. This is the fastest way to use voice input.

### Push-to-Talk with Space Hold

Hold the `Space` key for 1.5 seconds to activate push-to-talk mode. Release `Space` to stop recording and transcribe. This is useful for quick voice inputs without needing to press Enter.

### Continuous Mode

```
/voice --continuous
```

Enables auto-recording after each assistant response. The chat automatically starts a new voice recording when returning to the prompt. Toggle off by running the command again or pressing `Ctrl+C`.

## Transcription Backends

### Local Whisper (Default)

Uses whisper-rs (Rust bindings for whisper.cpp) for fully local transcription:

- No internet required after initial model download
- Models stored in `~/.local/share/kiro/models/`
- Available sizes: `tiny` (~75MB), `base` (~142MB, default), `small` (~466MB), `medium` (~1.5GB), `turbo` (~1.62GB), `turbo-q5` (~574MB)

Configure the model size:

```bash
kiro-cli settings set voice.modelSize base
```

### Remote Voice Server (Cloud Desktops)

When running on a cloud desktop without a microphone, voice mode can use a remote server running on your local machine:

```bash
# On your local machine (with microphone):
kiro-cli voice-serve

# SSH to cloud desktop with reverse port forwarding:
ssh -R 19876:localhost:19876 cloud-desktop

# On the cloud desktop, configure the server URL:
kiro-cli settings set voice.serverUrl http://localhost:19876
```

Voice mode automatically falls back to the remote server when no local microphone is detected.

### One-Step Cloud Setup

For easier setup, use the `voice-cloud-setup` command from your local machine:

```bash
kiro-cli voice-cloud-setup <cloud-hostname>
```

This command:
1. Verifies SSH connectivity to the cloud desktop
2. Configures `voice.serverUrl` on the cloud desktop
3. Starts `voice-serve` locally
4. Opens an SSH reverse tunnel so the cloud desktop can reach the local voice server

Options:
- `--port <port>` - Port for voice server (default: 19876)
- `--remote-bin <path>` - Path to kiro-cli on cloud desktop
- `-i <identity>` - SSH identity file

## Voice Activity Detection

Voice Mode uses Silero VAD (Voice Activity Detection) to:

- Detect when you start and stop speaking
- Filter out keyboard clicks, fan noise, and other non-speech sounds
- Auto-stop recording after a configurable silence period

This prevents false transcriptions from background noise.

## Context-Aware Transcription

Voice Mode passes recent conversation context to Whisper's `initial_prompt` parameter. This improves accuracy for:

- Code identifiers and function names
- Technical terms discussed in the session
- Project-specific vocabulary

## Configuration

Configure voice settings with `kiro-cli settings`:

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `voice.language` | string | `en` | Voice input language |
| `voice.modelSize` | string | `base` | Whisper model size |
| `voice.silenceTimeout` | integer | `5` | Seconds of silence before auto-stop |
| `voice.maxSessionTime` | integer | `300` | Maximum recording duration in seconds |
| `voice.serverUrl` | string | none | Remote voice server URL for cloud desktops |
| `voice.autoSubmit` | boolean | `true` | Auto-submit transcription without review |

### Examples

```bash
# Use a larger model for better accuracy
kiro-cli settings set voice.modelSize small

# Increase silence timeout for slower speakers
kiro-cli settings set voice.silenceTimeout 8

# Set language for non-English transcription
kiro-cli settings set voice.language es
```

## Examples

### Basic Voice Input

```
> /voice
Recording, press ENTER when done... 2.1sec ░░░████████

"Fix the bug in the authentication module"

> Analyzing the authentication module...
```

### Push-to-Talk Workflow

1. Type your question partially: `How do I `
2. Press `Ctrl+O` to switch to voice
3. Speak: "implement pagination in the users API"
4. Press Enter to stop recording
5. Full prompt submitted: "How do I implement pagination in the users API"

### Continuous Conversation

```
> /voice --continuous
Continuous voice mode enabled

Recording, press ENTER when done... 1.8sec ░░░██████

"What files are in the src directory"

> The src directory contains...

Recording, press ENTER when done... 2.3sec ░░░████████

"Show me the main function"

> Here's the main function...
```

## Troubleshooting

### No microphone detected

Voice mode requires a microphone. On cloud desktops, set up the remote voice server:

```bash
# Local machine
kiro-cli voice-serve

# Cloud desktop
kiro-cli settings set voice.serverUrl http://localhost:19876
```

### Model download fails

The Whisper model downloads automatically on first use. If it fails:

1. Check internet connectivity
2. Ensure `~/.local/share/kiro/models/` is writable
3. Try a smaller model: `kiro-cli settings set voice.modelSize tiny`

### Poor transcription accuracy

- Use a larger model: `voice.modelSize small` or `medium`
- Speak clearly and at a moderate pace
- Reduce background noise
- The `turbo` model offers good accuracy with faster speed

### Recording doesn't stop

- Press Enter to manually stop
- Adjust `voice.silenceTimeout` if auto-stop triggers too early/late
- Check that your microphone isn't picking up constant background noise

### High CPU usage during transcription

Transcription is CPU-intensive. Options:

- Use a smaller model (`tiny` or `base`)
- Use the quantized model (`turbo-q5`)
- Wait for transcription to complete before starting new tasks

## Limitations

- Requires a physical microphone (or remote voice server for cloud desktops)
- Local transcription is CPU-bound; larger models are slower
- No GPU acceleration currently (CoreML/CUDA support planned)
- English models (`.en`) are used by default; multilingual support is limited
- No wake word activation ("Hey Kiro")

## Technical Details

### Audio Pipeline

1. **Capture**: cpal opens the default input device
2. **Resampling**: Audio converted to 16kHz mono PCM
3. **VAD**: Silero VAD filters non-speech audio
4. **Transcription**: whisper-rs processes the audio
5. **Submission**: Text inserted into chat input

### Build Requirements

Voice mode requires the `voice` feature flag at compile time:

- C/C++ compiler (GCC 10+ on Linux, Xcode on macOS)
- CMake 3.14+
- libclang (for bindgen)
- ALSA dev headers on Linux (`alsa-lib-devel`)

### Dependencies

| Component | Purpose |
|-----------|---------|
| cpal | Cross-platform audio capture |
| whisper-rs | Local Whisper transcription |
| voice_activity_detector | Silero VAD speech detection |

## Related

- [/voice command](../slash-commands/voice.md) - Slash command reference
- [Voice Settings](../settings/voice-settings.md) - Configuration options
