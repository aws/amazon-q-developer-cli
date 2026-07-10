---
doc_meta:
  title: /voice
  description: Start voice input mode for hands-free speech-to-text prompts
  category: slash_command
  keywords: [voice, speech, microphone, recording, transcription, whisper, dictation]
  related: [settings]
  validated: 2026-07-07
  commit: 515ae058e
  status: validated
  testable_headless: false
---

# /voice

Start voice input mode for hands-free speech-to-text prompts.

## Overview

The `/voice` command activates voice recording. Speak your prompt, and it will be transcribed and submitted to the AI assistant. Recording stops when you press Enter or after a period of silence.

## Usage

```
/voice
/voice --continuous
/voice -c
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--continuous` | `-c` | Enable continuous voice mode (auto-record after each response) |

## Keybinds

- **Ctrl+O** — Start voice recording instantly from the prompt
- **Space (hold 1.5s)** — Push-to-talk; release to stop and transcribe

## Examples

### Basic Voice Input

```
> /voice
Recording, press ENTER when done... 1.5sec ░░░██████

"List all files in the current directory"

> Here are the files...
```

### Continuous Conversation

```
> /voice -c
Continuous voice mode enabled

Recording, press ENTER when done... 2.0sec ░░░████████

"What does the main function do"

> The main function...

Recording, press ENTER when done...
```

Recording automatically restarts after each assistant response. Toggle off by running `/voice -c` again or pressing `Ctrl+C`.

### Push-to-Talk with Ctrl+O

```
> [Ctrl+O pressed]
Recording, press ENTER when done... 3.2sec ░░░██████████

"Refactor this function to use async await"

> I'll refactor the function...
```

## How It Works

1. The TUI spawns the Rust binary in voice-only mode (or connects to a remote voice server)
2. Audio is captured from the microphone
3. Transcription runs locally via Whisper (or remotely if configured)
4. Transcribed text is auto-submitted to the assistant

### Remote Voice Server

On cloud desktops without microphone access, configure a remote voice server:

```bash
export KIRO_VOICE_SERVER_URL=http://localhost:19876
```

When set, the TUI streams from the remote server's SSE endpoint instead of spawning the local binary.

## Configuration

Voice behavior is controlled by settings:

```bash
# Adjust silence timeout (seconds before auto-stop)
kiro-cli settings set voice.silenceTimeout 8

# Change Whisper model size
kiro-cli settings set voice.modelSize small

# Set transcription language
kiro-cli settings set voice.language en
```

## Troubleshooting

### Command not available

If you don't see `/voice` in the command list, ensure you're running the latest version:

```bash
kiro-cli --version
```

### "No microphone detected"

Voice mode requires a microphone. On cloud desktops, configure a remote voice server via `KIRO_VOICE_SERVER_URL`.

### Recording stops too quickly

Increase the silence timeout:

```bash
kiro-cli settings set voice.silenceTimeout 10
```

### First use is slow

The Whisper model downloads on first use (~75MB–1.5GB depending on model size). Subsequent uses are instant.

## Limitations

- Requires microphone access (or remote voice server)
- Transcription is CPU-intensive
- English-optimized by default
- No edit/review before submission (auto-submits)

## Related

- [/settings](settings.md) — Configure voice settings (voice.silenceTimeout, voice.modelSize, voice.language)
