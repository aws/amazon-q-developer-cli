---
doc_meta:
  title: /voice
  description: Start voice input using local Whisper speech-to-text transcription
  category: slash_command
  keywords: [voice, speech, whisper, microphone, dictation, ptt, push-to-talk, recording]
  related: [settings]
  validated: 2026-07-22
  commit: d544002bf
  status: validated
  testable_headless: false
---

## Overview

The `/voice` command starts a local voice recording session using your microphone. Speech is transcribed locally via OpenAI Whisper (ggml format via whisper.cpp) — no audio leaves your machine. The transcribed text is placed in the prompt input (or auto-submitted, depending on settings).

You can also hold **Space** in the prompt bar to start push-to-talk recording without typing the command.

## Usage

```
/voice
```

## First-Time Setup

On first use, Kiro needs to download the Whisper speech model (~148 MB for `base`, ~466 MB for `small`). The download does not happen silently:

1. `/voice` detects the model is missing
2. A confirmation panel appears showing the model name, size, and license (MIT)
3. Press **y** to confirm the download, **n** to cancel (or use arrow keys + Enter to select)
4. Once downloaded, the model is cached locally and voice is ready

If you trigger voice via Space (push-to-talk) while the model is missing, Kiro shows a hint to run `/voice` for the interactive setup.

## Model Licensing

The voice feature uses:

- **OpenAI Whisper** — MIT license ([source](https://github.com/openai/whisper/blob/main/LICENSE))
- **whisper.cpp** (ggml conversion) — MIT license ([source](https://github.com/ggml-org/whisper.cpp/blob/master/LICENSE))

License attribution is displayed during the download confirmation prompt and printed to stderr during CLI-mode downloads.

## Examples

### Start voice recording

```
/voice
```

Records until you stop (press Enter or the stop key). Transcribed text appears in the input.

### First-time download confirmation

```
/voice
```

A panel titled "Voice setup — download speech model?" appears with the model name, size, and license URL. Options:

```
(y) Yes, download the model    ~148MB, one-time
(n) No, not now
```

After confirming, an alert shows: `Downloading voice model — this runs once, then voice is ready.`

### Push-to-talk (Space hold)

Hold Space in the prompt bar to record. Release to stop and transcribe. If the model isn't downloaded yet, you'll see: `Voice needs a one-time model download. Type /voice to set it up.`

## Settings

Voice behavior is configured via `/settings`:

| Setting | Effect |
|---------|--------|
| `voice.modelSize` | Whisper model size (`base` or `small`) |
| `voice.language` | Language hint for transcription (`auto` for detection) |
| `voice.silenceTimeout` | Seconds of silence before auto-stop |
| `voice.autoSubmit` | Send transcription immediately vs. place in input |
| `voice.serverUrl` | Use a remote voice server instead of local Whisper |

## How It Works

1. Kiro spawns the CLI binary in voice-only mode (`kiro-cli voice`)
2. The binary records audio via the system microphone (cpal)
3. Audio is transcribed locally using whisper-rs
4. JSON events stream back: recording status, audio levels, partial text, final text
5. The TUI displays a live level indicator and partial transcription
6. Final text is placed in the prompt (or auto-submitted)

## Remote Server Mode

When `KIRO_VOICE_SERVER_URL` is set (or the `voice.serverUrl` setting is configured), the local binary is skipped entirely. Audio streams to the remote server's SSE endpoint for transcription — useful for cloud desktops without local GPU.

## Troubleshooting

### "Voice binary not found"

The voice feature requires the `kiro-cli` binary with the `voice` feature compiled in. Reinstall kiro-cli or check that `KIRO_CHAT_CLI_BIN` points to the correct binary.

### No speech detected

Ensure your microphone is working and permissions are granted. Try speaking louder or closer to the mic. Check that the correct input device is selected in your OS audio settings.

### Download confirmation doesn't appear

The interactive download prompt only appears in TUI mode via `/voice`. If you're running in headless/pipe mode, use the `--confirm-download` flag on the CLI command directly.

### Model download fails

Check your network connection. The model is downloaded from Hugging Face. If behind a proxy, ensure `HTTPS_PROXY` is set.

## Related

- [/settings](settings.md) — Configure voice model size, language, and auto-submit behavior
