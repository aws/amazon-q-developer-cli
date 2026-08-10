---
doc_meta:
  validated: 2026-08-05
  commit: 792f78393
  status: draft
  testable_headless: false
  category: feature
  title: Voice Mode
  description: Hands-free speech-to-text input using local Whisper transcription, engine-agnostic across V2 and KAS
  keywords: [voice, speech, transcription, whisper, microphone, hands-free, dictation, audio, recording, ptt, push-to-talk, remote, cloud]
  related: [voice, settings]
---

## Overview

Voice Mode lets you speak your prompts instead of typing them. Speech is transcribed locally via OpenAI Whisper (ggml format via whisper.cpp) — no audio leaves your machine — and the resulting text is placed in the prompt input or auto-submitted, depending on your settings.

Voice Mode runs entirely locally by default; no cloud API keys are required. The Whisper model downloads once on first use, after an explicit confirmation prompt.

## Engine Behavior

Voice is a TUI-owned command. Capture and transcription happen entirely on the client — either the local `kiro-cli voice` binary or a configured remote voice server — and the transcribed text is injected into the prompt as ordinary input. The active agent engine never sees voice as a distinct capability; it receives the text exactly as if you had typed it.

Because of this, voice behaves identically under both the V2 and KAS (V3) engines. There are no engine-specific voice settings, commands, or capabilities.

## Activation

### Slash Command

```
/voice
```

Starts a single voice recording session. Transcribed text appears in the input (or is auto-submitted).

### Push-to-Talk (Space Hold)

Hold **Space** in the prompt bar to start recording. Release to stop and transcribe. If the model hasn't been downloaded yet, Kiro shows a hint to run `/voice` for the one-time interactive setup.

## First-Time Setup

On first use, Kiro needs to download the Whisper speech model (~148 MB for `base`, ~466 MB for `small`). The download is never silent:

1. `/voice` detects the model is missing.
2. A confirmation panel appears showing the model name, size, and license (MIT).
3. Press **y** to confirm the download, **n** to cancel.
4. Once downloaded, the model is cached locally and voice is ready.

## Model Licensing

- **OpenAI Whisper** — MIT license
- **whisper.cpp** (ggml conversion) — MIT license

Attribution is shown during the download confirmation prompt and printed to stderr during CLI-mode downloads.

## Model Integrity Verification

Kiro cryptographically verifies the Whisper model file against a pinned SHA-256 digest — both after downloading and before every use — to prevent loading a corrupt, tampered, or attacker-planted file from the local cache. On Unix, the model directory is restricted to owner-only access (mode 0700).

If verification fails on a cached model, Kiro deletes the invalid file and re-downloads automatically. No action is needed.

## Remote Voice Server (Cloud Desktops)

On a cloud desktop without a microphone, stream audio to a voice server running on your local machine. When `voice.serverUrl` (or the `KIRO_VOICE_SERVER_URL` environment variable) is set, the local binary is skipped entirely and audio streams to the remote server's SSE endpoint.

```bash
kiro-cli settings set voice.serverUrl http://localhost:19876
```

## Configuration

Voice behavior is controlled by settings. See [Voice Settings](../settings/voice-settings.md) for the full reference.

| Setting | Default | Effect |
|---------|---------|--------|
| `voice.modelSize` | `base` | Whisper model size (`base` or `small`) |
| `voice.language` | `en` | Language hint for transcription |
| `voice.silenceTimeout` | `5` | Seconds of silence before auto-stop |
| `voice.partialPause` | `500` | Pause in ms that triggers partial transcription |
| `voice.maxSessionTime` | `300` | Maximum recording duration in seconds |
| `voice.autoSubmit` | `true` | Auto-submit transcription vs. place in input |
| `voice.serverUrl` | none | Remote voice server URL for cloud desktops |

## How It Works

1. Kiro spawns the CLI binary in voice-only mode (`kiro-cli voice`), or connects to the remote voice server if configured.
2. The binary records audio via the system microphone (cpal).
3. Audio is transcribed locally using whisper-rs.
4. JSON events stream back: recording status, audio levels, partial text, final text.
5. The TUI shows a live level indicator and partial transcription.
6. Final text is placed in the prompt (or auto-submitted).

## Troubleshooting

### "Voice binary not found"

Voice requires the `kiro-cli` binary with the `voice` feature compiled in. Reinstall kiro-cli or check that `KIRO_CHAT_CLI_BIN` points to the correct binary.

### No microphone detected

Voice requires a microphone. On cloud desktops, configure a remote voice server via `voice.serverUrl`.

### No speech detected

Ensure your microphone is working and permissions are granted. Speak louder or closer to the mic, and confirm the correct input device is selected in your OS audio settings.

### Model integrity check failed

The cached model file doesn't match the expected SHA-256 digest. Kiro removes the invalid file and re-downloads automatically. If the error persists, check your network (a proxy may be altering the download) and try again.

## Related

- [/voice command](../slash-commands/voice.md) — Slash command reference
- [Voice Settings](../settings/voice-settings.md) — Configuration options
